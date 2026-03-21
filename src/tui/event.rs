use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum AppEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
    Tick,
}

pub fn poll_event(timeout: Duration) -> anyhow::Result<Option<AppEvent>> {
    if event::poll(timeout)? {
        match event::read()? {
            Event::Key(key) => {
                if key.kind == crossterm::event::KeyEventKind::Press {
                    Ok(Some(AppEvent::Key(key)))
                } else {
                    Ok(None)
                }
            }
            Event::Mouse(mouse) => Ok(Some(AppEvent::Mouse(mouse))),
            Event::Resize(w, h) => Ok(Some(AppEvent::Resize(w, h))),
            _ => Ok(None),
        }
    } else {
        Ok(Some(AppEvent::Tick))
    }
}

pub fn is_quit(key: &KeyEvent) -> bool {
    matches!(
        (key.code, key.modifiers),
        (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Char('q'), KeyModifiers::CONTROL)
    )
}

pub fn is_submit(key: &KeyEvent) -> bool {
    key.code == KeyCode::Enter && !key.modifiers.contains(KeyModifiers::SHIFT)
}

pub fn is_newline(key: &KeyEvent) -> bool {
    key.code == KeyCode::Enter && key.modifiers.contains(KeyModifiers::SHIFT)
}

pub fn is_copy(key: &KeyEvent) -> bool {
    key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::SUPER)
        || key.code == KeyCode::Char('c')
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && key.modifiers.contains(KeyModifiers::SHIFT)
}

pub fn is_paste(key: &KeyEvent) -> bool {
    key.code == KeyCode::Char('v') && key.modifiers.contains(KeyModifiers::SUPER)
        || key.code == KeyCode::Char('v')
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && key.modifiers.contains(KeyModifiers::SHIFT)
}

pub fn is_scroll_up(mouse: &MouseEvent) -> bool {
    matches!(mouse.kind, MouseEventKind::ScrollUp)
}

pub fn is_scroll_down(mouse: &MouseEvent) -> bool {
    matches!(mouse.kind, MouseEventKind::ScrollDown)
}
