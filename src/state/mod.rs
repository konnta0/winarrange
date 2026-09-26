use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
    config::{data_root, write_json},
    model::WindowKey,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub floating: Vec<WindowKey>,
}

impl State {
    pub fn load() -> Result<Self> {
        let path = state_path()?;
        match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .with_context(|| format!("failed to parse {}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
        }
    }

    pub fn save(&self) -> Result<()> {
        write_json(state_path()?, self)
    }

    pub fn contains(&self, key: &WindowKey) -> bool {
        self.floating.contains(key)
    }

    pub fn insert(&mut self, key: WindowKey) -> bool {
        if self.contains(&key) {
            false
        } else {
            self.floating.push(key);
            true
        }
    }

    pub fn remove(&mut self, key: &WindowKey) -> bool {
        let before = self.floating.len();
        self.floating.retain(|candidate| candidate != key);
        before != self.floating.len()
    }
}

fn state_path() -> Result<PathBuf> {
    Ok(data_root()?.join("state.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floating_identity_includes_process_and_window() {
        let first = WindowKey {
            pid: 100,
            id: "abc".into(),
        };
        let restarted = WindowKey {
            pid: 101,
            id: "abc".into(),
        };
        let mut state = State::default();

        assert!(state.insert(first.clone()));
        assert!(!state.insert(first.clone()));
        assert!(state.contains(&first));
        assert!(!state.contains(&restarted));
        assert!(state.remove(&first));
        assert!(!state.contains(&first));
    }
}
