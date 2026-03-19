use std::{collections::BTreeMap, path::PathBuf};

use crate::{config, error::ToolshedError, manifest::ToolManifest};

#[derive(Debug, Clone)]
pub struct Tool {
    pub dir: PathBuf,
    pub manifest: ToolManifest,
    pub run_path: Option<PathBuf>,
}

#[derive(Debug)]
pub struct Registry {
    pub by_category: BTreeMap<String, Vec<String>>,
    pub tools: BTreeMap<String, Tool>,
    pub errors: Vec<(String, String)>,
}

impl Registry {
    pub fn load() -> Result<Self, ToolshedError> {
        let tools_dir = config::tools_dir();

        let mut registry = Self {
            by_category: BTreeMap::new(),
            tools: BTreeMap::new(),
            errors: Vec::new(),
        };

        if !tools_dir.exists() {
            return Ok(registry);
        }

        let entries = std::fs::read_dir(&tools_dir).map_err(|e| ToolshedError::NoToolshedDir {
            path: format!("{}: {e}", tools_dir.display()),
        })?;

        for entry in entries {
            let Ok(entry) = entry else {
                continue;
            };

            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let dir_name = dir_name.to_string();

            let manifest_path = path.join("tool.json");
            if !manifest_path.exists() {
                registry
                    .errors
                    .push((dir_name.clone(), "missing tool.json".to_string()));
                continue;
            }

            match ToolManifest::load_and_validate(&manifest_path, &dir_name) {
                Ok(manifest) => {
                    if manifest.health.is_none() {
                        registry.errors.push((
                            dir_name.clone(),
                            "tool manifest missing required health field".to_string(),
                        ));
                        continue;
                    }

                    let run_path = path.join("run");
                    let has_run = run_path.exists();

                    match manifest.tool_type {
                        crate::manifest::ToolType::Native => {
                            if !has_run {
                                registry
                                    .errors
                                    .push((dir_name.clone(), "missing 'run' script".to_string()));
                                continue;
                            }

                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                if let Ok(meta) = std::fs::metadata(&run_path) {
                                    if meta.permissions().mode() & 0o111 == 0 {
                                        registry.errors.push((
                                            dir_name.clone(),
                                            "'run' script is not executable".to_string(),
                                        ));
                                        continue;
                                    }
                                }
                            }

                            registry
                                .by_category
                                .entry(manifest.category.clone())
                                .or_default()
                                .push(dir_name.clone());

                            registry.tools.insert(
                                dir_name,
                                Tool {
                                    dir: path.clone(),
                                    manifest,
                                    run_path: Some(run_path),
                                },
                            );
                        }
                        crate::manifest::ToolType::Mcp => {
                            registry
                                .by_category
                                .entry(manifest.category.clone())
                                .or_default()
                                .push(dir_name.clone());

                            registry.tools.insert(
                                dir_name,
                                Tool {
                                    dir: path.clone(),
                                    manifest,
                                    run_path: None,
                                },
                            );
                        }
                    }
                }
                Err(e) => {
                    registry.errors.push((dir_name, e.to_string()));
                }
            }
        }

        Ok(registry)
    }
}
