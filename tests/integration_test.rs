use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;
use tempfile::TempDir;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn cmd_with_toolshed_dir(dir: &str) -> Command {
    let mut cmd = Command::cargo_bin("toolshed").unwrap();
    cmd.env("TOOLSHED_DIR", dir);
    cmd
}

fn setup_fixture_toolshed() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().to_path_buf();
    let tools_dir = dir.join("tools");
    std::fs::create_dir_all(&tools_dir).unwrap();

    // Copy fixture tools
    let fixtures = fixtures_dir().join("tools");
    for entry in std::fs::read_dir(&fixtures).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let dest = tools_dir.join(&name);
        copy_dir_recursive(&entry.path(), &dest);
    }

    // Copy fixture skills if they exist
    let skills_fixtures = fixtures_dir().join("skills");
    if skills_fixtures.exists() {
        let skills_dir = dir.join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        for entry in std::fs::read_dir(&skills_fixtures).unwrap() {
            let entry = entry.unwrap();
            let dest = skills_dir.join(entry.file_name());
            copy_dir_recursive(&entry.path(), &dest);
        }
    }

    // Copy fixture agents if they exist
    let agents_fixtures = fixtures_dir().join("agents");
    if agents_fixtures.exists() {
        let agents_dir = dir.join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        for entry in std::fs::read_dir(&agents_fixtures).unwrap() {
            let entry = entry.unwrap();
            let dest = agents_dir.join(entry.file_name());
            copy_dir_recursive(&entry.path(), &dest);
        }
    }

    // Copy fixture rules if they exist
    let rules_fixtures = fixtures_dir().join("rules");
    if rules_fixtures.exists() {
        let rules_dir = dir.join("rules");
        std::fs::create_dir_all(&rules_dir).unwrap();
        for entry in std::fs::read_dir(&rules_fixtures).unwrap() {
            let entry = entry.unwrap();
            let dest = rules_dir.join(entry.file_name());
            copy_dir_recursive(&entry.path(), &dest);
        }
    }

    // Copy fixture workflows if they exist
    let workflows_fixtures = fixtures_dir().join("workflows");
    if workflows_fixtures.exists() {
        let workflows_dir = dir.join("workflows");
        std::fs::create_dir_all(&workflows_dir).unwrap();
        for entry in std::fs::read_dir(&workflows_fixtures).unwrap() {
            let entry = entry.unwrap();
            let dest = workflows_dir.join(entry.file_name());
            copy_dir_recursive(&entry.path(), &dest);
        }
    }

    (tmp, dir)
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let dest = dst.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir_recursive(&entry.path(), &dest);
        } else {
            std::fs::copy(&entry.path(), &dest).unwrap();
            // Preserve executable bit
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = std::fs::metadata(&entry.path()) {
                    let mode = meta.permissions().mode();
                    if mode & 0o111 != 0 {
                        let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(mode));
                    }
                }
            }
        }
    }
}

// ─── List Tests ─────────────────────────────────────────

#[test]
fn list_empty_toolshed() {
    let tmp = TempDir::new().unwrap();
    cmd_with_toolshed_dir(tmp.path().to_str().unwrap())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn list_categories() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("testing"))
        .stdout(predicate::str::contains("2 tools"));
}

#[test]
fn list_category_tools() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["list", "testing"])
        .assert()
        .success()
        .stdout(predicate::str::contains("echo"))
        .stdout(predicate::str::contains("failing"));
}

#[test]
fn list_nonexistent_category() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["list", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("category not found"));
}

// ─── Help Tests ─────────────────────────────────────────

#[test]
fn help_native_tool() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["help", "echo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("echo — Echo test tool"))
        .stdout(predicate::str::contains("Type: native"))
        .stdout(predicate::str::contains("say <message>"))
        .stdout(predicate::str::contains("greet <name>"));
}

#[test]
fn help_nonexistent_tool() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["help", "nope"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("tool not found"));
}

// ─── Run Tests (Native) ────────────────────────────────

#[test]
fn run_echo_say() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["run", "echo", "say", "hello world"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello world"));
}

#[test]
fn run_echo_greet() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["run", "echo", "greet", "Alice"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello Alice"));
}

#[test]
fn run_echo_greet_loud() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["run", "echo", "greet", "Alice", "--loud"])
        .assert()
        .success()
        .stdout(predicate::str::contains("HELLO Alice!"));
}

#[test]
fn run_failing_tool() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["run", "failing", "crash"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("something went wrong"));
}

#[test]
fn run_missing_command() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["run", "echo", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("command not found"));
}

#[test]
fn run_missing_arg() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["run", "echo", "say"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("missing required argument"));
}

// ─── Validate Tests ─────────────────────────────────────

#[test]
fn validate_all() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .arg("validate")
        .assert()
        .success()
        .stdout(predicate::str::contains("echo  ok"))
        .stdout(predicate::str::contains("failing  ok"));
}

#[test]
fn validate_specific() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["validate", "echo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("echo  ok"));
}

// ─── MCP Tests ──────────────────────────────────────────

fn setup_mcp_toolshed() -> (TempDir, PathBuf) {
    let (tmp, dir) = setup_fixture_toolshed();
    let mcp_dir = dir.join("tools/mock-mcp");
    std::fs::create_dir_all(&mcp_dir).unwrap();

    let mock_server = fixtures_dir().join("mcp_server_mock.py");
    let mock_path = std::fs::canonicalize(&mock_server).unwrap();

    let manifest = serde_json::json!({
        "name": "mock-mcp",
        "description": "Mock MCP server for testing",
        "category": "testing",
        "type": "mcp",
        "mcp": {
            "transport": "stdio",
            "command": "python3",
            "args": [mock_path.to_str().unwrap()]
        }
    });

    std::fs::write(
        mcp_dir.join("tool.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    (tmp, dir)
}

#[test]
fn help_mcp_tool() {
    let (_tmp, dir) = setup_mcp_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["help", "mock-mcp"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mock-mcp — Mock MCP server"))
        .stdout(predicate::str::contains("Type: mcp (stdio)"))
        .stdout(predicate::str::contains("echo"))
        .stdout(predicate::str::contains("add"));
}

#[test]
fn run_mcp_echo() {
    let (_tmp, dir) = setup_mcp_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["run", "mock-mcp", "echo", "--message", "hello from mcp"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello from mcp"));
}

#[test]
fn run_mcp_add() {
    let (_tmp, dir) = setup_mcp_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["run", "mock-mcp", "add", "--a", "3", "--b", "4"])
        .assert()
        .success()
        .stdout(predicate::str::contains("7"));
}

// ─── Status/Stop Stubs ─────────────────────────────────

#[test]
fn status_stub() {
    let tmp = TempDir::new().unwrap();
    cmd_with_toolshed_dir(tmp.path().to_str().unwrap())
        .arg("status")
        .assert()
        .success()
        .stderr(predicate::str::contains("not yet implemented"));
}

#[test]
fn stop_stub() {
    let tmp = TempDir::new().unwrap();
    cmd_with_toolshed_dir(tmp.path().to_str().unwrap())
        .arg("stop")
        .assert()
        .success()
        .stderr(predicate::str::contains("not yet implemented"));
}

// ─── Skill Tests ────────────────────────────────────────

#[test]
fn skill_list() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["skill", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test-skill"))
        .stdout(predicate::str::contains("A test skill for integration testing"));
}

#[test]
fn skill_show() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["skill", "show", "test-skill"])
        .assert()
        .success()
        .stdout(predicate::str::contains("# Test Skill"))
        .stdout(predicate::str::contains("Always test first"));
}

#[test]
fn skill_show_strips_frontmatter() {
    let (_tmp, dir) = setup_fixture_toolshed();
    let output = cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["skill", "show", "test-skill"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("name: test-skill"));
    assert!(!stdout.contains("---"));
}

#[test]
fn skill_validate() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["skill", "validate"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test-skill  ok"));
}

#[test]
fn skill_not_found() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["skill", "show", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("skill not found"));
}

// ─── Agent Tests ────────────────────────────────────────

#[test]
fn agent_list() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["agent", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test-agent"))
        .stdout(predicate::str::contains("A test agent for integration testing"))
        .stdout(predicate::str::contains("[sonnet]"));
}

#[test]
fn agent_show() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["agent", "show", "test-agent"])
        .assert()
        .success()
        .stdout(predicate::str::contains("You are a test agent"))
        .stdout(predicate::str::contains("Review code for correctness"));
}

#[test]
fn agent_show_strips_frontmatter() {
    let (_tmp, dir) = setup_fixture_toolshed();
    let output = cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["agent", "show", "test-agent"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("name: test-agent"));
    assert!(!stdout.contains("---"));
}

#[test]
fn agent_validate() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["agent", "validate"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test-agent  ok"));
}

#[test]
fn agent_not_found() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["agent", "show", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("agent not found"));
}

// ─── Rule Tests ─────────────────────────────────────────

#[test]
fn rule_list() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["rule", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test-rule"))
        .stdout(predicate::str::contains("guardrail"))
        .stdout(predicate::str::contains("error"))
        .stdout(predicate::str::contains("A test rule for integration testing"));
}

#[test]
fn rule_show() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["rule", "show", "test-rule"])
        .assert()
        .success()
        .stdout(predicate::str::contains("# No Force Push"))
        .stdout(predicate::str::contains("git push --force"));
}

#[test]
fn rule_show_strips_frontmatter() {
    let (_tmp, dir) = setup_fixture_toolshed();
    let output = cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["rule", "show", "test-rule"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("name: test-rule"));
    assert!(!stdout.contains("---"));
}

#[test]
fn rule_validate() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["rule", "validate"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test-rule  ok"));
}

#[test]
fn rule_not_found() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["rule", "show", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("rule not found"));
}

// ─── Workflow Tests ──────────────────────────────────────

#[test]
fn workflow_list() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["workflow", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test-workflow"))
        .stdout(predicate::str::contains("optional-step"))
        .stdout(predicate::str::contains("2 steps"));
}

#[test]
fn workflow_show() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["workflow", "show", "test-workflow"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test-workflow"))
        .stdout(predicate::str::contains("echo say hello"))
        .stdout(predicate::str::contains("echo say ${prev}"));
}

#[test]
fn workflow_validate() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["workflow", "validate"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test-workflow  ok"))
        .stdout(predicate::str::contains("optional-step  ok"));
}

#[test]
fn workflow_run() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["workflow", "run", "test-workflow"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello"));
}

#[test]
fn workflow_run_verbose() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["workflow", "run", "test-workflow", "--verbose"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello"))
        .stderr(predicate::str::contains("step 1 of 2"))
        .stderr(predicate::str::contains("step 2 of 2"));
}

#[test]
fn workflow_run_continue_on_error() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["workflow", "run", "optional-step"])
        .assert()
        .success()
        .stdout(predicate::str::contains("recovered"));
}

#[test]
fn workflow_not_found() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .args(["workflow", "show", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("workflow not found"));
}

// ─── Agent Prompt Includes Skills, Agents, and Rules ────

#[test]
fn agent_prompt_includes_skills_and_agents() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .arg("agent-prompt")
        .assert()
        .success()
        .stdout(predicate::str::contains("Available Skills"))
        .stdout(predicate::str::contains("test-skill"))
        .stdout(predicate::str::contains("Available Agents"))
        .stdout(predicate::str::contains("test-agent"));
}

#[test]
fn agent_prompt_includes_rules() {
    let (_tmp, dir) = setup_fixture_toolshed();
    cmd_with_toolshed_dir(dir.to_str().unwrap())
        .arg("agent-prompt")
        .assert()
        .success()
        .stdout(predicate::str::contains("## Rules"))
        .stdout(predicate::str::contains("MUST be followed"))
        .stdout(predicate::str::contains("test-rule"))
        .stdout(predicate::str::contains("No Force Push"));
}
