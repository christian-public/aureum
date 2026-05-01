use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(crate) fn is_quit_key(key: &KeyEvent) -> bool {
    key.code == KeyCode::Char('q')
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}
