use std::{collections::BTreeMap, path::Path};

use crate::{config, error::ToolshedError, frontmatter, manifest};

#[derive(Debug, Clone)]
pub struct AgentManifest {
    pub description: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Agent {
    pub manifest: AgentManifest,
    pub prompt: String,
}

pub struct AgentRegistry {
    pub agents: BTreeMap<String, Agent>,
    pub errors: Vec<(String, String)>,
}

impl AgentRegistry {
    pub fn load() -> Result<Self, ToolshedError> {
        let dir = config::agents_dir();
        let mut agents = BTreeMap::new();
        let mut errors = Vec::new();

        if !dir.exists() {
            return Ok(Self { agents, errors });
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

            match load_agent(&path, dir_name) {
                Ok(agent) => {
                    agents.insert(dir_name.to_string(), agent);
                }
                Err(e) => {
                    errors.push((dir_name.to_string(), e.to_string()));
                }
            }
        }

        Ok(Self { agents, errors })
    }
}

fn load_agent(dir: &Path, dir_name: &str) -> Result<Agent, ToolshedError> {
    let agent_md = dir.join("AGENT.md");

    if !agent_md.exists() {
        return Err(ToolshedError::BadAgent {
            agent: dir_name.to_string(),
            reason: "missing AGENT.md".to_string(),
        });
    }

    let content = std::fs::read_to_string(&agent_md).map_err(|e| ToolshedError::BadAgent {
        agent: dir_name.to_string(),
        reason: format!("cannot read AGENT.md: {e}"),
    })?;

    let (meta, body) = frontmatter::parse(&content)?;

    let name = meta.get("name").ok_or_else(|| ToolshedError::BadAgent {
        agent: dir_name.to_string(),
        reason: "frontmatter missing 'name'".to_string(),
    })?;

    let description = meta
        .get("description")
        .ok_or_else(|| ToolshedError::BadAgent {
            agent: dir_name.to_string(),
            reason: "frontmatter missing 'description'".to_string(),
        })?;

    let model = meta.get("model").cloned();

    // Validate name matches directory
    if name != dir_name {
        return Err(ToolshedError::BadAgent {
            agent: dir_name.to_string(),
            reason: format!("name '{name}' does not match directory '{dir_name}'"),
        });
    }

    // Validate name format
    if !manifest::is_valid_name(name) {
        return Err(ToolshedError::BadAgent {
            agent: dir_name.to_string(),
            reason: format!("name must be 1-64 chars, [a-z0-9_-] only, got '{name}'"),
        });
    }

    // Validate description length
    if description.is_empty() {
        return Err(ToolshedError::BadAgent {
            agent: dir_name.to_string(),
            reason: "description cannot be empty".to_string(),
        });
    }
    if description.len() > 200 {
        return Err(ToolshedError::BadAgent {
            agent: dir_name.to_string(),
            reason: "description exceeds 200 chars".to_string(),
        });
    }

    // Validate prompt is not empty
    if body.trim().is_empty() {
        return Err(ToolshedError::BadAgent {
            agent: dir_name.to_string(),
            reason: "agent prompt (body) cannot be empty".to_string(),
        });
    }

    // Validate model if present
    if let Some(ref m) = model {
        if m.is_empty() {
            return Err(ToolshedError::BadAgent {
                agent: dir_name.to_string(),
                reason: "model field cannot be empty if present".to_string(),
            });
        }
    }

    Ok(Agent {
        manifest: AgentManifest {
            description: description.clone(),
            model,
        },
        prompt: body,
    })
}
