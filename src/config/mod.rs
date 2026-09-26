use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub ignore: Vec<IgnoreRule>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IgnoreRule {
    Process { value: String },
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_path()?;
        match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .with_context(|| format!("failed to parse {}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
        }
    }

    pub fn save(&self) -> Result<()> {
        write_json(config_path()?, self)
    }

    pub fn ignores_process(&self, process: &str) -> bool {
        self.ignore.iter().any(|rule| match rule {
            IgnoreRule::Process { value } => value.eq_ignore_ascii_case(process),
        })
    }

    pub fn add_process(&mut self, process: String) -> bool {
        if self.ignore.iter().any(|rule| matches!(rule, IgnoreRule::Process { value } if value.eq_ignore_ascii_case(&process))) {
            false
        } else {
            self.ignore.push(IgnoreRule::Process { value: process });
            true
        }
    }

    pub fn remove_process(&mut self, process: &str) -> bool {
        let before = self.ignore.len();
        self.ignore.retain(|rule| !matches!(rule, IgnoreRule::Process { value } if value.eq_ignore_ascii_case(process)));
        before != self.ignore.len()
    }
}

pub(crate) fn data_root() -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    let root = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|p| p.join("Library/Application Support/winarrange"));
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|p| p.join("winarrange"));
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|p| p.join(".config"))
        })
        .map(|p| p.join("winarrange"));
    root.context("could not determine the user configuration directory")
}

fn config_path() -> Result<PathBuf> {
    Ok(data_root()?.join("config.json"))
}

pub(crate) fn write_json(path: PathBuf, value: &impl Serialize) -> Result<()> {
    let parent = path.parent().context("invalid data path")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(&path, bytes).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_rules_are_case_insensitive() {
        let mut config = Config::default();
        assert!(config.add_process("Calculator".into()));
        assert!(!config.add_process("calculator".into()));
        assert!(config.ignores_process("CALCULATOR"));
        assert!(config.remove_process("calculator"));
    }
}
