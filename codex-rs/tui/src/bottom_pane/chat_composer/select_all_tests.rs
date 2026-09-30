use super::tests::new_test_composer;
use super::*;
use crate::clipboard_copy::CopyStatus;
use crate::tui::TuiEvent;
use crossterm::event::MouseButton;
use crossterm::event::MouseEvent;
use crossterm::event::MouseEventKind;
use pretty_assertions::assert_eq;
use std::path::PathBuf;

#[test]
fn super_a_selects_entire_utf8_multiline_draft_and_paste_replaces_it() {
    let (mut composer, _rx) = new_test_composer();
    let draft = "Привет 👋\nмир\nпоследняя строка";
    let cursor = "При".len();
    composer.draft.textarea.set_text_clearing_elements(draft);
    composer.draft.textarea.set_cursor(cursor);

    let (result, redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SUPER));

    assert!(matches!(result, InputResult::None));
    assert!(redraw);
    assert_eq!(
        (
            composer.draft.textarea.text(),
            composer.draft.textarea.cursor(),
            composer.draft.textarea.mouse_selection_range(),
        ),
        (draft, cursor, Some(0..draft.len()))
    );
    assert!(composer.handle_paste("Замена\nполностью".to_string()));
    assert_eq!(
        (
            composer.draft.textarea.text(),
            composer.draft.textarea.mouse_selection_range(),
        ),
        ("Замена\nполностью", None)
    );
}

#[test]
fn super_a_selects_offscreen_lines_and_copy_keeps_the_whole_draft() {
    let (mut composer, _rx) = new_test_composer();
    let draft = (0..30)
        .map(|line| format!("строка {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    composer.draft.textarea.set_text_clearing_elements(&draft);
    composer.draft.textarea.set_cursor(draft.len());
    let area = Rect::new(0, 0, 40, 6);
    let mut buffer = Buffer::empty(area);
    composer.render(area, &mut buffer);

    composer.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SUPER));
    assert_eq!(
        composer.draft.textarea.mouse_selection_range(),
        Some(0..draft.len())
    );
    let mut copied = String::new();
    let result = composer.copy_selection(
        &TuiEvent::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::SUPER)),
        |text| {
            copied = text.to_string();
            Ok(CopyStatus::Confirmed)
        },
    );
    assert_eq!(
        result,
        Some((draft.chars().count(), Ok(CopyStatus::Confirmed)))
    );
    assert_eq!(copied, draft);
    assert_eq!(
        (
            composer.draft.textarea.text(),
            composer.draft.textarea.cursor(),
            composer.draft.textarea.mouse_selection_range(),
        ),
        (draft.as_str(), draft.len(), None)
    );
}

#[test]
fn super_a_copy_expands_hidden_paste_and_preserves_image_and_bash_state() {
    let (mut composer, _rx) = new_test_composer();
    let hidden = "длинная вставка 👋\n".repeat(LARGE_PASTE_CHAR_THRESHOLD);
    let image_path = PathBuf::from("selection-copy.png");
    let image_number = 1;
    let image_placeholder = codex_protocol::models::local_image_label_text(image_number);
    composer.insert_str("начало\n");
    composer.handle_paste(hidden.clone());
    composer.attach_image(image_path);
    composer.insert_str("\nконец");
    composer.draft.is_bash_mode = true;
    let before = (
        composer.draft.textarea.text().to_string(),
        composer.draft.textarea.cursor(),
        composer.draft.textarea.text_elements(),
        composer.draft.pending_pastes.clone(),
        composer.attachments.local_image_paths(),
        composer.draft.is_bash_mode,
    );

    composer.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SUPER));
    let mut copied = String::new();
    let result = composer.copy_selection(
        &TuiEvent::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::SUPER)),
        |text| {
            copied = text.to_string();
            Ok(CopyStatus::Confirmed)
        },
    );
    let expected = format!("начало\n{hidden}{image_placeholder}\nконец");
    assert_eq!(copied, expected);
    assert_eq!(
        result,
        Some((expected.chars().count(), Ok(CopyStatus::Confirmed)))
    );
    assert_eq!(
        (
            composer.draft.textarea.text().to_string(),
            composer.draft.textarea.cursor(),
            composer.draft.textarea.text_elements(),
            composer.draft.pending_pastes.clone(),
            composer.attachments.local_image_paths(),
            composer.draft.is_bash_mode,
        ),
        before
    );
    assert_eq!(composer.draft.textarea.mouse_selection_range(), None);
}

#[test]
fn super_a_pending_copy_reports_full_payload_count_and_preserves_draft() {
    let (mut composer, _rx) = new_test_composer();
    let hidden = "Привет 👋\n".repeat(LARGE_PASTE_CHAR_THRESHOLD);
    composer.insert_str("prefix ");
    composer.handle_paste(hidden.clone());
    composer.insert_str(" suffix");
    let visible = composer.draft.textarea.text().to_string();
    let cursor = composer.draft.textarea.cursor();
    let pending = composer.draft.pending_pastes.clone();
    composer.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SUPER));
    let copy_id = 7;
    let expected = format!("prefix {hidden} suffix");
    let result = composer.copy_selection(
        &TuiEvent::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::SUPER)),
        |text| {
            assert_eq!(text, expected);
            Ok(CopyStatus::Pending(copy_id))
        },
    );
    assert_eq!(
        result,
        Some((expected.chars().count(), Ok(CopyStatus::Pending(copy_id))))
    );
    assert_eq!(
        composer.draft.textarea.mouse_selection_range(),
        Some(0..visible.len())
    );
    assert_eq!(
        composer.finish_copy(&(copy_id, Ok(CopyStatus::Confirmed)), /*current*/ true),
        Some(expected.chars().count())
    );
    assert_eq!(composer.draft.textarea.mouse_selection_range(), None);
    assert_eq!(
        (
            composer.draft.textarea.text(),
            composer.draft.textarea.cursor(),
            composer.draft.pending_pastes.clone(),
        ),
        (visible.as_str(), cursor, pending)
    );
}

#[test]
fn partial_mouse_copy_keeps_existing_slice_behavior_with_hidden_paste() {
    let (mut composer, _rx) = new_test_composer();
    composer.insert_str("prefix ");
    composer.handle_paste("hidden\n".repeat(LARGE_PASTE_CHAR_THRESHOLD));
    composer.insert_str(" tail");
    let visible = composer.draft.textarea.text().to_string();
    let pending = composer.draft.pending_pastes.clone();
    composer.draft.textarea.set_cursor(0);
    let area = Rect::new(0, 0, 80, 6);
    composer.render(area, &mut Buffer::empty(area));
    let (x, y) = composer.cursor_pos(area).unwrap();
    for (kind, column) in [
        (MouseEventKind::Down(MouseButton::Left), x),
        (MouseEventKind::Drag(MouseButton::Left), x + 6),
        (MouseEventKind::Up(MouseButton::Left), x + 6),
    ] {
        let event = MouseEvent {
            kind,
            column,
            row: y,
            modifiers: KeyModifiers::NONE,
        };
        assert!(composer.prepare_mouse(event));
        composer.render(area, &mut Buffer::empty(area));
        assert!(composer.handle_mouse(event));
    }
    assert_eq!(composer.draft.textarea.mouse_selection_range(), Some(0..6));
    let mut copied = String::new();
    composer.copy_selection(
        &TuiEvent::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::SUPER)),
        |text| {
            copied = text.to_string();
            Ok(CopyStatus::Confirmed)
        },
    );
    assert_eq!(copied, "prefix");
    assert_eq!(composer.draft.textarea.text(), visible);
    assert_eq!(composer.draft.pending_pastes, pending);
}

#[test]
fn super_a_native_typing_replaces_the_selected_draft() {
    for replacement in ['x', 'Я'] {
        let (mut composer, _rx) = new_test_composer();
        composer
            .draft
            .textarea
            .set_text_clearing_elements("старый\nтекст");
        composer.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SUPER));
        composer.handle_key_event(KeyEvent::new(
            KeyCode::Char(replacement),
            KeyModifiers::NONE,
        ));
        composer.handle_paste_burst_flush(Instant::now() + PasteBurst::recommended_flush_delay());
        assert_eq!(composer.draft.textarea.text(), replacement.to_string());
        assert_eq!(composer.draft.textarea.mouse_selection_range(), None);
    }
}

#[test]
fn super_a_backspace_clears_hidden_paste_payload_and_all_visible_text() {
    let (mut composer, _rx) = new_test_composer();
    let hidden = "длинная строка\n".repeat(LARGE_PASTE_CHAR_THRESHOLD);
    composer.handle_paste(hidden.clone());
    composer.insert_str(" хвост");
    assert_eq!(composer.draft.pending_pastes.len(), 1);
    assert!(composer.current_text_with_pending().contains(&hidden));

    composer.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SUPER));
    composer.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(composer.draft.textarea.text(), "");
    assert_eq!(composer.draft.textarea.mouse_selection_range(), None);
    assert!(composer.draft.pending_pastes.is_empty());
    assert_eq!(composer.current_text_with_pending(), "");
}

#[test]
fn super_a_flushes_pending_paste_burst_before_selecting_everything() {
    let (mut composer, _rx) = new_test_composer();
    composer
        .draft
        .textarea
        .set_text_clearing_elements("начало ");
    composer.draft.textarea.set_cursor("начало ".len());
    let now = Instant::now();
    for ch in ['a', 'b'] {
        composer.handle_input_basic_with_time(
            KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            now,
        );
    }
    assert!(composer.is_in_paste_burst());

    composer.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SUPER));
    assert_eq!(
        (
            composer.draft.textarea.text(),
            composer.draft.textarea.mouse_selection_range(),
        ),
        ("начало ab", Some(0.."начало ab".len()))
    );
    assert!(!composer.is_in_paste_burst());
    assert!(!composer.handle_paste_burst_flush(now + PasteBurst::recommended_flush_delay()));
    composer.handle_paste("замена".to_string());
    assert_eq!(composer.draft.textarea.text(), "замена");
}

#[test]
fn super_a_empty_draft_keeps_selection_empty_and_accepts_typing() {
    let (mut composer, _rx) = new_test_composer();
    let (result, redraw) =
        composer.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SUPER));
    assert!(matches!(result, InputResult::None));
    assert!(redraw);
    assert_eq!(
        (
            composer.draft.textarea.text(),
            composer.draft.textarea.cursor(),
            composer.draft.textarea.mouse_selection_range(),
        ),
        ("", 0, None)
    );
    composer.handle_key_event(KeyEvent::new(KeyCode::Char('Я'), KeyModifiers::NONE));
    assert_eq!(composer.draft.textarea.text(), "Я");
}

#[test]
fn super_a_does_not_change_ctrl_a_or_home() {
    for key in [
        KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
        KeyEvent::new(KeyCode::Home, KeyModifiers::NONE),
    ] {
        let (mut composer, _rx) = new_test_composer();
        composer
            .draft
            .textarea
            .set_text_clearing_elements("one\ntwo");
        composer.draft.textarea.set_cursor("one\ntwo".len());
        composer.handle_key_event(key);
        assert_eq!(
            (
                composer.draft.textarea.text(),
                composer.draft.textarea.cursor(),
                composer.draft.textarea.mouse_selection_range(),
            ),
            ("one\ntwo", "one\n".len(), None)
        );
    }
}

#[test]
fn super_a_ignores_release_other_modifiers_and_disabled_input() {
    for event in [
        KeyEvent::new_with_kind(
            KeyCode::Char('a'),
            KeyModifiers::SUPER,
            KeyEventKind::Release,
        ),
        KeyEvent::new(
            KeyCode::Char('a'),
            KeyModifiers::SUPER | KeyModifiers::SHIFT,
        ),
    ] {
        let (mut composer, _rx) = new_test_composer();
        composer.draft.textarea.set_text_clearing_elements("текст");
        composer.handle_key_event(event);
        assert_eq!(composer.draft.textarea.mouse_selection_range(), None);
        assert_eq!(composer.draft.textarea.text(), "текст");
    }
    let (mut composer, _rx) = new_test_composer();
    composer.draft.textarea.set_text_clearing_elements("текст");
    composer.draft.input_enabled = false;
    composer.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SUPER));
    assert_eq!(composer.draft.textarea.mouse_selection_range(), None);
}

#[test]
fn super_a_multiline_selection_snapshot() {
    let (mut composer, _rx) = new_test_composer();
    composer
        .draft
        .textarea
        .set_text_clearing_elements("Привет 👋\nвесь черновик выделен");
    composer.draft.textarea.set_cursor("При".len());
    composer.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SUPER));
    let area = Rect::new(0, 0, 40, 8);
    let mut buffer = Buffer::empty(area);
    composer.render(area, &mut buffer);
    insta::assert_snapshot!("super_a_multiline_selection", format!("{buffer:?}"));
}
