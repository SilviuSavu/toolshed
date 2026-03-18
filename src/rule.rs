use std::{collections::BTreeMap, path::Path};

use crate::{config, error::ToolshedError, frontmatter, manifest};

const VALID_TYPES: &[&str] = &["guardrail", "permission", "validation", "context"];
const VALID_SEVERITIES: &[&str] = &["error", "warning", "info"];

#[derive(Debug, Clone)]
pub struct RuleManifest {
    pub description: String,
    pub rule_type: String,
    pub severity: String,
    pub scope: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub manifest: RuleManifest,
    pub body: String,
}

pub struct RuleRegistry {
    pub rules: BTreeMap<String, Rule>,
    pub errors: Vec<(String, String)>,
}

impl RuleRegistry {
    pub fn load() -> Result<Self, ToolshedError> {
        let dir = config::rules_dir();
        let mut rules = BTreeMap::new();
        let mut errors = Vec::new();

        if !dir.exists() {
            return Ok(Self { rules, errors });
        }

        let entries = std::fs::read_dir(&dir).map_err(ToolshedError::Io)?;

        for entry in entries {
            let entry = entry.map_err(ToolshedError::Io)?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let file_name = entry.file_name();
            let Some(dir_name) = file_name.to_str() else {
                continue;
            };

            match load_rule(&path, dir_name) {
                Ok(rule) => {
                    rules.insert(dir_name.to_string(), rule);
                }
                Err(e) => {
                    errors.push((dir_name.to_string(), e.to_string()));
                }
            }
        }

        Ok(Self { rules, errors })
    }
}

fn load_rule(dir: &Path, dir_name: &str) -> Result<Rule, ToolshedError> {
    let rule_md = dir.join("RULE.md");

    if !rule_md.exists() {
        return Err(ToolshedError::BadRule {
            rule: dir_name.to_string(),
            reason: "missing RULE.md".to_string(),
        });
    }

    let content = std::fs::read_to_string(&rule_md).map_err(|e| ToolshedError::BadRule {
        rule: dir_name.to_string(),
        reason: format!("cannot read RULE.md: {e}"),
    })?;

    let (meta, body) = frontmatter::parse(&content)?;

    let err = |reason: String| ToolshedError::BadRule {
        rule: dir_name.to_string(),
        reason,
    };

    let name = meta
        .get("name")
        .ok_or_else(|| err("frontmatter missing 'name'".to_string()))?;

    let description = meta
        .get("description")
        .ok_or_else(|| err("frontmatter missing 'description'".to_string()))?;

    // Validate name matches directory
    if name != dir_name {
        return Err(err(format!(
            "name '{name}' does not match directory '{dir_name}'"
        )));
    }

    // Validate name format
    if !manifest::is_valid_name(name) {
        return Err(err(format!(
            "name must be 1-64 chars, [a-z0-9_-] only, got '{name}'"
        )));
    }

    // Validate description
    if description.is_empty() {
        return Err(err("description cannot be empty".to_string()));
    }
    if description.len() > 300 {
        return Err(err("description exceeds 300 chars".to_string()));
    }

    // Parse and validate type (default: guardrail)
    let rule_type = meta
        .get("type")
        .cloned()
        .unwrap_or_else(|| "guardrail".to_string());
    if !VALID_TYPES.contains(&rule_type.as_str()) {
        return Err(err(format!(
            "type must be one of {VALID_TYPES:?}, got '{rule_type}'"
        )));
    }

    // Parse and validate severity (default: error)
    let severity = meta
        .get("severity")
        .cloned()
        .unwrap_or_else(|| "error".to_string());
    if !VALID_SEVERITIES.contains(&severity.as_str()) {
        return Err(err(format!(
            "severity must be one of {VALID_SEVERITIES:?}, got '{severity}'"
        )));
    }

    // Parse and validate scope (default: global)
    let scope: Vec<String> = meta.get("scope").map_or_else(
        || vec!["global".to_string()],
        |s| s.split(',').map(|p| p.trim().to_string()).collect(),
    );

    for entry in &scope {
        if !is_valid_scope(entry) {
            return Err(err(format!(
                "invalid scope '{entry}' — must be 'global' or 'type:name' (tool:x, agent:x, \
                 category:x)"
            )));
        }
    }

    // Body must not be empty
    if body.trim().is_empty() {
        return Err(err("rule body cannot be empty".to_string()));
    }

    Ok(Rule {
        manifest: RuleManifest {
            description: description.clone(),
            rule_type,
            severity,
            scope,
        },
        body,
    })
}

fn is_valid_scope(s: &str) -> bool {
    if s == "global" {
        return true;
    }
    let valid_prefixes = ["tool:", "agent:", "category:"];
    for prefix in &valid_prefixes {
        if let Some(name) = s.strip_prefix(prefix) {
            return manifest::is_valid_name(name);
        }
    }
    false
}
