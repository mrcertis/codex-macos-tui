use super::*;
use pretty_assertions::assert_eq;

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyBinding {
    KeyBinding::new(code, modifiers)
}

#[test]
fn reads_canvastty_shortcut_strings() {
    assert_eq!(
        parse_binding("Meta+Enter"),
        Some(key(KeyCode::Enter, KeyModifiers::SUPER))
    );
    assert_eq!(
        parse_binding("Enter"),
        Some(key(KeyCode::Enter, KeyModifiers::NONE))
    );
    assert_eq!(
        parse_binding("Shift+Meta+A"),
        Some(key(
            KeyCode::Char('a'),
            KeyModifiers::SHIFT | KeyModifiers::SUPER
        ))
    );
    assert_eq!(
        parse_binding("Ctrl+Alt+7"),
        Some(key(
            KeyCode::Char('7'),
            KeyModifiers::CONTROL | KeyModifiers::ALT
        ))
    );
    assert_eq!(
        parse_binding("Ctrl+Comma"),
        Some(key(KeyCode::Char(','), KeyModifiers::CONTROL))
    );
    assert_eq!(
        parse_binding("F12"),
        Some(key(KeyCode::F(12), KeyModifiers::NONE))
    );
    assert_eq!(
        parse_binding("Alt+ArrowUp"),
        Some(key(KeyCode::Up, KeyModifiers::ALT))
    );
    for unreadable in [
        "",
        "Meta",
        "Super+Enter",
        "Meta+Meta+A",
        "F25",
        "Mouse4",
        "Enter+Meta",
        "ab",
    ] {
        assert_eq!(parse_binding(unreadable), None, "{unreadable}");
    }
}

#[test]
fn missing_or_malformed_keys_keep_the_defaults() {
    let defaults = CanvasttyKeyboard::default();
    assert_eq!(defaults.submit, key(KeyCode::Enter, KeyModifiers::SUPER));
    assert_eq!(
        defaults.newline,
        vec![
            key(KeyCode::Enter, KeyModifiers::NONE),
            key(KeyCode::Enter, KeyModifiers::SHIFT)
        ]
    );
    assert_eq!(
        defaults.select_all,
        key(KeyCode::Char('a'), KeyModifiers::SUPER)
    );
    for value in ["", "not json", "[]", "{}", r#"{"submit":7}"#] {
        assert_eq!(CanvasttyKeyboard::from_json(value), defaults, "{value}");
    }
    let partly = CanvasttyKeyboard::from_json(r#"{"submit":"Hyper+Enter","newline":"Alt+Enter"}"#);
    assert_eq!(
        partly.submit, defaults.submit,
        "an unreadable binding keeps its default"
    );
    assert_eq!(
        partly.newline,
        vec![
            key(KeyCode::Enter, KeyModifiers::ALT),
            key(KeyCode::Enter, KeyModifiers::SHIFT)
        ]
    );
}

#[test]
fn reads_the_canvastty_contract() {
    let keyboard = CanvasttyKeyboard::from_json(
        r#"{"submit":"Ctrl+Enter","newline":"Enter","newlineAlternate":"Shift+Meta+Enter","selectAll":"Ctrl+L"}"#,
    );
    assert_eq!(
        keyboard,
        CanvasttyKeyboard {
            submit: key(KeyCode::Enter, KeyModifiers::CONTROL),
            newline: vec![
                key(KeyCode::Enter, KeyModifiers::NONE),
                key(KeyCode::Enter, KeyModifiers::SHIFT | KeyModifiers::SUPER),
            ],
            select_all: key(KeyCode::Char('l'), KeyModifiers::CONTROL),
            copy: Some(key(KeyCode::Char('c'), KeyModifiers::SUPER)),
            cut: Some(key(KeyCode::Char('x'), KeyModifiers::SUPER)),
            clear: Some(key(KeyCode::Backspace, KeyModifiers::NONE)),
        }
    );
    let selection =
        CanvasttyKeyboard::from_json(r#"{"copy":"","cut":"Ctrl+K","clear":"Hyper+Delete"}"#);
    assert_eq!(
        (selection.copy, selection.cut, selection.clear),
        (
            None,
            Some(key(KeyCode::Char('k'), KeyModifiers::CONTROL)),
            CanvasttyKeyboard::default().clear
        ),
        "empty switches a key off; unreadable keeps its default"
    );
    assert!(keyboard.binds_plain_enter());
    let without_enter = CanvasttyKeyboard::from_json(
        r#"{"submit":"Ctrl+Enter","newline":"Shift+Enter","newlineAlternate":"","selectAll":"Meta+A"}"#,
    );
    assert_eq!(
        without_enter.newline,
        vec![key(KeyCode::Enter, KeyModifiers::SHIFT)]
    );
    assert!(!without_enter.binds_plain_enter());
}
