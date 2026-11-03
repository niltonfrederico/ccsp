//! Seeding a profile's `.claude.json` from the global one, without the account.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

/// Copies `source` to `target`, dropping the signed-in account and its caches, so the
/// profile keeps MCP servers, trusted projects and onboarding but logs in on its own.
///
/// Returns `false` when there is no `source` to copy.
pub fn seed(source: &Path, target: &Path) -> Result<bool> {
    if !source.exists() {
        return Ok(false);
    }
    let raw =
        fs::read_to_string(source).with_context(|| format!("reading {}", source.display()))?;
    let mut config: Value =
        serde_json::from_str(&raw).with_context(|| format!("parsing {}", source.display()))?;
    if let Some(object) = config.as_object_mut() {
        object.retain(|key, _| !is_account_key(key));
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(target)
        .with_context(|| format!("creating {}", target.display()))?;
    serde_json::to_writer_pretty(&mut file, &config)?;
    file.write_all(b"\n")?;
    Ok(true)
}

fn is_account_key(key: &str) -> bool {
    matches!(
        key,
        "oauthAccount" | "primaryApiKey" | "customApiKeyResponses"
    ) || key.starts_with("cached")
        || key.ends_with("Cache")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_drop_account_keys_when_seeding() {
        assert!(is_account_key("oauthAccount"));
        assert!(is_account_key("cachedGrowthBookFeatures"));
        assert!(is_account_key("modelAccessCache"));
        assert!(!is_account_key("mcpServers"));
        assert!(!is_account_key("projects"));
    }
}
