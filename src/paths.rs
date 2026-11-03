use std::path::PathBuf;

use anyhow::{Context, Result, bail};

/// Filesystem locations ccsp works with.
pub struct Paths {
    /// The global Claude Code directory (`~/.claude`), source of every shared link.
    pub claude_home: PathBuf,
    /// The global config file (`~/.claude.json`), source of the seeded copy.
    pub claude_json: PathBuf,
    /// Where ccsp keeps its state and profiles.
    pub root: PathBuf,
}

impl Paths {
    pub fn from_env() -> Result<Self> {
        let home = non_empty_env("HOME").context("HOME is not set")?;
        let root = match non_empty_env("CCSP_HOME") {
            Some(root) => root,
            None => match non_empty_env("XDG_DATA_HOME") {
                Some(data) => data.join("ccsp"),
                None => home.join(".local/share/ccsp"),
            },
        };
        if !root.is_absolute() {
            bail!("ccsp home {} is not an absolute path", root.display());
        }
        Ok(Self {
            claude_home: home.join(".claude"),
            claude_json: home.join(".claude.json"),
            root,
        })
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.root.join("profiles")
    }

    pub fn state_file(&self) -> PathBuf {
        self.root.join("state.toml")
    }
}

fn non_empty_env(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}
