use std::fs;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::paths::Paths;

/// Machine-wide ccsp state.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    /// Profile every new shell starts with.
    pub default: Option<String>,
}

impl State {
    pub fn load(paths: &Paths) -> Result<Self> {
        let path = paths.state_file();
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        fs::create_dir_all(&paths.root)?;
        let path = paths.state_file();
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, toml::to_string(self)?)?;
        fs::rename(&tmp, &path).with_context(|| format!("writing {}", path.display()))
    }
}
