use std::io::{self, Stderr};
use std::time::{Duration, Instant};

use chrono::Utc;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use thiserror::Error;

use crate::model::Database;
use app::{Action, App, ClickButton, Outcome};
use view::{ViewLayout, layout_for};

pub mod app;
pub mod view;

#[derive(Debug, Error)]
pub enum UiError {
    #[error("terminal I/O failed: {0}")]
    Io(#[from] io::Error),
}

/// Runs the interactive selector without writing terminal control bytes to stdout.
pub fn run(database: Database) -> Result<Outcome, UiError> {
    let mut terminal = TerminalGuard::enter()?;
    let mut app = App::from_database_at(database, Utc::now());
    let started_at = Instant::now();

    let outcome = loop {
        terminal
            .terminal_mut()
            .draw(|frame| view::render(frame, &mut app))?;
        let area = terminal.terminal_mut().size()?;
        let action = match event::read()? {
            Event::Key(key) => map_key_event(key, app.is_searching()),
            Event::Mouse(mouse) => mouse_action(
                &app,
                layout_for(ratatui::layout::Rect::new(0, 0, area.width, area.height)),
                mouse,
                started_at.elapsed(),
            ),
            Event::Resize(_, _) | Event::FocusGained | Event::FocusLost | Event::Paste(_) => None,
        };
        if let Some(action) = action {
            match app.handle(action, Utc::now()) {
                Outcome::Continue => {}
                finished => break finished,
            }
        }
    };

    terminal.restore()?;
    Ok(outcome)
}

pub(crate) fn map_key_event(event: KeyEvent, searching: bool) -> Option<Action> {
    if !matches!(event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return None;
    }
    let plain_or_shift = event.modifiers.is_empty() || event.modifiers == KeyModifiers::SHIFT;
    match event.code {
        KeyCode::Up => Some(Action::Up),
        KeyCode::Down => Some(Action::Down),
        KeyCode::Enter => Some(Action::Confirm),
        KeyCode::Esc => Some(Action::Cancel),
        KeyCode::Tab | KeyCode::BackTab => Some(Action::TogglePane),
        KeyCode::Backspace if searching => Some(Action::Backspace),
        KeyCode::Char(character) if searching && plain_or_shift => Some(Action::Input(character)),
        KeyCode::Char('/') if plain_or_shift => Some(Action::StartSearch),
        KeyCode::Char('k') if plain_or_shift => Some(Action::Up),
        KeyCode::Char('j') if plain_or_shift => Some(Action::Down),
        _ => None,
    }
}

pub(crate) fn mouse_action(
    app: &App,
    layout: ViewLayout,
    event: MouseEvent,
    elapsed: Duration,
) -> Option<Action> {
    let MouseEventKind::Down(button) = event.kind else {
        return None;
    };
    let button = match button {
        MouseButton::Left => ClickButton::Left,
        MouseButton::Right => ClickButton::Right,
        MouseButton::Middle => ClickButton::Middle,
    };

    let category_content = layout.category_content();
    if let Some(line) = row_in(category_content, event.column, event.row) {
        let row = app.category_offset().checked_add(line)?;
        if row < app.categories().len() {
            return Some(Action::ClickCategory { row });
        }
        return None;
    }

    let bookmark_content = layout.bookmark_content();
    let line = row_in(bookmark_content, event.column, event.row)?;
    let row = app.bookmark_offset().checked_add(line)?;
    (row < app.visible_bookmarks().len()).then_some(Action::ClickBookmark {
        row,
        button,
        elapsed,
    })
}

fn row_in(area: ratatui::layout::Rect, column: u16, row: u16) -> Option<usize> {
    let right = area.x.saturating_add(area.width);
    let bottom = area.y.saturating_add(area.height);
    (area.width > 0
        && area.height > 0
        && column >= area.x
        && column < right
        && row >= area.y
        && row < bottom)
        .then(|| usize::from(row - area.y))
}

type StderrTerminal = Terminal<CrosstermBackend<Stderr>>;

struct TerminalGuard {
    terminal: StderrTerminal,
    restored: bool,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stderr = io::stderr();
        if let Err(error) = execute!(stderr, EnterAlternateScreen, EnableMouseCapture) {
            let _ = execute!(io::stderr(), LeaveAlternateScreen, DisableMouseCapture);
            let _ = disable_raw_mode();
            return Err(error);
        }

        match Terminal::new(CrosstermBackend::new(stderr)) {
            Ok(terminal) => Ok(Self {
                terminal,
                restored: false,
            }),
            Err(error) => {
                let _ = execute!(io::stderr(), LeaveAlternateScreen, DisableMouseCapture);
                let _ = disable_raw_mode();
                Err(error)
            }
        }
    }

    fn terminal_mut(&mut self) -> &mut StderrTerminal {
        &mut self.terminal
    }

    fn restore(&mut self) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }
        let cursor_result = self.terminal.show_cursor();
        let screen_result = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
        let raw_result = disable_raw_mode();
        self.restored = true;
        cursor_result.and(screen_result).and(raw_result)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::{map_key_event, mouse_action};
    use crate::model::{Bookmark, Database};
    use crate::ui::app::{Action, App, ClickButton};
    use crate::ui::view::layout_for;
    use chrono::{TimeZone, Utc};
    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::layout::Rect;
    use std::path::PathBuf;
    use std::time::Duration;
    use uuid::Uuid;

    #[test]
    fn keyboard_mapping_covers_navigation_selection_and_search_modes() {
        assert_eq!(key(KeyCode::Up, false), Some(Action::Up));
        assert_eq!(key(KeyCode::Char('k'), false), Some(Action::Up));
        assert_eq!(key(KeyCode::Down, false), Some(Action::Down));
        assert_eq!(key(KeyCode::Char('j'), false), Some(Action::Down));
        assert_eq!(key(KeyCode::Enter, false), Some(Action::Confirm));
        assert_eq!(key(KeyCode::Esc, false), Some(Action::Cancel));
        assert_eq!(key(KeyCode::Tab, false), Some(Action::TogglePane));
        assert_eq!(key(KeyCode::Char('/'), false), Some(Action::StartSearch));
        assert_eq!(key(KeyCode::Char('j'), true), Some(Action::Input('j')));
        assert_eq!(key(KeyCode::Char('/'), true), Some(Action::Input('/')));
        assert_eq!(key(KeyCode::Backspace, true), Some(Action::Backspace));
        assert_eq!(key(KeyCode::Backspace, false), None);
    }

    #[test]
    fn mouse_coordinates_include_scroll_offset_and_reject_non_visible_rows() {
        let mut app = fixture_app(6);
        app.set_viewport_rows(3, 3);
        for _ in 0..5 {
            app.handle(Action::Down, test_now());
        }
        assert_eq!(app.bookmark_offset(), 3);
        let layout = layout_for(Rect::new(0, 0, 80, 10));
        let content = layout.bookmark_content();

        assert_eq!(
            mouse_action(
                &app,
                layout,
                mouse_down(MouseButton::Left, content.x, content.y),
                Duration::from_millis(100),
            ),
            Some(Action::ClickBookmark {
                row: 3,
                button: ClickButton::Left,
                elapsed: Duration::from_millis(100),
            })
        );
        assert_eq!(
            mouse_action(
                &app,
                layout,
                mouse_down(
                    MouseButton::Left,
                    content.x,
                    content.y.saturating_add(content.height),
                ),
                Duration::ZERO,
            ),
            None
        );
    }

    #[test]
    fn mouse_clicks_on_blank_lines_and_tiny_views_do_not_map_to_items() {
        let app = fixture_app(1);
        let layout = layout_for(Rect::new(0, 0, 80, 10));
        let content = layout.bookmark_content();
        assert_eq!(
            mouse_action(
                &app,
                layout,
                mouse_down(MouseButton::Left, content.x, content.y + 1),
                Duration::ZERO,
            ),
            None
        );

        let tiny = layout_for(Rect::new(0, 0, 1, 1));
        assert_eq!(
            mouse_action(
                &app,
                tiny,
                mouse_down(MouseButton::Left, 0, 0),
                Duration::ZERO,
            ),
            None
        );
    }

    #[test]
    fn category_mouse_coordinates_respect_category_scroll() {
        let mut app = fixture_app(1);
        app.set_viewport_rows(3, 3);
        app.handle(Action::TogglePane, test_now());
        for _ in 0..5 {
            app.handle(Action::Down, test_now());
        }
        assert_eq!(app.category_offset(), 3);
        let layout = layout_for(Rect::new(0, 0, 80, 10));
        let content = layout.category_content();
        assert_eq!(
            mouse_action(
                &app,
                layout,
                mouse_down(MouseButton::Left, content.x, content.y),
                Duration::ZERO,
            ),
            Some(Action::ClickCategory { row: 3 })
        );
    }

    fn key(code: KeyCode, searching: bool) -> Option<Action> {
        map_key_event(KeyEvent::new(code, KeyModifiers::NONE), searching)
    }

    fn mouse_down(button: MouseButton, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(button),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn fixture_app(bookmark_count: usize) -> App {
        let categories = (0..5).map(|index| format!("category-{index}")).collect();
        let bookmarks = (0..bookmark_count)
            .map(|index| Bookmark {
                id: Uuid::new_v4(),
                name: format!("bookmark-{index}"),
                path: PathBuf::from(format!("missing-{index}")),
                category: "category-0".into(),
                created_at: test_now(),
                last_visited_at: None,
                visit_count: 0,
            })
            .collect();
        App::from_database_at(
            Database {
                version: 1,
                categories,
                bookmarks,
            },
            test_now(),
        )
    }

    fn test_now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 31, 12, 0, 0).unwrap()
    }
}
