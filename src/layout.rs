//! Which entries of `~/.claude` a profile links, and keeping those links in sync.

use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::symlink;
use std::path::Path;

use anyhow::{Context, Result};

/// What an entry of `~/.claude` holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Bound to the signed-in account; never shared between profiles.
    Account,
    /// Sessions, history and caches: shared by shared profiles, local to isolated ones.
    State,
    /// Settings, instructions, skills, hooks, plugins: replicated unless the profile is bare.
    Config,
}

/// Entries tied to the logged-in account or to a running service.
const ACCOUNT: &[&str] = &[
    ".credentials.json",
    ".claude.json",
    ".claude.json.backup",
    ".session_ingress_token",
    "hfi-auth.json",
    "policy-limits.json",
    "remote-settings.json",
    "mcp-needs-auth-cache.json",
    "statsig",
    "daemon",
    "daemon.json",
    "daemon.log",
    "jobs",
    "server-sessions.json",
    "stats-cache.json",
    "usage-data",
    "telemetry",
    "cache",
    "remote-control",
    "bridge-spawn",
    "ccr",
];

/// Runtime state, following the list Claude Code itself excludes from config snapshots.
const STATE: &[&str] = &[
    "projects",
    "sessions",
    "todos",
    "shell-snapshots",
    "file-history",
    "history.jsonl",
    "ide",
    "logs",
    "backups",
    "teams",
    "shares",
    "state",
    "uploads",
    "feedback",
    "feedback-bundles",
    "plans",
    "dump-prompts",
    "debug",
    "traces",
    "startup-perf",
    "mcp-discovery-cache",
    "gh-pr-status-cache.json",
    "tasks",
    "local",
    "antproto.json",
    "session-env",
    "active-time.json",
    "loop.md",
    "image-cache",
    "paste-cache",
    "file-transfers",
    "mcp-skill-archives",
    "api-dumps",
    "chrome",
    "downloads",
    "local-settings",
    "project-settings",
    "remote",
    "scratch",
    "seed-admin",
    "storage-v2",
    "systemd",
];

pub fn classify(name: &str) -> Kind {
    if ACCOUNT.contains(&name) || name.ends_with(".lock") {
        Kind::Account
    } else if STATE.contains(&name) {
        Kind::State
    } else {
        Kind::Config
    }
}

#[derive(Debug, Default)]
pub struct SyncReport {
    pub linked: Vec<String>,
    pub pruned: Vec<String>,
    /// Entries that should be links into `~/.claude` but are not (anymore).
    pub diverged: Vec<String>,
}

/// Links every entry of `source` that `wanted` accepts into `target`, and removes
/// links into `source` that are dangling or no longer wanted.
///
/// Only symlinks are ever removed: a real file or directory in `target` is reported
/// as diverged and left untouched.
pub fn sync(source: &Path, target: &Path, wanted: impl Fn(&str) -> bool) -> Result<SyncReport> {
    let mut report = SyncReport::default();

    if source.is_dir() {
        for entry in read_dir(source)? {
            let name = entry.file_name();
            let Some(name_str) = name.to_str() else {
                continue;
            };
            if !wanted(name_str) {
                continue;
            }
            let origin = source.join(&name);
            let link = target.join(&name);
            match fs::symlink_metadata(&link) {
                Err(err) if err.kind() == ErrorKind::NotFound => {
                    symlink(&origin, &link)
                        .with_context(|| format!("linking {}", link.display()))?;
                    report.linked.push(name_str.to_owned());
                }
                Err(err) => {
                    return Err(err).with_context(|| format!("inspecting {}", link.display()));
                }
                Ok(meta) if meta.file_type().is_symlink() => {
                    if fs::read_link(&link)? != origin {
                        report
                            .diverged
                            .push(format!("{name_str} (links elsewhere)"));
                    }
                }
                Ok(_) => report.diverged.push(name_str.to_owned()),
            }
        }
    }

    for entry in read_dir(target)? {
        let link = entry.path();
        if !fs::symlink_metadata(&link)?.file_type().is_symlink() {
            continue;
        }
        let origin = fs::read_link(&link)?;
        if !origin.starts_with(source) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if fs::symlink_metadata(&origin).is_err() || !wanted(&name) {
            fs::remove_file(&link).with_context(|| format!("unlinking {}", link.display()))?;
            report.pruned.push(name);
        }
    }

    report.linked.sort();
    report.pruned.sort();
    report.diverged.sort();
    Ok(report)
}

fn read_dir(dir: &Path) -> Result<Vec<fs::DirEntry>> {
    fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("reading {}", dir.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_classify_credentials_as_account_when_named_so() {
        assert_eq!(classify(".credentials.json"), Kind::Account);
        assert_eq!(classify("server.lock"), Kind::Account);
    }

    #[test]
    fn test_should_classify_unknown_entries_as_config_when_not_listed() {
        assert_eq!(classify("skills"), Kind::Config);
        assert_eq!(classify("settings.json"), Kind::Config);
        assert_eq!(classify("something-new"), Kind::Config);
    }

    #[test]
    fn test_should_classify_history_as_state_when_listed() {
        assert_eq!(classify("projects"), Kind::State);
        assert_eq!(classify("history.jsonl"), Kind::State);
    }
}
