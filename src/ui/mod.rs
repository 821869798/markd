use std::io::{self, Stderr, Write};
use std::time::{Duration, Instant};

use chrono::Utc;
use crossterm::cursor::Show;
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
    #[error("persistence failed: {0}")]
    Persistence(String),
}

pub trait UiRunner {
    fn run(&mut self, database: Database) -> Result<Outcome, UiError>;
}

/// Runs the selector with a callback used to persist management changes.
pub fn run_with<F>(database: Database, mut persist: F) -> Result<Outcome, UiError>
where
    F: FnMut(&Database) -> Result<(), String>,
{
    let mut terminal = TerminalGuard::enter()?;
    let mut app = App::from_database_at(database, Utc::now());
    let started_at = Instant::now();

    let outcome = loop {
        terminal
            .terminal_mut()
            .draw(|frame| view::render(frame, &mut app))?;
        let area = terminal.terminal_mut().size()?;
        let action = match event::read()? {
            Event::Key(key) => key_action(key, &app),
            Event::Mouse(mouse) => mouse_action(
                &app,
                layout_for(ratatui::layout::Rect::new(0, 0, area.width, area.height)),
                mouse,
                started_at.elapsed(),
            ),
            Event::Resize(_, _) | Event::FocusGained | Event::FocusLost | Event::Paste(_) => None,
        };
        if let Some(action) = action {
            let before = app.database_snapshot();
            match app.handle(action, Utc::now()) {
                Outcome::Continue => {
                    let after = app.database_snapshot();
                    if after != before
                        && let Err(error) = persist(&after)
                    {
                        app.restore_database(before, Utc::now(), error);
                    }
                }
                finished => break finished,
            }
        }
    };

    terminal.restore()?;
    Ok(outcome)
}

/// Runs the interactive selector without writing terminal control bytes to stdout.
pub fn run(database: Database) -> Result<Outcome, UiError> {
    run_with(database, |_| Ok(()))
}

fn key_action(event: KeyEvent, app: &App) -> Option<Action> {
    if !matches!(event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return None;
    }
    if app.is_editing() {
        return match event.code {
            KeyCode::Enter => app.commit_editing_action(),
            KeyCode::Esc => Some(Action::Cancel),
            KeyCode::Backspace => Some(Action::Backspace),
            KeyCode::Char(character)
                if event.modifiers.is_empty() || event.modifiers == KeyModifiers::SHIFT =>
            {
                Some(Action::Input(character))
            }
            _ => None,
        };
    }
    if app.is_confirming_delete() && matches!(event.code, KeyCode::Char('d')) {
        return Some(Action::ConfirmDelete(true));
    }
    map_key_event(event, app.is_searching())
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
        KeyCode::Char('d') if !searching && plain_or_shift => Some(Action::DeleteSelected),
        KeyCode::Char('e') if !searching && plain_or_shift => Some(Action::BeginRename),
        KeyCode::Char('c') if !searching && plain_or_shift => Some(Action::BeginCreateCategory),
        KeyCode::Char('r') if !searching && plain_or_shift => Some(Action::BeginRenameCategory),
        KeyCode::Char('D') if !searching && plain_or_shift => Some(Action::DeleteCategory),
        KeyCode::Char('m') if !searching && plain_or_shift => Some(Action::BeginMoveBookmark),
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

#[derive(Debug, Clone, Copy, Default)]
struct CleanupState {
    raw_mode: bool,
    alternate_screen: bool,
    mouse_capture: bool,
    show_cursor: bool,
}

trait CleanupOperations {
    fn disable_mouse_capture(&mut self) -> io::Result<()>;
    fn leave_alternate_screen(&mut self) -> io::Result<()>;
    fn show_cursor(&mut self) -> io::Result<()>;
    fn disable_raw_mode(&mut self) -> io::Result<()>;
}

fn cleanup_best_effort(
    operations: &mut impl CleanupOperations,
    state: CleanupState,
) -> io::Result<()> {
    let mut first_error = None;
    if state.mouse_capture {
        preserve_first_error(&mut first_error, operations.disable_mouse_capture());
    }
    if state.alternate_screen {
        preserve_first_error(&mut first_error, operations.leave_alternate_screen());
    }
    if state.show_cursor {
        preserve_first_error(&mut first_error, operations.show_cursor());
    }
    if state.raw_mode {
        preserve_first_error(&mut first_error, operations.disable_raw_mode());
    }
    first_error.map_or(Ok(()), Err)
}

fn preserve_first_error(first_error: &mut Option<io::Error>, result: io::Result<()>) {
    if let Err(error) = result
        && first_error.is_none()
    {
        *first_error = Some(error);
    }
}

struct CrosstermCleanup<'a, W> {
    writer: &'a mut W,
}

impl<W: Write> CleanupOperations for CrosstermCleanup<'_, W> {
    fn disable_mouse_capture(&mut self) -> io::Result<()> {
        execute!(self.writer, DisableMouseCapture)
    }

    fn leave_alternate_screen(&mut self) -> io::Result<()> {
        execute!(self.writer, LeaveAlternateScreen)
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        execute!(self.writer, Show)
    }

    fn disable_raw_mode(&mut self) -> io::Result<()> {
        disable_raw_mode()
    }
}

fn cleanup_writer(writer: &mut impl Write, state: CleanupState) -> io::Result<()> {
    cleanup_best_effort(&mut CrosstermCleanup { writer }, state)
}

type StderrTerminal = Terminal<CrosstermBackend<Stderr>>;

struct TerminalGuard {
    terminal: StderrTerminal,
    cleanup_state: CleanupState,
    restored: bool,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        let mut cleanup_state = CleanupState::default();
        enable_raw_mode()?;
        cleanup_state.raw_mode = true;

        let mut stderr = io::stderr();
        cleanup_state.alternate_screen = true;
        if let Err(error) = execute!(stderr, EnterAlternateScreen) {
            let _ = cleanup_writer(&mut stderr, cleanup_state);
            return Err(error);
        }

        cleanup_state.mouse_capture = true;
        if let Err(error) = execute!(stderr, EnableMouseCapture) {
            let _ = cleanup_writer(&mut stderr, cleanup_state);
            return Err(error);
        }

        cleanup_state.show_cursor = true;
        match Terminal::new(CrosstermBackend::new(stderr)) {
            Ok(terminal) => Ok(Self {
                terminal,
                cleanup_state,
                restored: false,
            }),
            Err(error) => {
                let _ = cleanup_writer(&mut io::stderr(), cleanup_state);
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
        self.restored = true;
        let cleanup_state = std::mem::take(&mut self.cleanup_state);
        cleanup_writer(self.terminal.backend_mut(), cleanup_state)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CleanupOperations, CleanupState, cleanup_best_effort, map_key_event, mouse_action,
    };
    use crate::model::{Bookmark, Database};
    use crate::ui::app::{Action, App, ClickButton};
    use crate::ui::view::layout_for;
    use chrono::{TimeZone, Utc};
    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::layout::Rect;
    use std::io;
    use std::path::PathBuf;
    use std::time::Duration;
    use uuid::Uuid;

    #[test]
    fn cleanup_continues_after_failures_and_preserves_the_first_error() {
        let mut operations = FakeCleanup::failing(&["mouse", "cursor"]);

        let error = cleanup_best_effort(
            &mut operations,
            CleanupState {
                raw_mode: true,
                alternate_screen: true,
                mouse_capture: true,
                show_cursor: true,
            },
        )
        .unwrap_err();

        assert_eq!(operations.calls, ["mouse", "alternate", "cursor", "raw"]);
        assert_eq!(error.to_string(), "mouse failed");
    }

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

    #[derive(Default)]
    struct FakeCleanup {
        calls: Vec<&'static str>,
        failures: Vec<&'static str>,
    }

    impl FakeCleanup {
        fn failing(failures: &[&'static str]) -> Self {
            Self {
                calls: Vec::new(),
                failures: failures.to_vec(),
            }
        }

        fn perform(&mut self, operation: &'static str) -> io::Result<()> {
            self.calls.push(operation);
            if self.failures.contains(&operation) {
                Err(io::Error::other(format!("{operation} failed")))
            } else {
                Ok(())
            }
        }
    }

    impl CleanupOperations for FakeCleanup {
        fn disable_mouse_capture(&mut self) -> io::Result<()> {
            self.perform("mouse")
        }

        fn leave_alternate_screen(&mut self) -> io::Result<()> {
            self.perform("alternate")
        }

        fn show_cursor(&mut self) -> io::Result<()> {
            self.perform("cursor")
        }

        fn disable_raw_mode(&mut self) -> io::Result<()> {
            self.perform("raw")
        }
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
