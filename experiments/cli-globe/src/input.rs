//! Keyboard mapping kept separate from application behavior.

use crate::{
    app::{Action, App, Overlay},
    gfx::GfxBackend,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn key_action(app: &App, key: KeyEvent) -> Option<Action> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(Action::Quit);
    }

    if app.overlay == Overlay::Privacy {
        return match key.code {
            KeyCode::Char('a') | KeyCode::Enter => {
                Some(Action::AcceptPrivacy { local_only: false })
            }
            KeyCode::Char('l') => Some(Action::AcceptPrivacy { local_only: true }),
            KeyCode::Char('q') | KeyCode::Esc => Some(Action::Quit),
            _ => None,
        };
    }

    if app.filtering {
        return match key.code {
            KeyCode::Esc | KeyCode::Enter => Some(Action::SubmitFilter),
            KeyCode::Backspace => Some(Action::FilterBackspace),
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::FilterClear)
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::FilterChar(ch))
            }
            _ => None,
        };
    }

    if app.overlay == Overlay::Settings {
        return match key.code {
            KeyCode::Char('1') => Some(Action::ToggleSetting(1)),
            KeyCode::Char('2') => Some(Action::ToggleSetting(2)),
            KeyCode::Char('3') => Some(Action::ToggleSetting(3)),
            KeyCode::Char('4') => Some(Action::ToggleSetting(4)),
            KeyCode::Char('s') => Some(Action::ToggleSettings),
            KeyCode::Esc => Some(Action::Clear),
            _ => None,
        };
    }

    if app.overlay == Overlay::Help {
        return match key.code {
            KeyCode::Char('?') => Some(Action::ToggleHelp),
            KeyCode::Esc => Some(Action::Clear),
            _ => None,
        };
    }

    if app.overlay == Overlay::Debug {
        return match key.code {
            KeyCode::Char('b') => Some(Action::SetBackend(GfxBackend::Braille)),
            KeyCode::Char('h') => Some(Action::SetBackend(GfxBackend::Halfblocks)),
            KeyCode::Char('k') => Some(Action::SetBackend(GfxBackend::Kitty)),
            KeyCode::Char('d') => Some(Action::ToggleDebug),
            KeyCode::Esc => Some(Action::Clear),
            _ => None,
        };
    }

    match key.code {
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Esc => Some(Action::Clear),
        KeyCode::Tab => Some(Action::NextPane),
        KeyCode::BackTab => Some(Action::PreviousPane),
        KeyCode::Up => Some(Action::Up),
        KeyCode::Down => Some(Action::Down),
        KeyCode::Left => Some(Action::Left),
        KeyCode::Right => Some(Action::Right),
        KeyCode::Enter => Some(Action::Activate),
        KeyCode::Char(' ') => Some(Action::ToggleFocus),
        KeyCode::Char('/') => Some(Action::StartFilter),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::Char('d') => Some(Action::ToggleDebug),
        KeyCode::Char('s') => Some(Action::ToggleSettings),
        KeyCode::Char('r') => Some(Action::Recenter),
        KeyCode::Char('t') => Some(Action::TraceAll),
        KeyCode::Char('R') => Some(Action::Reset),
        KeyCode::Char('g') => Some(Action::CycleDensity),
        KeyCode::Char('l') => Some(Action::ToggleLabels),
        KeyCode::Char('a') => Some(Action::ToggleFlow),
        KeyCode::Char('+') | KeyCode::Char('=') => Some(Action::ZoomIn),
        KeyCode::Char('-') | KeyCode::Char('_') => Some(Action::ZoomOut),
        KeyCode::Char('0') => Some(Action::ZoomReset),
        KeyCode::Char('p') => Some(Action::ToggleSpin),
        _ => None,
    }
}
