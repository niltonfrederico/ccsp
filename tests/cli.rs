use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

/// A fake HOME with a populated `~/.claude` and `~/.claude.json`.
struct Home {
    dir: TempDir,
}

impl Home {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join(".claude");
        for sub in ["skills", "projects", "statsig", "ctx"] {
            fs::create_dir_all(claude.join(sub)).unwrap();
        }
        for file in [
            "settings.json",
            "CLAUDE.md",
            "history.jsonl",
            ".credentials.json",
        ] {
            fs::write(claude.join(file), "{}").unwrap();
        }
        fs::write(
            dir.path().join(".claude.json"),
            r#"{"oauthAccount":{"emailAddress":"me@x"},"mcpServers":{"a":{}},"cachedFoo":1,"projects":{}}"#,
        )
        .unwrap();
        Self { dir }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn profile_home(&self, name: &str) -> PathBuf {
        self.path()
            .join(".local/share/ccsp/profiles")
            .join(name)
            .join("home")
    }

    fn ccsp(&self) -> Command {
        let mut cmd = Command::cargo_bin("ccsp").unwrap();
        cmd.env("HOME", self.path())
            .env("XDG_DATA_HOME", self.path().join(".local/share"))
            .env_remove("CCSP_HOME")
            .env_remove("CCSP_PROFILE")
            .env_remove("CCSP_SWITCHED")
            .env_remove("CLAUDE_CONFIG_DIR");
        cmd
    }

    fn create(&self, args: &[&str]) {
        self.ccsp()
            .arg("create")
            .args(args)
            .arg("--no-login")
            .assert()
            .success();
    }
}

fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

#[test]
fn test_should_link_everything_but_account_files_when_shared() {
    let home = Home::new();
    home.create(&["work"]);
    let profile = home.profile_home("work");

    for linked in [
        "skills",
        "projects",
        "settings.json",
        "CLAUDE.md",
        "history.jsonl",
        "ctx",
    ] {
        assert!(is_link(&profile.join(linked)), "{linked} should be linked");
    }
    assert!(!profile.join(".credentials.json").exists());
    assert!(!profile.join("statsig").exists());
    assert!(!is_link(&profile.join(".claude.json")));
}

#[test]
fn test_should_link_only_config_when_isolated() {
    let home = Home::new();
    home.create(&["work", "--isolated"]);
    let profile = home.profile_home("work");

    assert!(is_link(&profile.join("skills")));
    assert!(is_link(&profile.join("settings.json")));
    assert!(!profile.join("projects").exists());
    assert!(!profile.join("history.jsonl").exists());
    assert!(profile.join(".claude.json").is_file());
}

#[test]
fn test_should_link_nothing_when_bare() {
    let home = Home::new();
    home.create(&["work", "--isolated", "--bare"]);
    let entries: Vec<_> = fs::read_dir(home.profile_home("work")).unwrap().collect();
    assert!(entries.is_empty(), "bare profile should start empty");
}

#[test]
fn test_should_reject_bare_when_not_isolated() {
    let home = Home::new();
    home.ccsp()
        .args(["create", "work", "--bare", "--no-login"])
        .assert()
        .failure();
}

#[test]
fn test_should_strip_account_keys_when_seeding_claude_json() {
    let home = Home::new();
    home.create(&["work"]);
    let raw = fs::read_to_string(home.profile_home("work").join(".claude.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(json.get("oauthAccount").is_none());
    assert!(json.get("cachedFoo").is_none());
    assert!(json.get("mcpServers").is_some());
}

#[test]
fn test_should_refuse_duplicate_when_profile_exists() {
    let home = Home::new();
    home.create(&["work"]);
    home.ccsp()
        .args(["create", "work", "--no-login"])
        .assert()
        .failure()
        .stderr(contains("already exists"));
}

#[test]
fn test_should_reject_name_when_it_has_a_slash() {
    let home = Home::new();
    home.ccsp()
        .args(["create", "../evil", "--no-login"])
        .assert()
        .failure();
    assert!(!home.path().join(".local/share/ccsp/evil").exists());
}

#[test]
fn test_should_emit_default_env_when_shell_starts() {
    let home = Home::new();
    home.create(&["work"]);
    home.ccsp().args(["set-default", "work"]).assert().success();
    let expected = format!(
        "set -gx CLAUDE_CONFIG_DIR '{}'",
        home.profile_home("work").display()
    );
    home.ccsp()
        .args(["env", "--shell", "fish"])
        .assert()
        .success()
        .stdout(contains(expected))
        .stdout(contains("set -e CCSP_SWITCHED"));
}

#[test]
fn test_should_keep_switched_profile_when_default_differs() {
    let home = Home::new();
    home.create(&["work"]);
    home.create(&["personal"]);
    home.ccsp()
        .args(["set-default", "personal"])
        .assert()
        .success();
    home.ccsp()
        .args(["env", "--shell", "bash"])
        .env("CCSP_PROFILE", "work")
        .env("CCSP_SWITCHED", "1")
        .assert()
        .success()
        .stdout(contains("export CCSP_PROFILE='work'"))
        .stdout(contains("export CCSP_SWITCHED=1"));
}

#[test]
fn test_should_print_switched_env_when_switching_with_shell() {
    let home = Home::new();
    home.create(&["work"]);
    home.ccsp()
        .args(["switch", "work", "--shell", "bash"])
        .assert()
        .success()
        .stdout(contains("export CCSP_SWITCHED=1"));
}

#[test]
fn test_should_explain_integration_when_switching_without_shell() {
    let home = Home::new();
    home.create(&["work"]);
    home.ccsp()
        .args(["switch", "work"])
        .assert()
        .failure()
        .stderr(contains("ccsp init fish"));
}

#[test]
fn test_should_leave_foreign_config_dir_when_no_default() {
    let home = Home::new();
    home.ccsp()
        .args(["env", "--shell", "bash"])
        .assert()
        .success()
        .stdout("");
}

#[test]
fn test_should_clear_env_when_ccsp_profile_has_no_default() {
    let home = Home::new();
    home.ccsp()
        .args(["env", "--shell", "bash"])
        .env("CCSP_PROFILE", "gone")
        .assert()
        .success()
        .stdout(contains("unset CLAUDE_CONFIG_DIR"));
}

#[test]
fn test_should_trash_profile_and_clear_default_when_deleting() {
    let home = Home::new();
    home.create(&["work"]);
    home.ccsp().args(["set-default", "work"]).assert().success();
    home.ccsp()
        .args(["delete", "work", "--yes"])
        .assert()
        .success();

    assert!(!home.profile_home("work").exists());
    assert!(
        home.path().join(".claude/skills").is_dir(),
        "~/.claude must survive"
    );
    let trashed = home.path().join(".local/share/Trash/files/work");
    assert!(
        trashed.join("home").is_dir(),
        "profile should be in the trash"
    );
    let state = fs::read_to_string(home.path().join(".local/share/ccsp/state.toml")).unwrap();
    assert!(!state.contains("work"));
}

#[test]
fn test_should_refuse_delete_when_not_confirmed_without_terminal() {
    let home = Home::new();
    home.create(&["work"]);
    home.ccsp()
        .args(["delete", "work"])
        .assert()
        .failure()
        .stderr(contains("--yes"));
    assert!(home.profile_home("work").exists());
}

#[test]
fn test_should_link_new_entry_when_syncing() {
    let home = Home::new();
    home.create(&["work"]);
    fs::create_dir(home.path().join(".claude/agents")).unwrap();
    home.ccsp()
        .arg("sync")
        .assert()
        .success()
        .stderr(contains("linked agents"));
    assert!(is_link(&home.profile_home("work").join("agents")));
}

#[test]
fn test_should_prune_link_when_source_is_removed() {
    let home = Home::new();
    home.create(&["work"]);
    fs::remove_file(home.path().join(".claude/CLAUDE.md")).unwrap();
    home.ccsp()
        .arg("sync")
        .assert()
        .success()
        .stderr(contains("removed stale link CLAUDE.md"));
    assert!(fs::symlink_metadata(home.profile_home("work").join("CLAUDE.md")).is_err());
}

#[test]
fn test_should_warn_and_keep_file_when_link_was_replaced() {
    let home = Home::new();
    home.create(&["work"]);
    let settings = home.profile_home("work").join("settings.json");
    fs::remove_file(&settings).unwrap();
    fs::write(&settings, "local").unwrap();
    home.ccsp()
        .arg("sync")
        .assert()
        .success()
        .stderr(contains("no longer a link"));
    assert_eq!(fs::read_to_string(&settings).unwrap(), "local");
}

#[test]
fn test_should_mark_default_when_listing() {
    let home = Home::new();
    home.create(&["work", "--isolated"]);
    home.ccsp().args(["set-default", "work"]).assert().success();
    home.ccsp()
        .arg("list")
        .assert()
        .success()
        .stdout(contains("*  work"));
}
