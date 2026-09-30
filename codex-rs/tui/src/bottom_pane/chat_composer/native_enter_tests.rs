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
