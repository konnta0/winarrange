use std::{collections::BTreeSet, fmt};

use anyhow::{bail, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Arrange,
    ArrangeFocusedProcess,
    ToggleFloat,
    FocusNext,
    FocusPrevious,
    FocusNextAll,
    FocusPreviousAll,
    ToggleZoom,
    Profile(String),
}

impl Action {
    pub fn parse(name: &str, profile: Option<&str>) -> Result<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "arrange" => no_profile(profile, Self::Arrange),
            "arrange-focused" => no_profile(profile, Self::ArrangeFocusedProcess),
            "toggle-float" => no_profile(profile, Self::ToggleFloat),
            "focus-next" => no_profile(profile, Self::FocusNext),
            "focus-prev" | "focus-previous" => no_profile(profile, Self::FocusPrevious),
            "focus-next-all" => no_profile(profile, Self::FocusNextAll),
            "focus-prev-all" | "focus-previous-all" => no_profile(profile, Self::FocusPreviousAll),
            "toggle-zoom" => no_profile(profile, Self::ToggleZoom),
            "profile" => {
                let profile = profile
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| anyhow::anyhow!("action 'profile' requires a profile name"))?;
                Ok(Self::Profile(profile.to_owned()))
            }
            unknown => bail!("unknown hotkey action '{unknown}'"),
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arrange => formatter.write_str("arrange"),
            Self::ArrangeFocusedProcess => formatter.write_str("arrange-focused"),
            Self::ToggleFloat => formatter.write_str("toggle-float"),
            Self::FocusNext => formatter.write_str("focus-next"),
            Self::FocusPrevious => formatter.write_str("focus-prev"),
            Self::FocusNextAll => formatter.write_str("focus-next-all"),
            Self::FocusPreviousAll => formatter.write_str("focus-prev-all"),
            Self::ToggleZoom => formatter.write_str("toggle-zoom"),
            Self::Profile(profile) => write!(formatter, "profile:{profile}"),
        }
    }
}

fn no_profile(profile: Option<&str>, action: Action) -> Result<Action> {
    if profile.is_some() {
        bail!("profile is only valid with action 'profile'");
    }
    Ok(action)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HotkeySpec {
    pub display: String,
    pub normalized: String,
}

impl HotkeySpec {
    pub fn parse(value: &str) -> Result<Self> {
        let tokens: Vec<_> = value.split('+').map(str::trim).collect();
        if tokens.is_empty() || tokens.iter().any(|token| token.is_empty()) {
            bail!("invalid hotkey '{value}': empty key or modifier");
        }

        let (key, modifiers) = tokens
            .split_last()
            .expect("a non-empty token list always has a final key");
        if modifier_name(key).is_some() {
            bail!("invalid hotkey '{value}': a non-modifier key is required");
        }
        if !valid_key(key) {
            bail!("invalid hotkey '{value}': unsupported key '{key}'");
        }

        let mut normalized_modifiers = BTreeSet::new();
        for modifier in modifiers {
            let normalized = modifier_name(modifier).ok_or_else(|| {
                anyhow::anyhow!("invalid hotkey '{value}': unknown modifier '{modifier}'")
            })?;
            if !normalized_modifiers.insert(normalized) {
                bail!("invalid hotkey '{value}': duplicate modifier '{modifier}'");
            }
        }

        let mut normalized = normalized_modifiers
            .into_iter()
            .collect::<Vec<_>>()
            .join("+");
        if !normalized.is_empty() {
            normalized.push('+');
        }
        normalized.push_str(&normalize_key(key));
        Ok(Self {
            display: value.trim().to_owned(),
            normalized,
        })
    }
}

fn modifier_name(value: &str) -> Option<&'static str> {
    match value.to_ascii_lowercase().as_str() {
        "cmd" | "command" | "win" | "super" => Some("super"),
        "ctrl" | "control" => Some("control"),
        "alt" | "option" => Some("alt"),
        "shift" => Some("shift"),
        _ => None,
    }
}

fn normalize_key(value: &str) -> String {
    if value.len() == 1 && value.as_bytes()[0].is_ascii_alphabetic() {
        format!("Key{}", value.to_ascii_uppercase())
    } else if value.len() == 1 && value.as_bytes()[0].is_ascii_digit() {
        format!("Digit{value}")
    } else {
        value.to_owned()
    }
}

fn valid_key(value: &str) -> bool {
    if value.len() == 1
        && (value.as_bytes()[0].is_ascii_alphanumeric() || "`\\[],=-.';/".contains(value))
    {
        return true;
    }
    let uppercase = value.to_ascii_uppercase();
    matches!(
        uppercase.as_str(),
        "BACKQUOTE"
            | "BACKSLASH"
            | "BRACKETLEFT"
            | "BRACKETRIGHT"
            | "COMMA"
            | "EQUAL"
            | "MINUS"
            | "PERIOD"
            | "QUOTE"
            | "SEMICOLON"
            | "SLASH"
            | "BACKSPACE"
            | "CAPSLOCK"
            | "ENTER"
            | "SPACE"
            | "TAB"
            | "DELETE"
            | "END"
            | "HOME"
            | "INSERT"
            | "PAGEDOWN"
            | "PAGEUP"
            | "PRINTSCREEN"
            | "SCROLLLOCK"
            | "DOWN"
            | "LEFT"
            | "RIGHT"
            | "UP"
            | "ARROWDOWN"
            | "ARROWLEFT"
            | "ARROWRIGHT"
            | "ARROWUP"
            | "ESC"
            | "ESCAPE"
    ) || uppercase
        .strip_prefix('F')
        .and_then(|number| number.parse::<u8>().ok())
        .is_some_and(|number| (1..=24).contains(&number))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_required_hotkey_forms_case_insensitively() {
        assert_eq!(
            HotkeySpec::parse("cmd+shift+a").unwrap().normalized,
            "shift+super+KeyA"
        );
        assert_eq!(
            HotkeySpec::parse("CTRL+Alt+A").unwrap().normalized,
            "alt+control+KeyA"
        );
        assert_eq!(
            HotkeySpec::parse("win+shift+a").unwrap().normalized,
            "shift+super+KeyA"
        );
    }

    #[test]
    fn rejects_invalid_hotkeys() {
        for value in ["cmd++a", "foo+a", "cmd"] {
            assert!(HotkeySpec::parse(value).is_err(), "{value} must be invalid");
        }
    }

    #[test]
    fn parses_structured_actions() {
        assert_eq!(Action::parse("arrange", None).unwrap(), Action::Arrange);
        assert_eq!(
            Action::parse("arrange-focused", None).unwrap(),
            Action::ArrangeFocusedProcess
        );
        assert_eq!(
            Action::parse("toggle-float", None).unwrap(),
            Action::ToggleFloat
        );
        assert_eq!(
            Action::parse("focus-next", None).unwrap(),
            Action::FocusNext
        );
        assert_eq!(
            Action::parse("focus-prev", None).unwrap(),
            Action::FocusPrevious
        );
        assert_eq!(
            Action::parse("focus-next-all", None).unwrap(),
            Action::FocusNextAll
        );
        assert_eq!(
            Action::parse("focus-prev-all", None).unwrap(),
            Action::FocusPreviousAll
        );
        assert_eq!(
            Action::parse("toggle-zoom", None).unwrap(),
            Action::ToggleZoom
        );
        assert_eq!(
            Action::parse("profile", Some("unity")).unwrap(),
            Action::Profile("unity".into())
        );
        assert!(Action::parse("profile", None).is_err());
    }
}
