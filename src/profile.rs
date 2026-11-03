//! Profiles on disk: `<root>/profiles/<name>/{meta.toml,home/}`.
//!
//! `home/` is what `CLAUDE_CONFIG_DIR` points at for the profile.

use std::fs::{self, DirBuilder};
use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::claude_json;
use crate::layout::{self, Kind, SyncReport};
use crate::paths::Paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Uses the global `~/.claude`; only the account is its own.
    Shared,
    /// Has its own sessions and history.
    Isolated,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Meta {
    pub mode: Mode,
    /// Starts without the global configuration (isolated profiles only).
    #[serde(default)]
    pub bare: bool,
}

impl Meta {
    pub fn links(&self, name: &str) -> bool {
        match (self.mode, layout::classify(name)) {
            (_, Kind::Account) => false,
            (Mode::Shared, _) => true,
            (Mode::Isolated, Kind::Config) => !self.bare,
            (Mode::Isolated, Kind::State) => false,
        }
    }

    pub fn describe(&self) -> &'static str {
        match (self.mode, self.bare) {
            (Mode::Shared, _) => "shared",
            (Mode::Isolated, false) => "isolated",
            (Mode::Isolated, true) => "isolated, bare",
        }
    }
}

pub struct Profile {
    pub name: String,
    pub dir: PathBuf,
    pub meta: Meta,
}

impl Profile {
    /// The directory `CLAUDE_CONFIG_DIR` points at.
    pub fn home(&self) -> PathBuf {
        self.dir.join("home")
    }

    pub fn sync(&self, paths: &Paths) -> Result<SyncReport> {
        layout::sync(&paths.claude_home, &self.home(), |name| {
            self.meta.links(name)
        })
    }
}

pub fn validate_name(name: &str) -> Result<()> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with(['.', '-'])
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if !valid {
        bail!(
            "invalid profile name '{name}': use letters, digits, '-', '_' or '.', \
             not starting with '.' or '-'"
        );
    }
    Ok(())
}

pub fn load(paths: &Paths, name: &str) -> Result<Profile> {
    validate_name(name)?;
    let dir = paths.profiles_dir().join(name);
    let meta_path = dir.join("meta.toml");
    if !meta_path.is_file() {
        bail!("profile '{name}' does not exist (see `ccsp list`)");
    }
    let raw = fs::read_to_string(&meta_path)
        .with_context(|| format!("reading {}", meta_path.display()))?;
    let meta = toml::from_str(&raw).with_context(|| format!("parsing {}", meta_path.display()))?;
    Ok(Profile {
        name: name.to_owned(),
        dir,
        meta,
    })
}

pub fn list(paths: &Paths) -> Result<Vec<Profile>> {
    let dir = paths.profiles_dir();
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut profiles = Vec::new();
    for entry in fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
        let name = entry?.file_name();
        if let Some(name) = name.to_str()
            && let Ok(profile) = load(paths, name)
        {
            profiles.push(profile);
        }
    }
    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(profiles)
}

pub fn create(paths: &Paths, name: &str, meta: Meta) -> Result<Profile> {
    validate_name(name)?;
    let dir = paths.profiles_dir().join(name);
    if fs::symlink_metadata(&dir).is_ok() {
        bail!("profile '{name}' already exists");
    }
    let profile = Profile {
        name: name.to_owned(),
        dir,
        meta,
    };
    // The directory is new and ours: on failure, remove it. `remove_dir_all` removes
    // symlinks themselves and never follows them into `~/.claude`.
    populate(paths, &profile).inspect_err(|_| {
        let _ = fs::remove_dir_all(&profile.dir);
    })?;
    Ok(profile)
}

fn populate(paths: &Paths, profile: &Profile) -> Result<()> {
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(profile.home())
        .with_context(|| format!("creating {}", profile.home().display()))?;
    fs::write(
        profile.dir.join("meta.toml"),
        toml::to_string(&profile.meta)?,
    )?;
    profile.sync(paths)?;
    if !profile.meta.bare {
        claude_json::seed(&paths.claude_json, &profile.home().join(".claude.json"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_reject_name_when_it_escapes_the_profiles_dir() {
        assert!(validate_name("../etc").is_err());
        assert!(validate_name(".hidden").is_err());
        assert!(validate_name("a/b").is_err());
        assert!(validate_name("").is_err());
    }

    #[test]
    fn test_should_accept_name_when_it_is_plain() {
        assert!(validate_name("trabalho").is_ok());
        assert!(validate_name("work-2.0_x").is_ok());
    }

    #[test]
    fn test_should_link_state_only_when_shared() {
        let shared = Meta {
            mode: Mode::Shared,
            bare: false,
        };
        let isolated = Meta {
            mode: Mode::Isolated,
            bare: false,
        };
        let bare = Meta {
            mode: Mode::Isolated,
            bare: true,
        };
        assert!(shared.links("projects"));
        assert!(!isolated.links("projects"));
        assert!(isolated.links("skills"));
        assert!(!bare.links("skills"));
        assert!(!shared.links(".credentials.json"));
    }
}
