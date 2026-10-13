use super::tests::new_test_composer;
use super::*;
use pretty_assertions::assert_eq;

fn native_composer() -> ChatComposer {
    let (mut composer, _) = new_test_composer();
    composer.config.canvastty_native_editing = true;
    composer.set_keymap_bindings(&RuntimeKeymap::defaults());
    composer
}

#[test]
fn native_enter_only_command_enter_submits_even_with_popup_or_paste_burst() {
    for text in ["Первая 👋", "/review аргументы", "@file", "$skill"] {
        for modifiers in [KeyModifiers::NONE, KeyModifiers::SHIFT] {
            let mut composer = native_composer();
            composer.insert_str(text);
            let (result, _) = composer.handle_key_event(KeyEvent::new(KeyCode::Enter, modifiers));
            assert!(matches!(result, InputResult::None));
            assert_eq!(composer.current_text(), format!("{text}\n"));
        }
    }
    for queue in [false, true] {
        let mut composer = native_composer();
        composer.queue_submissions = queue;
        // The last character is still held by paste detection when Enter arrives.
        composer.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        composer.handle_key_event(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
        composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
        composer.handle_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        let (result, _) =
            composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::SUPER));
        let submitted = match result {
            InputResult::Submitted { text, .. } if !queue => text,
            InputResult::Queued { text, .. } if queue => text,
            other => panic!("unexpected dispatch: {other:?}"),
        };
        assert_eq!(submitted, "a\nb\nc");
    }
}

#[test]
fn native_enter_replaces_selection_and_tab_cannot_queue() {
    let mut composer = native_composer();
    composer.insert_str("удалить 👋\nвсё");
    composer.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SUPER));
    composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
    assert_eq!(composer.current_text(), "\n");
    for text in ["draft", "/unknown-command"] {
        composer.draft.textarea.set_text_clearing_elements(text);
        composer.is_task_running = true;
        composer.sync_popups();
        let (result, _) =
            composer.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert!(matches!(result, InputResult::None));
        assert_eq!(composer.current_text(), text);
    }
    let mut remapped = RuntimeKeymap::defaults();
    remapped.composer.submit = vec![key_hint::plain(KeyCode::Enter)];
    remapped.composer.queue = vec![key_hint::plain(KeyCode::Tab)];
    composer.set_keymap_bindings(&remapped);
    assert!(composer.queue_keys.is_empty());
    assert_eq!(
        composer.submit_keys,
        vec![KeyBinding::new(KeyCode::Enter, KeyModifiers::SUPER)]
    );
}

#[test]
fn native_command_enter_accepts_highlighted_slash_command() {
    let mut composer = native_composer();
    composer.insert_str("/rev");
    assert!(matches!(composer.popups.active, ActivePopup::Command(_)));
    let (result, _) = composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::SUPER));
    assert!(matches!(result, InputResult::Command(SlashCommand::Review)));
}

#[test]
fn default_and_embedded_composers_preserve_their_enter_behavior() {
    for config in [
        ChatComposerConfig::default(),
        ChatComposerConfig::plain_text(),
    ] {
        let (mut composer, _) = new_test_composer();
        composer.config = config;
        composer.set_keymap_bindings(&RuntimeKeymap::defaults());
        composer.insert_str("unchanged");
        let (result, _) =
            composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(result, InputResult::Submitted { text, .. } if text == "unchanged"));
    }
}

#[test]
#[cfg(target_os = "macos")]
fn native_enter_shortcut_help_shows_command_enter_and_newline() {
    super::tests::snapshot_composer_state_with_width(
        "canvastty_native_enter_shortcuts",
        110,
        true,
        |composer| {
            composer.config.canvastty_native_editing = true;
            composer.set_keymap_bindings(&RuntimeKeymap::defaults());
            composer.footer.mode = FooterMode::ShortcutOverlay;
        },
    );
}

#[test]
fn canvastty_keyboard_rebinds_submit_newline_and_select_all() {
    let mut composer = native_composer();
    composer.set_canvastty_keyboard(
        CanvasttyKeyboard::from_json(
            r#"{"submit":"Ctrl+Enter","newline":"Alt+Enter","newlineAlternate":"","selectAll":"Ctrl+L"}"#,
        ),
        &RuntimeKeymap::defaults(),
    );
    composer.insert_str("a");
    composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT));
    composer.insert_str("b");
    let (result, _) = composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::SUPER));
    assert!(
        matches!(result, InputResult::None),
        "the old submit key no longer sends"
    );
    composer.handle_key_event(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL));
    assert_eq!(
        composer.draft.textarea.mouse_selection_range(),
        Some(0.."a\nb".len()),
        "Ctrl+L selected the whole draft"
    );
    let (result, _) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
    assert!(matches!(result, InputResult::Submitted { text, .. } if text == "a\nb"));

    // A plain Enter no CanvasTTY key uses keeps Codex's own submit.
    composer.insert_str("plain");
    let (result, _) = composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(result, InputResult::Submitted { text, .. } if text == "plain"));
    assert_eq!(
        composer.submit_keys,
        vec![
            KeyBinding::new(KeyCode::Enter, KeyModifiers::CONTROL),
            KeyBinding::new(KeyCode::Enter, KeyModifiers::NONE),
        ]
    );
    assert_eq!(
        composer.footer.insert_newline_key,
        Some(KeyBinding::new(KeyCode::Enter, KeyModifiers::ALT).into())
    );
}

#[test]
fn canvastty_keyboard_copies_cuts_and_deletes_a_selected_draft() {
    use crate::clipboard_copy::CopyStatus;
    use crate::tui::TuiEvent;

    let mut composer = native_composer();
    composer.set_canvastty_keyboard(
        CanvasttyKeyboard::from_json(r#"{"copy":"Ctrl+Y","cut":"Ctrl+K","clear":"Ctrl+D"}"#),
        &RuntimeKeymap::defaults(),
    );
    let key = |code, modifiers| TuiEvent::Key(KeyEvent::new(code, modifiers));
    let mut copied = String::new();
    composer.insert_str("без выделения");
    let result = composer.copy_selection(&key(KeyCode::Char('k'), KeyModifiers::CONTROL), |_| {
        panic!("nothing is selected")
    });
    assert_eq!(
        result, None,
        "without a selection the key keeps its meaning"
    );

    composer.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SUPER));
    let result = composer.copy_selection(&key(KeyCode::Char('y'), KeyModifiers::CONTROL), |text| {
        copied = text.to_string();
        Ok(CopyStatus::Confirmed)
    });
    assert_eq!(
        (result, copied.as_str(), composer.current_text()),
        (
            Some((13, Ok(CopyStatus::Confirmed))),
            "без выделения",
            "без выделения".to_string()
        )
    );

    composer.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SUPER));
    let result = composer.copy_selection(&key(KeyCode::Char('k'), KeyModifiers::CONTROL), |text| {
        copied = format!("cut: {text}");
        Ok(CopyStatus::Confirmed)
    });
    assert_eq!(
        (result, copied.as_str(), composer.current_text()),
        (
            Some((13, Ok(CopyStatus::Confirmed))),
            "cut: без выделения",
            String::new()
        )
    );

    composer.insert_str("удалить");
    composer.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SUPER));
    composer.handle_key_event(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
    assert_eq!(composer.current_text(), "");
    let failed = composer.copy_selection(&key(KeyCode::Char('k'), KeyModifiers::CONTROL), |_| {
        Err("no clipboard".to_string())
    });
    assert_eq!(failed, None, "an empty draft has nothing to cut");
    composer.insert_str("keep");
    composer.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SUPER));
    composer.copy_selection(&key(KeyCode::Char('k'), KeyModifiers::CONTROL), |_| {
        Err("no clipboard".to_string())
    });
    assert_eq!(
        composer.current_text(),
        "keep",
        "a failed copy cuts nothing"
    );
}
