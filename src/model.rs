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
}
