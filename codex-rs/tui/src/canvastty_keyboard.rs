//! Editor keys CanvasTTY passes to its owned macOS frontend in `CANVASTTY_CODEX_KEYBOARD`.
//!
//! The variable is a JSON object of CanvasTTY shortcut strings (`"Meta+Enter"`, `"Shift+Enter"`,
//! `"Meta+A"`): `submit`, `newline`, `newlineAlternate`, `selectAll`, and the selection keys
//! `copy`, `cut` and `clear`. It is read once per process, so a changed binding applies to new or
//! restarted cards. A missing or malformed variable keeps the defaults, and a binding this
//! frontend cannot read keeps that action's default. An empty `newlineAlternate`, `copy`, `cut` or
//! `clear` switches that key off.

use std::sync::OnceLock;

use crossterm::event::KeyCode;
use crossterm::event::KeyModifiers;

use crate::key_hint::KeyBinding;

pub(crate) const ENV_VAR: &str = "CANVASTTY_CODEX_KEYBOARD";

/// The main draft's editor keys when CanvasTTY native editing is on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CanvasttyKeyboard {
    pub(crate) submit: KeyBinding,
    /// The line break and its alternate, in that order.
    pub(crate) newline: Vec<KeyBinding>,
    pub(crate) select_all: KeyBinding,
    /// Act on a selected draft only; without a selection the key keeps its usual meaning.
    pub(crate) copy: Option<KeyBinding>,
    pub(crate) cut: Option<KeyBinding>,
    pub(crate) clear: Option<KeyBinding>,
}

impl Default for CanvasttyKeyboard {
    fn default() -> Self {
        Self {
            submit: KeyBinding::new(KeyCode::Enter, KeyModifiers::SUPER),
            newline: vec![
                KeyBinding::new(KeyCode::Enter, KeyModifiers::NONE),
                KeyBinding::new(KeyCode::Enter, KeyModifiers::SHIFT),
            ],
            select_all: KeyBinding::new(KeyCode::Char('a'), KeyModifiers::SUPER),
            copy: Some(KeyBinding::new(KeyCode::Char('c'), KeyModifiers::SUPER)),
            cut: Some(KeyBinding::new(KeyCode::Char('x'), KeyModifiers::SUPER)),
            clear: Some(KeyBinding::new(KeyCode::Backspace, KeyModifiers::NONE)),
        }
    }
}

impl CanvasttyKeyboard {
    /// The process's keys, read from the environment on first use.
    pub(crate) fn current() -> &'static Self {
        static CURRENT: OnceLock<CanvasttyKeyboard> = OnceLock::new();
        CURRENT.get_or_init(|| {
            std::env::var(ENV_VAR)
                .map(|value| Self::from_json(&value))
                .unwrap_or_default()
        })
    }

    pub(crate) fn from_json(value: &str) -> Self {
        let defaults = Self::default();
        let Ok(serde_json::Value::Object(fields)) =
            serde_json::from_str::<serde_json::Value>(value)
        else {
            return defaults;
        };
        let binding = |name: &str| {
            fields
                .get(name)
                .and_then(serde_json::Value::as_str)
                .and_then(parse_binding)
        };
        // An optional key: empty is off; missing or unreadable keeps the default.
        let optional = |name: &str, default: Option<KeyBinding>| match fields
            .get(name)
            .and_then(serde_json::Value::as_str)
        {
            Some("") => None,
            Some(text) => parse_binding(text).or(default),
            None => default,
        };
        let newline = binding("newline").unwrap_or(defaults.newline[0]);
        let mut newlines = vec![newline];
        let alternate = optional("newlineAlternate", Some(defaults.newline[1]));
        if let Some(alternate) = alternate.filter(|alternate| *alternate != newline) {
            newlines.push(alternate);
        }
        Self {
            submit: binding("submit").unwrap_or(defaults.submit),
            newline: newlines,
            select_all: binding("selectAll").unwrap_or(defaults.select_all),
            copy: optional("copy", defaults.copy),
            cut: optional("cut", defaults.cut),
            clear: optional("clear", defaults.clear),
        }
    }

    /// Whether a plain Enter is one of these keys; otherwise it keeps Codex's own submit.
    pub(crate) fn binds_plain_enter(&self) -> bool {
        let enter = KeyBinding::new(KeyCode::Enter, KeyModifiers::NONE);
        self.submit == enter || self.select_all == enter || self.newline.contains(&enter)
    }

    /// The composer's submit keys: the CanvasTTY one, plus a plain Enter no CanvasTTY key uses.
    pub(crate) fn submit_keys(&self) -> Vec<KeyBinding> {
        let mut keys = vec![self.submit];
        if !self.binds_plain_enter() {
            keys.push(KeyBinding::new(KeyCode::Enter, KeyModifiers::NONE));
        }
        keys
    }
}

/// Reads one CanvasTTY shortcut string: modifiers (`Ctrl`, `Alt`, `Shift`, `Meta`) and a key.
pub(crate) fn parse_binding(text: &str) -> Option<KeyBinding> {
    let mut parts = text.split('+').collect::<Vec<_>>();
    let key = parts.pop()?;
    let mut modifiers = KeyModifiers::NONE;
    for part in parts {
        let modifier = match part {
            "Ctrl" => KeyModifiers::CONTROL,
            "Alt" => KeyModifiers::ALT,
            "Shift" => KeyModifiers::SHIFT,
            "Meta" => KeyModifiers::SUPER,
            _ => return None,
        };
        if modifiers.contains(modifier) {
            return None;
        }
        modifiers |= modifier;
    }
    let code = match key {
        "Enter" => KeyCode::Enter,
        "Space" => KeyCode::Char(' '),
        "Tab" => KeyCode::Tab,
        "Escape" => KeyCode::Esc,
        "Backspace" => KeyCode::Backspace,
        "Delete" => KeyCode::Delete,
        "Insert" => KeyCode::Insert,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        "PageUp" => KeyCode::PageUp,
        "PageDown" => KeyCode::PageDown,
        "ArrowUp" => KeyCode::Up,
        "ArrowDown" => KeyCode::Down,
        "ArrowLeft" => KeyCode::Left,
        "ArrowRight" => KeyCode::Right,
        "Comma" => KeyCode::Char(','),
        _ => {
            let mut chars = key.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_alphanumeric() => {
                    KeyCode::Char(c.to_ascii_lowercase())
                }
                _ => KeyCode::F(
                    key.strip_prefix('F')?
                        .parse()
                        .ok()
                        .filter(|n| (1..=24).contains(n))?,
                ),
            }
        }
    };
    Some(KeyBinding::new(code, modifiers))
}

#[cfg(test)]
#[path = "canvastty_keyboard_tests.rs"]
mod tests;
