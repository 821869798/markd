use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bookmark {
    pub id: Uuid,
    pub name: String,
    pub path: PathBuf,
    pub category: String,
    pub created_at: DateTime<Utc>,
    pub last_visited_at: Option<DateTime<Utc>>,
    pub visit_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Database {
    pub version: u32,
    pub categories: Vec<String>,
    pub bookmarks: Vec<Bookmark>,
}

impl Default for Database {
    fn default() -> Self {
        Self {
            version: 1,
            categories: vec!["default".to_owned()],
            bookmarks: Vec::new(),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ModelError {
    #[error("a bookmark already exists for path: {0}")]
    DuplicatePath(PathBuf),
    #[error("a category already exists: {0}")]
    DuplicateCategory(String),
    #[error("category not found: {0}")]
    CategoryNotFound(String),
    #[error("the default category cannot be changed")]
    ProtectedCategory,
    #[error("bookmark not found: {0}")]
    BookmarkNotFound(String),
    #[error("bookmark name is ambiguous: {0}")]
    AmbiguousBookmark(String),
}

impl Database {
    pub fn add_bookmark(
        &mut self,
        path: PathBuf,
        name: Option<String>,
        category: Option<String>,
    ) -> Result<&Bookmark, ModelError> {
        if self.bookmarks.iter().any(|bookmark| bookmark.path == path) {
            return Err(ModelError::DuplicatePath(path));
        }

        let name = name.unwrap_or_else(|| {
            path.file_name()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string())
        });
        let category = category.unwrap_or_else(|| "default".to_owned());
        if !self.categories.contains(&category) {
            self.categories.push(category.clone());
        }

        self.bookmarks.push(Bookmark {
            id: Uuid::new_v4(),
            name,
            path,
            category,
            created_at: Utc::now(),
            last_visited_at: None,
            visit_count: 0,
        });
        Ok(self.bookmarks.last().expect("bookmark was just inserted"))
    }

    pub fn add_category(&mut self, name: &str) -> Result<(), ModelError> {
        if self.categories.iter().any(|category| category == name) {
            return Err(ModelError::DuplicateCategory(name.to_owned()));
        }
        self.categories.push(name.to_owned());
        Ok(())
    }

    pub fn rename_category(&mut self, old: &str, new: &str) -> Result<(), ModelError> {
        if old == "default" {
            return Err(ModelError::ProtectedCategory);
        }
        let Some(index) = self.categories.iter().position(|category| category == old) else {
            return Err(ModelError::CategoryNotFound(old.to_owned()));
        };
        if self.categories.iter().any(|category| category == new) {
            return Err(ModelError::DuplicateCategory(new.to_owned()));
        }
        self.categories[index] = new.to_owned();
        for bookmark in &mut self.bookmarks {
            if bookmark.category == old {
                bookmark.category = new.to_owned();
            }
        }
        Ok(())
    }

    pub fn remove_category(&mut self, name: &str) -> Result<(), ModelError> {
        if name == "default" {
            return Err(ModelError::ProtectedCategory);
        }
        let Some(index) = self.categories.iter().position(|category| category == name) else {
            return Err(ModelError::CategoryNotFound(name.to_owned()));
        };
        self.categories.remove(index);
        for bookmark in &mut self.bookmarks {
            if bookmark.category == name {
                bookmark.category = "default".to_owned();
            }
        }
        Ok(())
    }

    pub fn resolve_bookmark(&self, selector: &str) -> Result<&Bookmark, ModelError> {
        if let Ok(id) = Uuid::parse_str(selector) {
            return self
                .bookmarks
                .iter()
                .find(|bookmark| bookmark.id == id)
                .ok_or_else(|| ModelError::BookmarkNotFound(selector.to_owned()));
        }
        let mut matches = self
            .bookmarks
            .iter()
            .filter(|bookmark| bookmark.name == selector);
        let Some(bookmark) = matches.next() else {
            return Err(ModelError::BookmarkNotFound(selector.to_owned()));
        };
        if matches.next().is_some() {
            return Err(ModelError::AmbiguousBookmark(selector.to_owned()));
        }
        Ok(bookmark)
    }

    pub fn rename_bookmark(&mut self, selector: &str, name: &str) -> Result<(), ModelError> {
        let index = self.bookmark_index(selector)?;
        self.bookmarks[index].name = name.to_owned();
        Ok(())
    }

    pub fn remove_bookmark(&mut self, selector: &str) -> Result<Bookmark, ModelError> {
        let index = self.bookmark_index(selector)?;
        Ok(self.bookmarks.remove(index))
    }

    pub fn record_visit(&mut self, id: Uuid, now: DateTime<Utc>) -> Result<(), ModelError> {
        let bookmark = self
            .bookmarks
            .iter_mut()
            .find(|bookmark| bookmark.id == id)
            .ok_or_else(|| ModelError::BookmarkNotFound(id.to_string()))?;
        bookmark.visit_count = bookmark.visit_count.saturating_add(1);
        bookmark.last_visited_at = Some(now);
        Ok(())
    }

    fn bookmark_index(&self, selector: &str) -> Result<usize, ModelError> {
        let bookmark = self.resolve_bookmark(selector)?;
        Ok(self
            .bookmarks
            .iter()
            .position(|candidate| candidate.id == bookmark.id)
            .expect("resolved bookmark must be present"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_database_always_contains_default_category() {
        let db = Database::default();
        assert_eq!(db.categories, vec!["default"]);
        assert!(db.bookmarks.is_empty());
    }

    #[test]
    fn adding_same_canonical_path_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().canonicalize().unwrap();
        let mut db = Database::default();
        db.add_bookmark(path.clone(), None, None).unwrap();
        assert!(matches!(
            db.add_bookmark(path, None, None),
            Err(ModelError::DuplicatePath(_))
        ));
    }

    #[test]
    fn adding_category_rejects_duplicates() {
        let mut db = Database::default();
        db.add_category("work").unwrap();
        assert!(matches!(
            db.add_category("work"),
            Err(ModelError::DuplicateCategory(name)) if name == "work"
        ));
    }

    #[test]
    fn renaming_category_updates_bookmarks() {
        let mut db = database_with_bookmark_in("work");
        db.rename_category("work", "personal").unwrap();
        assert_eq!(db.categories, vec!["default", "personal"]);
        assert_eq!(db.bookmarks[0].category, "personal");
    }

    #[test]
    fn removing_category_moves_bookmarks_to_default() {
        let mut db = database_with_bookmark_in("work");
        db.remove_category("work").unwrap();
        assert_eq!(db.bookmarks[0].category, "default");
        assert_eq!(db.categories, vec!["default"]);
    }

    #[test]
    fn default_category_cannot_be_renamed_or_removed() {
        let mut db = Database::default();
        assert!(matches!(
            db.rename_category("default", "other"),
            Err(ModelError::ProtectedCategory)
        ));
        assert!(matches!(
            db.remove_category("default"),
            Err(ModelError::ProtectedCategory)
        ));
    }

    #[test]
    fn bookmarks_resolve_by_uuid_or_unique_name_and_can_be_renamed_and_removed() {
        let mut db = Database::default();
        let bookmark = db
            .add_bookmark(PathBuf::from("/tmp/repo"), Some("repo".into()), None)
            .unwrap()
            .clone();
        assert_eq!(
            db.resolve_bookmark(&bookmark.id.to_string()).unwrap(),
            &bookmark
        );
        assert_eq!(db.resolve_bookmark("repo").unwrap(), &bookmark);
        db.rename_bookmark(&bookmark.id.to_string(), "project")
            .unwrap();
        assert_eq!(db.bookmarks[0].name, "project");
        let removed = db.remove_bookmark("project").unwrap();
        assert_eq!(removed.id, bookmark.id);
        assert_eq!(removed.name, "project");
        assert!(db.bookmarks.is_empty());
    }

    #[test]
    fn duplicate_names_are_not_resolved() {
        let mut db = Database::default();
        db.add_bookmark(PathBuf::from("/tmp/one"), Some("repo".into()), None)
            .unwrap();
        db.add_bookmark(PathBuf::from("/tmp/two"), Some("repo".into()), None)
            .unwrap();
        assert!(matches!(
            db.resolve_bookmark("repo"),
            Err(ModelError::AmbiguousBookmark(name)) if name == "repo"
        ));
    }

    #[test]
    fn recording_visit_saturates_count_and_updates_timestamp() {
        let mut db = Database::default();
        let id = db
            .add_bookmark(PathBuf::from("/tmp/repo"), None, None)
            .unwrap()
            .id;
        db.bookmarks[0].visit_count = u64::MAX - 1;
        let now = Utc::now();
        db.record_visit(id, now).unwrap();
        db.record_visit(id, now + chrono::Duration::seconds(1))
            .unwrap();
        assert_eq!(db.bookmarks[0].visit_count, u64::MAX);
        assert_eq!(
            db.bookmarks[0].last_visited_at,
            Some(now + chrono::Duration::seconds(1))
        );
    }

    fn database_with_bookmark_in(category: &str) -> Database {
        let mut db = Database::default();
        db.add_category(category).unwrap();
        db.add_bookmark(
            PathBuf::from("/tmp/bookmark"),
            Some("bookmark".into()),
            Some(category.into()),
        )
        .unwrap();
        db
    }
}
