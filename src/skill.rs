use std::{collections::BTreeMap, path::Path};

use crate::{config, error::ToolshedError, frontmatter, manifest};

#[derive(Debug, Clone)]
pub struct SkillManifest {
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct Skill {
    pub manifest: SkillManifest,
    pub body: String,
}

pub struct SkillRegistry {
    pub skills: BTreeMap<String, Skill>,
    pub errors: Vec<(String, String)>,
}

impl SkillRegistry {
    pub fn load() -> Result<Self, ToolshedError> {
        let dir = config::skills_dir();
        let mut skills = BTreeMap::new();
        let mut errors = Vec::new();

        if !dir.exists() {
            return Ok(Self { skills, errors });
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

            match load_skill(&path, dir_name) {
                Ok(skill) => {
                    skills.insert(dir_name.to_string(), skill);
                }
                Err(e) => {
                    errors.push((dir_name.to_string(), e.to_string()));
                }
            }
        }

        Ok(Self { skills, errors })
    }
}

fn load_skill(dir: &Path, dir_name: &str) -> Result<Skill, ToolshedError> {
    let skill_md = dir.join("SKILL.md");

    if !skill_md.exists() {
        return Err(ToolshedError::BadSkill {
            skill: dir_name.to_string(),
            reason: "missing SKILL.md".to_string(),
        });
    }

    let content = std::fs::read_to_string(&skill_md).map_err(|e| ToolshedError::BadSkill {
        skill: dir_name.to_string(),
        reason: format!("cannot read SKILL.md: {e}"),
    })?;

    let (meta, body) = frontmatter::parse(&content)?;

    let name = meta.get("name").ok_or_else(|| ToolshedError::BadSkill {
        skill: dir_name.to_string(),
        reason: "frontmatter missing 'name'".to_string(),
    })?;

    let description = meta
        .get("description")
        .ok_or_else(|| ToolshedError::BadSkill {
            skill: dir_name.to_string(),
            reason: "frontmatter missing 'description'".to_string(),
        })?;

    // Validate name matches directory
    if name != dir_name {
        return Err(ToolshedError::BadSkill {
            skill: dir_name.to_string(),
            reason: format!("name '{name}' does not match directory '{dir_name}'"),
        });
    }

    // Validate name format
    if !manifest::is_valid_name(name) {
        return Err(ToolshedError::BadSkill {
            skill: dir_name.to_string(),
            reason: format!("name must be 1-64 chars, [a-z0-9_-] only, got '{name}'"),
        });
    }

    // Validate description length
    if description.is_empty() {
        return Err(ToolshedError::BadSkill {
            skill: dir_name.to_string(),
            reason: "description cannot be empty".to_string(),
        });
    }
    if description.len() > 500 {
        return Err(ToolshedError::BadSkill {
            skill: dir_name.to_string(),
            reason: "description exceeds 500 chars".to_string(),
        });
    }

    Ok(Skill {
        manifest: SkillManifest {
            description: description.clone(),
        },
        body,
    })
}
