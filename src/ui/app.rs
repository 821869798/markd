use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::model::Database;
use crate::query::{Query, query_bookmarks};

const DOUBLE_CLICK_THRESHOLD: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Categories,
    Bookmarks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    Confirm,
    Cancel,
    TogglePane,
    StartSearch,
    Input(char),
    Backspace,
    ClickBookmark {
        row: usize,
        button: ClickButton,
        elapsed: Duration,
    },
    ClickCategory {
        row: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Continue,
    Cancelled,
    Selected { id: Uuid, path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookmarkRow {
    pub id: Uuid,
    pub name: String,
    pub path: PathBuf,
    pub category: String,
    pub valid_target: bool,
}

#[derive(Debug, Clone, Copy)]
struct LastClick {
    bookmark_id: Uuid,
    button: ClickButton,
    elapsed: Duration,
}

#[derive(Debug)]
pub struct App {
    database: Database,
    categories: Vec<String>,
    visible_bookmarks: Vec<BookmarkRow>,
    active_pane: Pane,
    selected_category: usize,
    selected_bookmark: Option<usize>,
    search_text: String,
    search_mode: bool,
    status_message: Option<String>,
    last_click: Option<LastClick>,
    category_offset: usize,
    bookmark_offset: usize,
    category_viewport_rows: usize,
    bookmark_viewport_rows: usize,
}

impl App {
    pub fn from_database(database: Database) -> Self {
        Self::from_database_at(database, Utc::now())
    }

    pub fn from_database_at(database: Database, now: DateTime<Utc>) -> Self {
        let mut categories = Vec::with_capacity(database.categories.len() + 1);
        categories.push("全部".to_owned());
        categories.extend(database.categories.iter().cloned());

        let mut app = Self {
            database,
            categories,
            visible_bookmarks: Vec::new(),
            active_pane: Pane::Bookmarks,
            selected_category: 0,
            selected_bookmark: None,
            search_text: String::new(),
            search_mode: false,
            status_message: None,
            last_click: None,
            category_offset: 0,
            bookmark_offset: 0,
            category_viewport_rows: 0,
            bookmark_viewport_rows: 0,
        };
        app.refresh(now, None);
        app
    }

    pub fn handle(&mut self, action: Action, now: DateTime<Utc>) -> Outcome {
        match action {
            Action::Up => {
                self.move_up(now);
                Outcome::Continue
            }
            Action::Down => {
                self.move_down(now);
                Outcome::Continue
            }
            Action::Confirm => self.confirm_selected(),
            Action::Cancel => {
                self.last_click = None;
                if self.search_mode {
                    self.search_mode = false;
                    Outcome::Continue
                } else {
                    Outcome::Cancelled
                }
            }
            Action::TogglePane => {
                self.active_pane = match self.active_pane {
                    Pane::Categories => Pane::Bookmarks,
                    Pane::Bookmarks => Pane::Categories,
                };
                self.last_click = None;
                Outcome::Continue
            }
            Action::StartSearch => {
                self.search_mode = true;
                self.active_pane = Pane::Bookmarks;
                self.status_message = None;
                self.last_click = None;
                Outcome::Continue
            }
            Action::Input(character) => {
                if self.search_mode && !character.is_control() {
                    self.search_text.push(character);
                    self.refresh(now, None);
                }
                Outcome::Continue
            }
            Action::Backspace => {
                if self.search_mode && self.search_text.pop().is_some() {
                    self.refresh(now, None);
                }
                Outcome::Continue
            }
            Action::ClickBookmark {
                row,
                button,
                elapsed,
            } => self.click_bookmark(row, button, elapsed),
            Action::ClickCategory { row } => {
                self.last_click = None;
                if row < self.categories.len() {
                    self.active_pane = Pane::Categories;
                    self.selected_category = row;
                    self.refresh(now, None);
                }
                Outcome::Continue
            }
        }
    }

    pub fn visible_bookmarks(&self) -> &[BookmarkRow] {
        &self.visible_bookmarks
    }

    pub fn categories(&self) -> &[String] {
        &self.categories
    }

    pub fn active_pane(&self) -> Pane {
        self.active_pane
    }

    pub fn selected_category(&self) -> usize {
        self.selected_category
    }

    pub fn selected_bookmark(&self) -> Option<usize> {
        self.selected_bookmark
    }

    pub fn search_text(&self) -> &str {
        &self.search_text
    }

    pub fn is_searching(&self) -> bool {
        self.search_mode
    }

    pub fn status_message(&self) -> Option<&str> {
        self.status_message.as_deref()
    }

    pub fn category_offset(&self) -> usize {
        self.category_offset
    }

    pub fn bookmark_offset(&self) -> usize {
        self.bookmark_offset
    }

    pub fn set_viewport_rows(&mut self, category_rows: usize, bookmark_rows: usize) {
        self.category_viewport_rows = category_rows;
        self.bookmark_viewport_rows = bookmark_rows;
        self.category_offset = keep_visible(
            self.category_offset,
            self.selected_category,
            self.categories.len(),
            category_rows,
        );
        if let Some(selected) = self.selected_bookmark {
            self.bookmark_offset = keep_visible(
                self.bookmark_offset,
                selected,
                self.visible_bookmarks.len(),
                bookmark_rows,
            );
        } else {
            self.bookmark_offset = 0;
        }
    }

    fn move_up(&mut self, now: DateTime<Utc>) {
        self.last_click = None;
        match self.active_pane {
            Pane::Categories => {
                if self.selected_category > 0 {
                    self.selected_category -= 1;
                    self.refresh(now, None);
                }
            }
            Pane::Bookmarks => {
                if let Some(selected) = self.selected_bookmark.as_mut() {
                    *selected = selected.saturating_sub(1);
                    self.update_bookmark_offset();
                }
            }
        }
    }

    fn move_down(&mut self, now: DateTime<Utc>) {
        self.last_click = None;
        match self.active_pane {
            Pane::Categories => {
                if self.selected_category + 1 < self.categories.len() {
                    self.selected_category += 1;
                    self.refresh(now, None);
                }
            }
            Pane::Bookmarks => {
                if let Some(selected) = self.selected_bookmark.as_mut() {
                    *selected = (*selected + 1).min(self.visible_bookmarks.len() - 1);
                    self.update_bookmark_offset();
                }
            }
        }
    }

    fn confirm_selected(&mut self) -> Outcome {
        let Some(row) = self
            .selected_bookmark
            .and_then(|index| self.visible_bookmarks.get(index))
        else {
            self.status_message = Some("没有可选择的书签".to_owned());
            return Outcome::Continue;
        };

        if !row.path.is_dir() {
            self.status_message = Some(format!("目录不存在或不是目录：{}", row.path.display()));
            return Outcome::Continue;
        }

        Outcome::Selected {
            id: row.id,
            path: row.path.clone(),
        }
    }

    fn click_bookmark(&mut self, row: usize, button: ClickButton, elapsed: Duration) -> Outcome {
        self.active_pane = Pane::Bookmarks;
        let Some(clicked) = self.visible_bookmarks.get(row) else {
            self.last_click = None;
            return Outcome::Continue;
        };
        let clicked_id = clicked.id;
        self.selected_bookmark = Some(row);
        self.update_bookmark_offset();

        let is_double_click = self.last_click.is_some_and(|previous| {
            previous.bookmark_id == clicked_id
                && previous.button == button
                && elapsed
                    .checked_sub(previous.elapsed)
                    .is_some_and(|duration| duration <= DOUBLE_CLICK_THRESHOLD)
        });
        self.last_click = Some(LastClick {
            bookmark_id: clicked_id,
            button,
            elapsed,
        });

        if is_double_click {
            self.confirm_selected()
        } else {
            Outcome::Continue
        }
    }

    fn refresh(&mut self, now: DateTime<Utc>, preferred_id: Option<Uuid>) {
        let previous_id = preferred_id.or_else(|| {
            self.selected_bookmark
                .and_then(|index| self.visible_bookmarks.get(index))
                .map(|row| row.id)
        });
        let category = self
            .selected_category
            .checked_sub(1)
            .and_then(|index| self.categories.get(index + 1))
            .map(String::as_str);

        self.visible_bookmarks = query_bookmarks(
            &self.database,
            Query {
                category,
                search: &self.search_text,
            },
            now,
        )
        .into_iter()
        .map(|result| BookmarkRow {
            id: result.bookmark.id,
            name: result.bookmark.name.clone(),
            path: result.bookmark.path.clone(),
            category: result.bookmark.category.clone(),
            valid_target: result.bookmark.path.is_dir(),
        })
        .collect();

        self.selected_bookmark = previous_id
            .and_then(|id| self.visible_bookmarks.iter().position(|row| row.id == id))
            .or_else(|| (!self.visible_bookmarks.is_empty()).then_some(0));
        self.status_message = None;
        self.last_click = None;
        self.set_viewport_rows(self.category_viewport_rows, self.bookmark_viewport_rows);
    }

    fn update_bookmark_offset(&mut self) {
        if let Some(selected) = self.selected_bookmark {
            self.bookmark_offset = keep_visible(
                self.bookmark_offset,
                selected,
                self.visible_bookmarks.len(),
                self.bookmark_viewport_rows,
            );
        }
    }
}

fn keep_visible(offset: usize, selected: usize, len: usize, rows: usize) -> usize {
    if len == 0 || rows == 0 {
        return 0;
    }
    let max_offset = len.saturating_sub(rows);
    if selected < offset {
        selected
    } else if selected >= offset.saturating_add(rows) {
        selected + 1 - rows
    } else {
        offset.min(max_offset)
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, App, ClickButton, Outcome, Pane};
    use crate::model::{Bookmark, Database};
    use chrono::{DateTime, TimeZone, Utc};
    use std::path::PathBuf;
    use std::time::Duration;
    use uuid::Uuid;

    #[test]
    fn enter_selects_current_valid_bookmark_with_stable_identity() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().to_path_buf();
        let database = database_with(vec![bookmark("project", path.clone(), "work")]);
        let expected_id = database.bookmarks[0].id;
        let mut app = App::from_database_at(database, test_now());

        assert_eq!(
            app.handle(Action::Confirm, test_now()),
            Outcome::Selected {
                id: expected_id,
                path
            }
        );
    }

    #[test]
    fn confirm_rejects_missing_directory() {
        let missing = tempfile::tempdir().unwrap().path().join("gone");
        let mut app = App::from_database_at(
            database_with(vec![bookmark("missing", missing, "default")]),
            test_now(),
        );

        assert_eq!(app.handle(Action::Confirm, test_now()), Outcome::Continue);
        assert!(
            app.status_message()
                .is_some_and(|message| message.contains("目录不存在或不是目录"))
        );
    }

    #[test]
    fn confirm_rejects_regular_file_and_row_marks_it_as_invalid_target() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut app = App::from_database_at(
            database_with(vec![bookmark(
                "regular-file",
                file.path().to_path_buf(),
                "default",
            )]),
            test_now(),
        );

        assert!(!app.visible_bookmarks()[0].valid_target);
        assert_eq!(app.handle(Action::Confirm, test_now()), Outcome::Continue);
        assert!(
            app.status_message()
                .is_some_and(|message| message.contains("目录不存在或不是目录"))
        );
    }

    #[test]
    fn bookmark_navigation_stops_at_both_boundaries_and_supports_j_k_actions() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = App::from_database_at(
            database_with(vec![
                bookmark("alpha", temp.path().join("alpha"), "default"),
                bookmark("beta", temp.path().join("beta"), "default"),
            ]),
            test_now(),
        );

        assert_eq!(app.selected_bookmark(), Some(0));
        app.handle(Action::Up, test_now());
        assert_eq!(app.selected_bookmark(), Some(0));
        app.handle(Action::Down, test_now());
        app.handle(Action::Down, test_now());
        assert_eq!(app.selected_bookmark(), Some(1));
        app.handle(Action::Up, test_now());
        assert_eq!(app.selected_bookmark(), Some(0));
    }

    #[test]
    fn tab_switches_panes_and_category_navigation_filters_bookmarks_at_boundaries() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = App::from_database_at(
            database_with(vec![
                bookmark("default", temp.path().join("default"), "default"),
                bookmark("work", temp.path().join("work"), "work"),
            ]),
            test_now(),
        );

        assert_eq!(app.active_pane(), Pane::Bookmarks);
        app.handle(Action::TogglePane, test_now());
        assert_eq!(app.active_pane(), Pane::Categories);
        app.handle(Action::Up, test_now());
        assert_eq!(app.selected_category(), 0);
        app.handle(Action::Down, test_now());
        app.handle(Action::Down, test_now());
        app.handle(Action::Down, test_now());
        assert_eq!(app.selected_category(), 2);
        assert_eq!(app.visible_bookmarks()[0].name, "work");
        app.handle(Action::TogglePane, test_now());
        assert_eq!(app.active_pane(), Pane::Bookmarks);
    }

    #[test]
    fn search_mode_accepts_text_and_backspace_while_escape_exits_without_cancelling() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = App::from_database_at(
            database_with(vec![
                bookmark("alpha", temp.path().join("alpha"), "default"),
                bookmark("beta", temp.path().join("beta"), "default"),
            ]),
            test_now(),
        );

        app.handle(Action::Input('x'), test_now());
        assert_eq!(app.search_text(), "");
        app.handle(Action::StartSearch, test_now());
        app.handle(Action::Input('b'), test_now());
        app.handle(Action::Input('e'), test_now());
        assert_eq!(app.search_text(), "be");
        assert_eq!(app.visible_bookmarks()[0].name, "beta");
        app.handle(Action::Backspace, test_now());
        assert_eq!(app.search_text(), "b");
        app.handle(Action::Input('中'), test_now());
        app.handle(Action::Input('🙂'), test_now());
        assert_eq!(app.search_text(), "b中🙂");
        app.handle(Action::Backspace, test_now());
        assert_eq!(app.search_text(), "b中");
        app.handle(Action::Backspace, test_now());
        assert_eq!(app.search_text(), "b");
        assert_eq!(app.handle(Action::Cancel, test_now()), Outcome::Continue);
        assert!(!app.is_searching());
        assert_eq!(app.search_text(), "b");
        assert_eq!(app.handle(Action::Cancel, test_now()), Outcome::Cancelled);
    }

    #[test]
    fn empty_data_and_search_without_matches_have_no_selection() {
        let mut empty = App::from_database_at(Database::default(), test_now());
        assert_eq!(empty.selected_bookmark(), None);
        assert!(empty.visible_bookmarks().is_empty());
        assert_eq!(empty.handle(Action::Down, test_now()), Outcome::Continue);

        let temp = tempfile::tempdir().unwrap();
        let mut searched = App::from_database_at(
            database_with(vec![bookmark(
                "alpha",
                temp.path().join("alpha"),
                "default",
            )]),
            test_now(),
        );
        searched.handle(Action::StartSearch, test_now());
        for character in "no-such-bookmark-987654321".chars() {
            searched.handle(Action::Input(character), test_now());
        }
        assert!(searched.visible_bookmarks().is_empty());
        assert_eq!(searched.selected_bookmark(), None);
    }

    #[test]
    fn second_click_on_same_bookmark_and_button_within_threshold_selects_row() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().to_path_buf();
        let database = database_with(vec![bookmark("project", path.clone(), "default")]);
        let expected_id = database.bookmarks[0].id;
        let mut app = App::from_database_at(database, test_now());

        assert_eq!(
            app.handle(
                Action::ClickBookmark {
                    row: 0,
                    button: ClickButton::Left,
                    elapsed: Duration::from_millis(100),
                },
                test_now(),
            ),
            Outcome::Continue
        );
        assert_eq!(
            app.handle(
                Action::ClickBookmark {
                    row: 0,
                    button: ClickButton::Left,
                    elapsed: Duration::from_millis(350),
                },
                test_now(),
            ),
            Outcome::Selected {
                id: expected_id,
                path
            }
        );
    }

    #[test]
    fn double_click_requires_same_item_button_and_at_most_five_hundred_milliseconds() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = App::from_database_at(
            database_with(vec![
                bookmark("alpha", temp.path().to_path_buf(), "default"),
                bookmark("beta", temp.path().to_path_buf(), "default"),
            ]),
            test_now(),
        );

        click(&mut app, 0, ClickButton::Left, 100);
        click(&mut app, 0, ClickButton::Right, 200);
        assert_eq!(
            click(&mut app, 0, ClickButton::Left, 300),
            Outcome::Continue
        );
        assert_eq!(
            click(&mut app, 0, ClickButton::Left, 801),
            Outcome::Continue
        );
        assert_eq!(
            click(&mut app, 1, ClickButton::Left, 900),
            Outcome::Continue
        );
        assert_eq!(
            click(&mut app, 0, ClickButton::Left, 1_000),
            Outcome::Continue
        );
        assert!(matches!(
            click(&mut app, 0, ClickButton::Left, 1_500),
            Outcome::Selected { .. }
        ));
    }

    fn click(app: &mut App, row: usize, button: ClickButton, millis: u64) -> Outcome {
        app.handle(
            Action::ClickBookmark {
                row,
                button,
                elapsed: Duration::from_millis(millis),
            },
            test_now(),
        )
    }

    fn database_with(bookmarks: Vec<Bookmark>) -> Database {
        Database {
            version: 1,
            categories: vec!["default".into(), "work".into()],
            bookmarks,
        }
    }

    fn bookmark(name: &str, path: PathBuf, category: &str) -> Bookmark {
        Bookmark {
            id: Uuid::new_v4(),
            name: name.into(),
            path,
            category: category.into(),
            created_at: test_now(),
            last_visited_at: None,
            visit_count: 0,
        }
    }

    fn test_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 31, 12, 0, 0).unwrap()
    }
}
