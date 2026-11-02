//! 键位表：KeyEvent -> Action。纯函数，好测。

use crate::action::Action;
use crate::mode::{InputKind, Mode, OverlayKind};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn is_ctrl(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
}

fn is_alt(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::ALT)
}

pub fn key_to_action(mode: Mode, key: KeyEvent) -> Option<Action> {
    match mode {
        Mode::Browse => browse_action(key),
        Mode::Input(kind) => input_action(kind, key),
        Mode::Overlay(kind) => overlay_action(kind, key),
    }
}

fn browse_action(key: KeyEvent) -> Option<Action> {
    if is_ctrl(&key) {
        return match key.code {
            KeyCode::Char('l') => Some(Action::FocusOmnibox),
            KeyCode::Char('h') => Some(Action::ToggleHistory),
            KeyCode::Char('p') => Some(Action::TogglePalette),
            KeyCode::Char('a') => Some(Action::ToggleAi),
            KeyCode::Char('t') => Some(Action::TabNew),
            KeyCode::Char('w') => Some(Action::TabClose),
            KeyCode::Char('r') => Some(Action::ToggleMarkdownView),
            KeyCode::Char('e') => Some(Action::ExportMarkdown),
            KeyCode::Char('s') => Some(Action::ExportHtml),
            KeyCode::Char('q') => Some(Action::ForceQuit),
            KeyCode::Char('c') => Some(Action::CancelInput),
            KeyCode::Tab => Some(Action::TabNext),
            KeyCode::Enter => Some(Action::OpenFocusedLink { new_tab: true }),
            _ => None,
        };
    }
    if is_alt(&key) {
        return match key.code {
            KeyCode::Left => Some(Action::Back),
            KeyCode::Right => Some(Action::Forward),
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let n = c.to_digit(10).unwrap_or(0) as usize;
                if n == 0 {
                    Some(Action::TabSelect(9))
                } else {
                    Some(Action::TabSelect(n - 1))
                }
            }
            _ => None,
        };
    }
    match key.code {
        KeyCode::Char('j') => Some(Action::ScrollDown(1)),
        KeyCode::Char('k') => Some(Action::ScrollUp(1)),
        KeyCode::Char(' ') => Some(Action::PageDown),
        KeyCode::Char('g') => Some(Action::PageTop),
        KeyCode::Char('G') => Some(Action::PageBottom),
        KeyCode::Char('b') => Some(Action::Back),
        KeyCode::Char('f') => Some(Action::Forward),
        KeyCode::Char('r') => Some(Action::Reload),
        KeyCode::Char('l') => Some(Action::FocusOmnibox),
        KeyCode::Char('/') => Some(Action::FocusFind),
        KeyCode::Char('n') => Some(Action::FindNext),
        KeyCode::Char('N') => Some(Action::FindPrev),
        KeyCode::Char('L') => Some(Action::ToggleLinks),
        KeyCode::Char('v') => Some(Action::ToggleRawView),
        KeyCode::Char('e') => Some(Action::OpenHumanAssist),
        KeyCode::Char('c') => Some(Action::PasteCookies),
        KeyCode::Char('y') => Some(Action::CopyUrl),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Char(':') => Some(Action::FocusCommand),
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Enter => Some(Action::OpenFocusedLink { new_tab: false }),
        KeyCode::Tab => Some(Action::FocusNextLink),
        KeyCode::BackTab => Some(Action::FocusPrevLink),
        KeyCode::Esc => Some(Action::CancelInput),
        KeyCode::Down => Some(Action::ScrollDown(1)),
        KeyCode::Up => Some(Action::ScrollUp(1)),
        KeyCode::PageDown => Some(Action::PageDown),
        KeyCode::PageUp => Some(Action::PageTop),
        KeyCode::Left => Some(Action::Back),
        KeyCode::Right => Some(Action::Forward),
        KeyCode::Home => Some(Action::PageTop),
        KeyCode::End => Some(Action::PageBottom),
        _ => None,
    }
}

fn input_action(kind: InputKind, key: KeyEvent) -> Option<Action> {
    if is_ctrl(&key) {
        return match key.code {
            KeyCode::Char('c') => Some(Action::CancelInput),
            KeyCode::Char('u') => Some(Action::ClearInput),
            KeyCode::Char('w') => Some(Action::EditWordBackspace),
            KeyCode::Char('a') => Some(Action::EditHome),
            KeyCode::Char('e') => Some(Action::EditEnd),
            _ => None,
        };
    }
    match key.code {
        KeyCode::Enter => Some(Action::Submit),
        KeyCode::Esc => Some(Action::CancelInput),
        KeyCode::Backspace => Some(Action::EditBackspace),
        KeyCode::Delete => Some(Action::EditBackspace),
        KeyCode::Left => Some(Action::EditLeft),
        KeyCode::Right => Some(Action::EditRight),
        KeyCode::Home => Some(Action::EditHome),
        KeyCode::End => Some(Action::EditEnd),
        KeyCode::Up => Some(Action::HistoryPrev),
        KeyCode::Down => Some(Action::HistoryNext),
        KeyCode::Tab if kind == InputKind::Omnibox => Some(Action::Submit),
        _ => None,
    }
}

fn overlay_action(kind: OverlayKind, key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::CloseOverlay),
        KeyCode::Char('q') => Some(Action::CloseOverlay),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::OverlayDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::OverlayUp),
        KeyCode::Enter => Some(Action::OverlaySelect),
        KeyCode::Char('/') if kind == OverlayKind::History => Some(Action::OverlayFilter),
        KeyCode::Char('L') if kind == OverlayKind::Links => Some(Action::CloseOverlay),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    fn alt(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::ALT)
    }

    fn browse(code: KeyCode) -> Option<Action> {
        key_to_action(Mode::Browse, key(code))
    }

    #[test]
    fn browse_navigation_keys() {
        assert_eq!(browse(KeyCode::Char('b')), Some(Action::Back));
        assert_eq!(browse(KeyCode::Char('f')), Some(Action::Forward));
        assert_eq!(browse(KeyCode::Char('r')), Some(Action::Reload));
        assert_eq!(browse(KeyCode::Char('l')), Some(Action::FocusOmnibox));
        assert_eq!(browse(KeyCode::Char('j')), Some(Action::ScrollDown(1)));
        assert_eq!(browse(KeyCode::Char('k')), Some(Action::ScrollUp(1)));
        assert_eq!(
            browse(KeyCode::Enter),
            Some(Action::OpenFocusedLink { new_tab: false })
        );
        assert_eq!(browse(KeyCode::Tab), Some(Action::FocusNextLink));
        assert_eq!(browse(KeyCode::BackTab), Some(Action::FocusPrevLink));
    }

    #[test]
    fn ctrl_and_alt_shortcuts() {
        assert_eq!(
            key_to_action(Mode::Browse, ctrl(KeyCode::Char('l'))),
            Some(Action::FocusOmnibox)
        );
        assert_eq!(
            key_to_action(Mode::Browse, ctrl(KeyCode::Enter)),
            Some(Action::OpenFocusedLink { new_tab: true })
        );
        assert_eq!(
            key_to_action(Mode::Browse, ctrl(KeyCode::Char('t'))),
            Some(Action::TabNew)
        );
        assert_eq!(
            key_to_action(Mode::Browse, alt(KeyCode::Char('1'))),
            Some(Action::TabSelect(0))
        );
        assert_eq!(
            key_to_action(Mode::Browse, alt(KeyCode::Char('0'))),
            Some(Action::TabSelect(9))
        );
        assert_eq!(
            key_to_action(Mode::Browse, alt(KeyCode::Left)),
            Some(Action::Back)
        );
    }

    #[test]
    fn input_mode_editing() {
        let mode = Mode::Input(InputKind::Omnibox);
        assert_eq!(
            key_to_action(mode, key(KeyCode::Enter)),
            Some(Action::Submit)
        );
        assert_eq!(
            key_to_action(mode, key(KeyCode::Esc)),
            Some(Action::CancelInput)
        );
        assert_eq!(
            key_to_action(mode, key(KeyCode::Backspace)),
            Some(Action::EditBackspace)
        );
        assert_eq!(
            key_to_action(mode, ctrl(KeyCode::Char('w'))),
            Some(Action::EditWordBackspace)
        );
        assert_eq!(
            key_to_action(mode, ctrl(KeyCode::Char('c'))),
            Some(Action::CancelInput)
        );
        assert_eq!(
            key_to_action(mode, key(KeyCode::Up)),
            Some(Action::HistoryPrev)
        );
        assert_eq!(
            key_to_action(mode, key(KeyCode::Char('a'))),
            None,
            "plain chars are handled by the app"
        );
    }

    #[test]
    fn overlay_mode_keys() {
        let mode = Mode::Overlay(OverlayKind::History);
        assert_eq!(
            key_to_action(mode, key(KeyCode::Esc)),
            Some(Action::CloseOverlay)
        );
        assert_eq!(
            key_to_action(mode, key(KeyCode::Down)),
            Some(Action::OverlayDown)
        );
        assert_eq!(
            key_to_action(mode, key(KeyCode::Enter)),
            Some(Action::OverlaySelect)
        );
        assert_eq!(
            key_to_action(mode, key(KeyCode::Char('/'))),
            Some(Action::OverlayFilter)
        );
    }

    #[test]
    fn browse_single_letter_bindings_are_unique() {
        let letters = "jkbgfrlLvcnNyq?e";
        let mut seen = std::collections::HashSet::new();
        for ch in letters.chars() {
            let action = browse(KeyCode::Char(ch));
            assert!(action.is_some(), "unbound letter {ch}");
            assert!(seen.insert(ch), "duplicate binding {ch}");
        }
    }

    #[test]
    fn every_documented_browse_key_maps_to_something() {
        let documented = [
            KeyCode::Char('j'),
            KeyCode::Char('k'),
            KeyCode::Char(' '),
            KeyCode::Char('g'),
            KeyCode::Char('G'),
            KeyCode::Char('b'),
            KeyCode::Char('f'),
            KeyCode::Char('r'),
            KeyCode::Char('l'),
            KeyCode::Char('/'),
            KeyCode::Char('n'),
            KeyCode::Char('N'),
            KeyCode::Char('L'),
            KeyCode::Char('v'),
            KeyCode::Char('e'),
            KeyCode::Char('c'),
            KeyCode::Char('y'),
            KeyCode::Char('?'),
            KeyCode::Char(':'),
            KeyCode::Char('q'),
            KeyCode::Enter,
            KeyCode::Tab,
            KeyCode::BackTab,
            KeyCode::Esc,
        ];
        for code in documented {
            assert!(browse(code).is_some(), "documented key {code:?} is unbound");
        }
    }
}
