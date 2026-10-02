use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::PathBuf,
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub default_profile: Option<String>,
    #[serde(default)]
    pub layout: Option<LayoutKind>,
    #[serde(default)]
    pub sort: Option<SortOrder>,
    #[serde(default)]
    pub columns: Option<usize>,
    #[serde(default)]
    pub gap: Option<i32>,
    #[serde(default)]
    pub margin: Option<i32>,
    #[serde(default)]
    pub ignore: Vec<IgnoreRule>,
    #[serde(default, rename = "profile")]
    pub profiles: BTreeMap<String, Profile>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    #[serde(default)]
    pub process: String,
    #[serde(default)]
    pub layout: Option<LayoutKind>,
    #[serde(default)]
    pub sort: Option<SortOrder>,
    #[serde(default)]
    pub columns: Option<usize>,
    #[serde(default)]
    pub gap: Option<i32>,
    #[serde(default)]
    pub margin: Option<i32>,
    #[serde(default, rename = "slot", skip_serializing_if = "Vec::is_empty")]
    pub slots: Vec<SlotRule>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotRule {
    pub title: String,
    pub position: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutKind {
    #[default]
    Grid,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    #[default]
    Title,
}

impl fmt::Display for LayoutKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Grid => formatter.write_str("grid"),
        }
    }
}

impl fmt::Display for SortOrder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Title => formatter.write_str("title"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IgnoreRule {
    Process { value: String },
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_path()?;
        match fs::read_to_string(&path) {
            Ok(contents) => {
                let config: Self = toml::from_str(&contents)
                    .with_context(|| format!("Failed to load config:\n{}", path.display()))?;
                config
                    .validate()
                    .with_context(|| format!("Failed to load config:\n{}", path.display()))?;
                Ok(config)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::load_legacy_json(),
            Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
        }
    }

    fn load_legacy_json() -> Result<Self> {
        let path = legacy_config_path()?;
        match fs::read(&path) {
            Ok(bytes) => {
                let config: Self = serde_json::from_slice(&bytes).with_context(|| {
                    format!("Failed to load legacy config:\n{}", path.display())
                })?;
                config.validate().with_context(|| {
                    format!("Failed to load legacy config:\n{}", path.display())
                })?;
                Ok(config)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
        }
    }

    pub fn save(&self) -> Result<()> {
        self.validate()?;
        let path = config_path()?;
        let parent = path.parent().context("invalid config path")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
        let contents = toml::to_string_pretty(self)?;
        fs::write(&path, contents)
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        if self.columns == Some(0) {
            bail!("columns must be greater than zero");
        }
        if self.gap.is_some_and(|value| value < 0) {
            bail!("gap must not be negative");
        }
        if self.margin.is_some_and(|value| value < 0) {
            bail!("margin must not be negative");
        }

        for (name, profile) in &self.profiles {
            if profile.columns == Some(0) {
                bail!("Invalid profile '{name}': columns must be greater than zero.");
            }
            if profile.gap.is_some_and(|value| value < 0) {
                bail!("Invalid profile '{name}': gap must not be negative.");
            }
            if profile.margin.is_some_and(|value| value < 0) {
                bail!("Invalid profile '{name}': margin must not be negative.");
            }

            let mut positions = BTreeSet::new();
            for rule in &profile.slots {
                if rule.title.trim().is_empty() {
                    bail!("Invalid profile '{name}': slot title must not be empty.");
                }
                if rule.position < 0 {
                    bail!(
                        "Invalid profile '{name}': slot position {} must not be negative.",
                        rule.position
                    );
                }
                if usize::try_from(rule.position).is_err() {
                    bail!(
                        "Invalid profile '{name}': slot position {} is too large.",
                        rule.position
                    );
                }
                if !positions.insert(rule.position) {
                    bail!(
                        "Invalid profile '{name}': slot {} is assigned by multiple rules.",
                        rule.position
                    );
                }
            }
        }
        Ok(())
    }

    pub fn ignores_process(&self, process: &str) -> bool {
        self.ignore.iter().any(|rule| match rule {
            IgnoreRule::Process { value } => value.eq_ignore_ascii_case(process),
        })
    }

    pub fn add_process(&mut self, process: String) -> bool {
        if self.ignore.iter().any(
            |rule| matches!(rule, IgnoreRule::Process { value } if value.eq_ignore_ascii_case(&process)),
        ) {
            false
        } else {
            self.ignore.push(IgnoreRule::Process { value: process });
            true
        }
    }

    pub fn remove_process(&mut self, process: &str) -> bool {
        let before = self.ignore.len();
        self.ignore.retain(
            |rule| !matches!(rule, IgnoreRule::Process { value } if value.eq_ignore_ascii_case(process)),
        );
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

pub fn config_path() -> Result<PathBuf> {
    Ok(data_root()?.join("config.toml"))
}

pub fn active_config_path() -> Result<PathBuf> {
    let path = config_path()?;
    if path.exists() {
        return Ok(path);
    }
    let legacy = legacy_config_path()?;
    if legacy.exists() {
        Ok(legacy)
    } else {
        Ok(path)
    }
}

fn legacy_config_path() -> Result<PathBuf> {
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

    #[test]
    fn parses_v02_profiles() {
        let config: Config = toml::from_str(
            r#"
gap = 8
margin = 4

[profile.unity]
process = "Unity"
layout = "grid"
sort = "title"
columns = 3
gap = 12
"#,
        )
        .unwrap();
        let profile = &config.profiles["unity"];
        assert_eq!(config.gap, Some(8));
        assert_eq!(profile.columns, Some(3));
        assert_eq!(profile.gap, Some(12));
        assert_eq!(profile.layout, Some(LayoutKind::Grid));
    }

    #[test]
    fn parses_and_validates_slot_rules() {
        let config: Config = toml::from_str(
            r#"
[profile.unity]
process = "Unity"
columns = 2

[[profile.unity.slot]]
title = "Server"
position = 0

[[profile.unity.slot]]
title = "Client-01"
position = 1
"#,
        )
        .unwrap();
        config.validate().unwrap();
        assert_eq!(config.profiles["unity"].slots.len(), 2);
    }

    #[test]
    fn rejects_invalid_and_duplicate_slots() {
        let mut config = Config::default();
        config.profiles.insert(
            "unity".into(),
            Profile {
                process: "Unity".into(),
                slots: vec![
                    SlotRule {
                        title: "Server".into(),
                        position: 0,
                    },
                    SlotRule {
                        title: "Client".into(),
                        position: 0,
                    },
                ],
                ..Profile::default()
            },
        );
        let error = config.validate().unwrap_err().to_string();
        assert!(error.contains("Invalid profile 'unity'"));
        assert!(error.contains("slot 0"));

        config.profiles.get_mut("unity").unwrap().slots[1].position = -1;
        let error = config.validate().unwrap_err().to_string();
        assert!(error.contains("must not be negative"));
    }

    #[test]
    fn config_round_trips_through_toml() {
        let mut config = Config {
            default_profile: Some("unity".into()),
            gap: Some(8),
            ..Config::default()
        };
        config.profiles.insert(
            "unity".into(),
            Profile {
                process: "Unity".into(),
                layout: Some(LayoutKind::Grid),
                sort: Some(SortOrder::Title),
                ..Profile::default()
            },
        );
        config.add_process("Calculator".into());

        let encoded = toml::to_string_pretty(&config).unwrap();
        let decoded: Config = toml::from_str(&encoded).unwrap();
        assert_eq!(decoded.default_profile.as_deref(), Some("unity"));
        assert_eq!(decoded.profiles["unity"].process, "Unity");
        assert!(decoded.ignores_process("calculator"));
    }

    #[test]
    fn invalid_layout_reports_a_toml_location() {
        let error = toml::from_str::<Config>(
            r#"
[profile.bad]
process = "Unity"
layout = "bsp"
"#,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("layout"));
        assert!(error.contains("bsp"));
        assert!(error.contains("line"));
    }

    #[test]
    fn example_config_stays_valid() {
        let config: Config = toml::from_str(include_str!("../../config.example.toml")).unwrap();
        config.validate().unwrap();
        assert!(config.profiles.contains_key("unity"));
        assert!(config.profiles.contains_key("browser"));
    }
}
