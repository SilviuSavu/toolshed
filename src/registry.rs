use crate::config;
use crate::error::ToolshedError;
use crate::manifest::ToolManifest;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug)]
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

        let mut registry = Registry {
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
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let dir_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            let manifest_path = path.join("tool.json");
            if !manifest_path.exists() {
                registry.errors.push((
                    dir_name.clone(),
                    "missing tool.json".to_string(),
                ));
                continue;
            }

            match ToolManifest::load_and_validate(&manifest_path, &dir_name) {
                Ok(manifest) => {
                    let run_path = path.join("run");
                    let has_run = run_path.exists();

                    match manifest.tool_type {
                        crate::manifest::ToolType::Native => {
                            if !has_run {
                                registry.errors.push((
                                    dir_name.clone(),
                                    "missing 'run' script".to_string(),
                                ));
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
