use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::model::Database;
use crate::paths;
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
    DeleteSelected,
    ConfirmDelete(bool),
    BeginRename,
    CommitRename(String),
    CreateCategory(String),
    RenameCategory(String),
    DeleteCategory,
    MoveBookmarkToCategory(String),
    BeginCreateCategory,
    BeginRenameCategory,
    BeginMoveBookmark,
    BeginAddBookmark,
    AddBookmark(String),
    CopySelectedPath,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditMode {
    BookmarkRename,
    CategoryCreate,
    CategoryRename,
    BookmarkMove,
    AddBookmark,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Mutation {
    DeleteBookmark {
        id: Uuid,
    },
    RenameBookmark {
        id: Uuid,
        old_name: String,
        new_name: String,
    },
    CreateCategory {
        name: String,
    },
    RenameCategory {
        old: String,
        new: String,
    },
    DeleteCategory {
        name: String,
    },
    MoveBookmark {
        id: Uuid,
        old_category: String,
        new_category: String,
    },
    AddBookmark {
        input: String,
        category: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingDelete {
    Bookmark(Uuid),
    Category(String),
}

#[derive(Debug, Clone)]
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
    edit_mode: Option<EditMode>,
    edit_text: String,
    pending_delete: Option<PendingDelete>,
    last_mutation: Option<Mutation>,
    pending_copy: Option<String>,
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
            edit_mode: None,
            edit_text: String::new(),
            pending_delete: None,
            last_mutation: None,
            pending_copy: None,
        };
        app.refresh(now, None);
        app
    }

    pub fn handle(&mut self, action: Action, now: DateTime<Utc>) -> Outcome {
        self.last_mutation = None;
        if self.pending_delete.is_some()
            && !matches!(action, Action::ConfirmDelete(_) | Action::Cancel)
        {
            return Outcome::Continue;
        }
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
                if self.pending_delete.is_some() || self.edit_mode.is_some() {
                    self.pending_delete = None;
                    self.edit_mode = None;
                    self.edit_text.clear();
                    self.status_message = None;
                    Outcome::Continue
                } else if self.search_mode {
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
                } else if self.edit_mode.is_some() && !character.is_control() {
                    self.edit_text.push(character);
                }
                Outcome::Continue
            }
            Action::Backspace => {
                if self.search_mode && self.search_text.pop().is_some() {
                    self.refresh(now, None);
                } else if self.edit_mode.is_some() {
                    self.edit_text.pop();
                }
                Outcome::Continue
            }
            Action::DeleteSelected => {
                if let Some(id) = self.selected_id() {
                    self.pending_delete = Some(PendingDelete::Bookmark(id));
                    self.status_message = Some("再次按 d 确认删除，按 Esc 取消".to_owned());
                } else {
                    self.status_message = Some("没有可删除的书签".to_owned());
                }
                Outcome::Continue
            }
            Action::CopySelectedPath => {
                if let Some(row) = self.selected_row() {
                    self.pending_copy = Some(row.path.display().to_string());
                    self.status_message = Some("路径已复制到剪贴板".to_owned());
                } else {
                    self.status_message = Some("没有可复制的书签".to_owned());
                }
                Outcome::Continue
            }
            Action::ConfirmDelete(confirm) => {
                if let Some(pending) = self.pending_delete.take() {
                    if confirm {
                        match pending {
                            PendingDelete::Bookmark(id) => self.delete_selected(id, now),
                            PendingDelete::Category(name) => self.delete_category(&name, now),
                        }
                    } else {
                        self.status_message = None;
                    }
                }
                Outcome::Continue
            }
            Action::BeginRename => {
                self.begin_edit(EditMode::BookmarkRename);
                Outcome::Continue
            }
            Action::CommitRename(name) => {
                self.commit_edit(EditMode::BookmarkRename, name, now);
                Outcome::Continue
            }
            Action::CreateCategory(name) => {
                self.create_category(&name, now);
                Outcome::Continue
            }
            Action::RenameCategory(name) => {
                self.rename_category(&name, now);
                Outcome::Continue
            }
            Action::DeleteCategory => {
                if let Some(name) = self.categories.get(self.selected_category) {
                    if self.selected_category > 0 {
                        self.pending_delete = Some(PendingDelete::Category(name.clone()));
                        self.status_message = Some("再次按 D 确认删除分类，按 Esc 取消".to_owned());
                    } else {
                        self.status_message = Some("当前选择不支持此操作".to_owned());
                    }
                }
                Outcome::Continue
            }
            Action::MoveBookmarkToCategory(category) => {
                self.move_bookmark(&category, now);
                Outcome::Continue
            }
            Action::BeginCreateCategory => {
                self.begin_edit(EditMode::CategoryCreate);
                Outcome::Continue
            }
            Action::BeginRenameCategory => {
                self.begin_edit(EditMode::CategoryRename);
                Outcome::Continue
            }
            Action::BeginMoveBookmark => {
                self.begin_edit(EditMode::BookmarkMove);
                Outcome::Continue
            }
            Action::BeginAddBookmark => {
                self.begin_edit(EditMode::AddBookmark);
                Outcome::Continue
            }
            Action::AddBookmark(input) => {
                let category = self.current_category_name();
                self.last_mutation = Some(Mutation::AddBookmark { input, category });
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

    /// The real category a new bookmark should land in. "全部" is a virtual
    /// filter, so adds from it fall back to the default category.
    pub(crate) fn current_category_name(&self) -> String {
        if self.selected_category == 0 {
            "default".to_owned()
        } else {
            self.categories
                .get(self.selected_category)
                .cloned()
                .unwrap_or_else(|| "default".to_owned())
        }
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

    pub fn is_editing(&self) -> bool {
        self.edit_mode.is_some()
    }

    pub fn edit_text(&self) -> &str {
        &self.edit_text
    }

    pub fn edit_prompt(&self) -> Option<&str> {
        self.edit_mode.map(|mode| match mode {
            EditMode::BookmarkRename => "重命名书签",
            EditMode::CategoryCreate => "新建分类",
            EditMode::CategoryRename => "重命名分类",
            EditMode::BookmarkMove => "移动到分类",
            EditMode::AddBookmark => "添加书签到当前分组",
        })
    }

    pub(crate) fn commit_editing_action(&self) -> Option<Action> {
        self.edit_mode.map(|mode| match mode {
            EditMode::BookmarkRename => Action::CommitRename(self.edit_text.clone()),
            EditMode::CategoryCreate => Action::CreateCategory(self.edit_text.clone()),
            EditMode::CategoryRename => Action::RenameCategory(self.edit_text.clone()),
            EditMode::BookmarkMove => Action::MoveBookmarkToCategory(self.edit_text.clone()),
            EditMode::AddBookmark => Action::AddBookmark(self.edit_text.clone()),
        })
    }

    pub fn is_confirming_delete(&self) -> bool {
        self.pending_delete.is_some()
    }

    #[cfg(test)]
    pub(crate) fn database_snapshot(&self) -> Database {
        self.database.clone()
    }

    pub(crate) fn pending_delete_key(&self, character: char) -> Option<Action> {
        match (&self.pending_delete, character) {
            (Some(PendingDelete::Bookmark(_)), 'd') | (Some(PendingDelete::Category(_)), 'D') => {
                Some(Action::ConfirmDelete(true))
            }
            _ => None,
        }
    }
    pub(crate) fn take_mutation(&mut self) -> Option<Mutation> {
        self.last_mutation.take()
    }

    pub(crate) fn snapshot(&self) -> Self {
        self.clone()
    }

    pub(crate) fn restore_snapshot(&mut self, snapshot: Self, error: impl Into<String>) {
        *self = snapshot;
        self.status_message = Some(error.into());
    }

    pub(crate) fn replace_database(&mut self, database: Database, now: DateTime<Utc>) {
        let selected_category = self.categories.get(self.selected_category).cloned();
        let selected_bookmark = self.selected_id();
        self.database = database;
        self.rebuild_categories();
        self.selected_category = selected_category
            .and_then(|name| {
                self.categories
                    .iter()
                    .position(|category| category == &name)
            })
            .unwrap_or(0);
        self.refresh(now, selected_bookmark);
    }

    pub(crate) fn replace_database_with_error(
        &mut self,
        database: Database,
        now: DateTime<Utc>,
        error: impl Into<String>,
    ) {
        self.replace_database(database, now);
        self.status_message = Some(error.into());
    }

    fn begin_edit(&mut self, mode: EditMode) {
        let valid = match mode {
            EditMode::BookmarkRename | EditMode::BookmarkMove => self.selected_id().is_some(),
            EditMode::CategoryCreate | EditMode::AddBookmark => true,
            EditMode::CategoryRename => self.selected_category > 0,
        };
        if !valid {
            let hint = match mode {
                EditMode::BookmarkRename | EditMode::BookmarkMove => {
                    "先选中一个书签再操作（右侧列表）"
                }
                EditMode::CategoryRename => "“全部”是虚拟分类，请先选中左侧一个真实分类",
                EditMode::CategoryCreate | EditMode::AddBookmark => "无法进入编辑",
            };
            self.status_message = Some(hint.to_owned());
            return;
        }
        self.pending_delete = None;
        self.edit_mode = Some(mode);
        self.edit_text.clear();
        self.status_message = None;
    }

    fn commit_edit(&mut self, expected: EditMode, text: String, now: DateTime<Utc>) {
        if self.edit_mode != Some(expected) {
            return;
        }
        self.edit_mode = None;
        self.edit_text.clear();
        if text.trim().is_empty() && !matches!(expected, EditMode::AddBookmark) {
            self.status_message = Some("名称不能为空".to_owned());
            return;
        }
        match expected {
            EditMode::BookmarkRename => self.rename_selected(&text, now),
            EditMode::CategoryCreate => self.create_category(&text, now),
            EditMode::CategoryRename => self.rename_category(&text, now),
            EditMode::BookmarkMove => self.move_bookmark(&text, now),
            // AddBookmark commits through commit_editing_action -> Action::AddBookmark,
            // which sets the mutation in `handle`.
            EditMode::AddBookmark => {}
        }
    }

    fn delete_selected(&mut self, id: Uuid, now: DateTime<Utc>) {
        if let Some(index) = self
            .database
            .bookmarks
            .iter()
            .position(|bookmark| bookmark.id == id)
        {
            self.database.bookmarks.remove(index);
            self.last_mutation = Some(Mutation::DeleteBookmark { id });
            self.refresh(now, None);
        } else {
            self.status_message = Some("书签不存在".to_owned());
        }
    }

    fn rename_selected(&mut self, name: &str, now: DateTime<Utc>) {
        let Some(id) = self.selected_id() else {
            self.status_message = Some("没有可重命名的书签".to_owned());
            return;
        };
        if let Some(bookmark) = self
            .database
            .bookmarks
            .iter_mut()
            .find(|bookmark| bookmark.id == id)
        {
            let old_name = std::mem::take(&mut bookmark.name);
            bookmark.name = name.to_owned();
            self.last_mutation = Some(Mutation::RenameBookmark {
                id,
                old_name,
                new_name: name.to_owned(),
            });
            self.refresh(now, Some(id));
        } else {
            self.status_message = Some("书签不存在".to_owned());
        }
    }

    fn create_category(&mut self, name: &str, now: DateTime<Utc>) {
        match self.database.add_category(name) {
            Ok(()) => {
                self.last_mutation = Some(Mutation::CreateCategory {
                    name: name.to_owned(),
                });
                self.rebuild_categories();
                self.refresh(now, None);
            }
            Err(error) => self.status_message = Some(error.to_string()),
        }
    }

    fn rename_category(&mut self, name: &str, now: DateTime<Utc>) {
        let Some(old) = self.categories.get(self.selected_category).cloned() else {
            self.status_message = Some("分类不存在".to_owned());
            return;
        };
        match self.database.rename_category(&old, name) {
            Ok(()) => {
                self.last_mutation = Some(Mutation::RenameCategory {
                    old: old.clone(),
                    new: name.to_owned(),
                });
                self.rebuild_categories();
                self.selected_category = self
                    .categories
                    .iter()
                    .position(|category| category == name)
                    .unwrap_or(0);
                self.refresh(now, None);
            }
            Err(error) => self.status_message = Some(error.to_string()),
        }
    }

    fn delete_category(&mut self, name: &str, now: DateTime<Utc>) {
        match self.database.remove_category(name) {
            Ok(()) => {
                self.last_mutation = Some(Mutation::DeleteCategory {
                    name: name.to_owned(),
                });
                self.selected_category = self.selected_category.saturating_sub(1);
                self.rebuild_categories();
                self.refresh(now, None);
            }
            Err(error) => self.status_message = Some(error.to_string()),
        }
    }

    fn move_bookmark(&mut self, category: &str, now: DateTime<Utc>) {
        let Some(id) = self.selected_id() else {
            self.status_message = Some("没有可移动的书签".to_owned());
            return;
        };
        if !self
            .database
            .categories
            .iter()
            .any(|candidate| candidate == category)
        {
            self.status_message = Some(format!("category not found: {category}"));
            return;
        }
        if let Some(bookmark) = self
            .database
            .bookmarks
            .iter_mut()
            .find(|bookmark| bookmark.id == id)
        {
            let old_category = std::mem::replace(&mut bookmark.category, category.to_owned());
            self.last_mutation = Some(Mutation::MoveBookmark {
                id,
                old_category,
                new_category: category.to_owned(),
            });
            self.refresh(now, Some(id));
        } else {
            self.status_message = Some("书签不存在".to_owned());
        }
    }

    fn selected_id(&self) -> Option<Uuid> {
        self.selected_bookmark
            .and_then(|index| self.visible_bookmarks.get(index))
            .map(|bookmark| bookmark.id)
    }

    fn selected_row(&self) -> Option<&BookmarkRow> {
        self.selected_bookmark
            .and_then(|index| self.visible_bookmarks.get(index))
    }

    pub(crate) fn take_pending_copy(&mut self) -> Option<String> {
        self.pending_copy.take()
    }

    pub(crate) fn set_copy_failed(&mut self) {
        self.status_message = Some("复制失败：未找到可用的剪贴板工具".to_owned());
    }

    fn rebuild_categories(&mut self) {
        self.categories.clear();
        self.categories.push("全部".to_owned());
        self.categories
            .extend(self.database.categories.iter().cloned());
        self.selected_category = self
            .selected_category
            .min(self.categories.len().saturating_sub(1));
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

impl Mutation {
    pub(crate) fn apply(&self, database: &mut Database) -> Result<(), String> {
        match self {
            Self::DeleteBookmark { id } => database
                .bookmarks
                .iter()
                .position(|bookmark| bookmark.id == *id)
                .map(|index| {
                    database.bookmarks.remove(index);
                })
                .ok_or_else(|| format!("bookmark no longer exists: {id}")),
            Self::RenameBookmark {
                id,
                old_name,
                new_name,
            } => {
                let bookmark = database
                    .bookmarks
                    .iter_mut()
                    .find(|bookmark| bookmark.id == *id)
                    .ok_or_else(|| format!("bookmark no longer exists: {id}"))?;
                if bookmark.name != *old_name {
                    return Err(format!("bookmark was changed externally: {id}"));
                }
                bookmark.name = new_name.clone();
                Ok(())
            }
            Self::CreateCategory { name } => database
                .add_category(name)
                .map_err(|error| error.to_string()),
            Self::RenameCategory { old, new } => database
                .rename_category(old, new)
                .map_err(|error| error.to_string()),
            Self::DeleteCategory { name } => database
                .remove_category(name)
                .map_err(|error| error.to_string()),
            Self::MoveBookmark {
                id,
                old_category,
                new_category,
            } => {
                if !database
                    .categories
                    .iter()
                    .any(|category| category == new_category)
                {
                    return Err(format!("category not found: {new_category}"));
                }
                let bookmark = database
                    .bookmarks
                    .iter_mut()
                    .find(|bookmark| bookmark.id == *id)
                    .ok_or_else(|| format!("bookmark no longer exists: {id}"))?;
                if bookmark.category != *old_category {
                    return Err(format!("bookmark was changed externally: {id}"));
                }
                bookmark.category = new_category.clone();
                Ok(())
            }
            Self::AddBookmark { input, category } => {
                // Resolve inside the mutation replay so concurrent runs revalidate
                // against the latest database state and the live filesystem.
                let trimmed = input.trim();
                let raw = if trimmed.is_empty() { "." } else { trimmed };
                let base = std::env::current_dir().unwrap_or_default();
                let path = paths::normalize_directory_from(std::path::Path::new(raw), &base)
                    .map_err(|error| error.to_string())?;
                database
                    .add_bookmark(path, None, Some(category.clone()))
                    .map_err(|error| error.to_string())?;
                Ok(())
            }
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
    use super::{Action, App, ClickButton, Mutation, Outcome, Pane};
    use crate::model::{Bookmark, Database};
    use chrono::{DateTime, TimeZone, Utc};
    use std::path::PathBuf;
    use std::time::Duration;
    use uuid::Uuid;

    #[test]
    fn add_bookmark_mutation_replays_onto_the_latest_database() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("boxed");
        std::fs::create_dir(&target).unwrap();
        let mut database = Database::default();
        let mutation = Mutation::AddBookmark {
            input: target.to_string_lossy().into_owned(),
            category: "default".into(),
        };
        mutation.apply(&mut database).unwrap();
        assert_eq!(database.bookmarks.len(), 1);
        assert_eq!(
            database.bookmarks[0].path,
            crate::paths::strip_verbatim_prefix(target.canonicalize().unwrap())
        );
        assert_eq!(database.bookmarks[0].category, "default");
    }

    #[test]
    fn add_bookmark_mutation_rejects_a_missing_directory() {
        let mut database = Database::default();
        let mutation = Mutation::AddBookmark {
            input: "definitely-missing-mkd-dir".into(),
            category: "default".into(),
        };
        assert!(mutation.apply(&mut database).is_err());
        assert!(database.bookmarks.is_empty());
    }

    #[test]
    fn add_bookmark_targets_the_selected_real_category() {
        let mut app = App::from_database(Database::default());
        // "全部" is virtual: adding from it lands in default.
        app.handle(Action::AddBookmark(String::new()), Utc::now());
        let Some(Mutation::AddBookmark { input, category }) = app.take_mutation() else {
            panic!("expected AddBookmark mutation");
        };
        assert_eq!(input, "");
        assert_eq!(category, "default");
    }

    #[test]
    fn add_bookmark_from_a_real_category_uses_that_category() {
        let mut database = Database::default();
        database.add_category("work").unwrap();
        // categories: [全部, default, work] — index 2 selects work.
        let mut app = App::from_database(database);
        app.handle(Action::ClickCategory { row: 2 }, Utc::now());
        app.handle(Action::AddBookmark(String::new()), Utc::now());
        let Some(Mutation::AddBookmark { category, .. }) = app.take_mutation() else {
            panic!("expected AddBookmark mutation");
        };
        assert_eq!(category, "work");
    }

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

    #[test]
    fn mutation_replay_preserves_external_bookmarks_and_rejects_stale_identity() {
        let temp = tempfile::tempdir().unwrap();
        let first = bookmark("first", temp.path().join("first"), "default");
        let id = first.id;
        let mut database = database_with(vec![first]);
        let mutation = Mutation::RenameBookmark {
            id,
            old_name: "first".into(),
            new_name: "renamed".into(),
        };
        database
            .add_bookmark(temp.path().join("second"), Some("second".into()), None)
            .unwrap();
        mutation.apply(&mut database).unwrap();
        assert_eq!(database.bookmarks.len(), 2);
        assert_eq!(database.resolve_bookmark("renamed").unwrap().id, id);

        database.bookmarks[0].name = "changed-externally".into();
        assert!(mutation.apply(&mut database).is_err());
    }
    #[test]
    fn management_actions_update_bookmarks_and_categories() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = App::from_database_at(
            database_with(vec![bookmark("old", temp.path().to_path_buf(), "default")]),
            test_now(),
        );
        let id = app.visible_bookmarks()[0].id;

        app.handle(Action::BeginRename, test_now());
        app.handle(Action::CommitRename("new".into()), test_now());
        assert_eq!(app.visible_bookmarks()[0].name, "new");

        app.handle(Action::CreateCategory("personal".into()), test_now());
        assert!(app.categories().contains(&"personal".to_owned()));
        app.handle(
            Action::MoveBookmarkToCategory("personal".into()),
            test_now(),
        );
        assert_eq!(app.visible_bookmarks()[0].category, "personal");

        app.handle(Action::ClickCategory { row: 2 }, test_now());
        app.handle(Action::RenameCategory("private".into()), test_now());
        assert!(app.categories().contains(&"private".to_owned()));
        app.handle(Action::DeleteCategory, test_now());
        assert!(app.is_confirming_delete());
        app.handle(Action::ConfirmDelete(true), test_now());
        assert!(!app.categories().contains(&"private".to_owned()));
        assert_eq!(app.database_snapshot().bookmarks[0].id, id);
    }

    #[test]
    fn delete_requires_confirmation_and_cancel_preserves_data() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = App::from_database_at(
            database_with(vec![bookmark(
                "project",
                temp.path().to_path_buf(),
                "default",
            )]),
            test_now(),
        );
        app.handle(Action::DeleteSelected, test_now());
        assert!(app.is_confirming_delete());
        app.handle(Action::ConfirmDelete(false), test_now());
        assert_eq!(app.database_snapshot().bookmarks.len(), 1);
        app.handle(Action::DeleteSelected, test_now());
        app.handle(Action::ConfirmDelete(true), test_now());
        assert!(app.database_snapshot().bookmarks.is_empty());
    }

    #[test]
    fn deleting_non_first_bookmark_keeps_first_bookmark() {
        let temp = tempfile::tempdir().unwrap();
        let first = bookmark("first", temp.path().join("first"), "default");
        let second = bookmark("second", temp.path().join("second"), "default");
        let second_id = second.id;
        let mut app = App::from_database_at(database_with(vec![first, second]), test_now());
        app.handle(Action::Down, test_now());
        assert_eq!(app.selected_id(), Some(second_id));
        app.handle(Action::DeleteSelected, test_now());
        app.handle(Action::ConfirmDelete(true), test_now());
        let database = app.database_snapshot();
        assert_eq!(database.bookmarks.len(), 1);
        assert_eq!(database.bookmarks[0].name, "first");
    }

    #[test]
    fn persistence_failure_restores_edit_mode_and_input() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = App::from_database_at(
            database_with(vec![bookmark(
                "before",
                temp.path().to_path_buf(),
                "default",
            )]),
            test_now(),
        );
        app.handle(Action::BeginRename, test_now());
        app.handle(Action::Input('d'), test_now());
        app.handle(Action::Input('r'), test_now());
        let snapshot = app.snapshot();
        app.handle(Action::CommitRename("saved".into()), test_now());
        app.restore_snapshot(snapshot, "save failed");
        assert!(app.is_editing());
        assert_eq!(app.edit_text(), "dr");
        assert_eq!(app.database_snapshot().bookmarks[0].name, "before");
        assert_eq!(app.status_message(), Some("save failed"));
    }
    #[test]
    fn edit_cancel_preserves_data() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = App::from_database_at(
            database_with(vec![bookmark(
                "project",
                temp.path().to_path_buf(),
                "default",
            )]),
            test_now(),
        );
        app.handle(Action::BeginRename, test_now());
        app.handle(Action::Input('x'), test_now());
        app.handle(Action::Cancel, test_now());
        assert_eq!(app.database_snapshot().bookmarks[0].name, "project");
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
