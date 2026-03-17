use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::config;
use crate::error::ToolshedError;
use crate::frontmatter;
use crate::manifest;

const VALID_TYPES: &[&str] = &["guardrail", "permission", "validation", "context"];
const VALID_SEVERITIES: &[&str] = &["error", "warning", "info"];

#[derive(Debug, Clone)]
pub struct RuleManifest {
    pub name: String,
    pub description: String,
    pub rule_type: String,
    pub severity: String,
    pub scope: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub dir: PathBuf,
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

            let dir_name = match entry.file_name().to_str() {
                Some(n) => n.to_string(),
                None => continue,
            };

            match load_rule(&path, &dir_name) {
                Ok(rule) => {
                    rules.insert(dir_name, rule);
                }
                Err(e) => {
                    errors.push((dir_name, e.to_string()));
                }
            }
        }

        Ok(Self { rules, errors })
    }
}

fn load_rule(dir: &PathBuf, dir_name: &str) -> Result<Rule, ToolshedError> {
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
            "name '{}' does not match directory '{dir_name}'",
            name
        )));
    }

    // Validate name format
    if !manifest::is_valid_name(name) {
        return Err(err(format!(
            "name must be 1-64 chars, [a-z0-9_-] only, got '{}'",
            name
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
            "type must be one of {:?}, got '{}'",
            VALID_TYPES, rule_type
        )));
    }

    // Parse and validate severity (default: error)
    let severity = meta
        .get("severity")
        .cloned()
        .unwrap_or_else(|| "error".to_string());
    if !VALID_SEVERITIES.contains(&severity.as_str()) {
        return Err(err(format!(
            "severity must be one of {:?}, got '{}'",
            VALID_SEVERITIES, severity
        )));
    }

    // Parse and validate scope (default: global)
    let scope: Vec<String> = meta
        .get("scope")
        .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
        .unwrap_or_else(|| vec!["global".to_string()]);

    for entry in &scope {
        if !is_valid_scope(entry) {
            return Err(err(format!(
                "invalid scope '{}' — must be 'global' or 'type:name' (tool:x, agent:x, category:x)",
                entry
            )));
        }
    }

    // Body must not be empty
    if body.trim().is_empty() {
        return Err(err("rule body cannot be empty".to_string()));
    }

    Ok(Rule {
        dir: dir.clone(),
        manifest: RuleManifest {
            name: name.clone(),
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
