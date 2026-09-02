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
    /// Manual position assigned by Alt+Arrow reordering. `None` keeps the
    /// automatic (usage-frequency) ranking; `Some(n)` pins the bookmark into
    /// the manually ordered head of the list.
    #[serde(default)]
    pub sort_key: Option<u64>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveDirection {
    Up,
    Down,
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
            sort_key: None,
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

    /// Moves a bookmark up or down within its category's display order.
    ///
    /// The caller supplies the ordered, filtered list of bookmark IDs for the
    /// category (as the TUI displays it). Moving pins both the target and its
    /// neighbor into the manual ordering region by assigning concrete sort
    /// keys that reflect the swap; previously manual items keep their keys so
    /// the rest of the manual region stays stable.
    pub fn move_bookmark(
        &mut self,
        id: Uuid,
        direction: MoveDirection,
        ordered_ids: &[Uuid],
    ) -> Result<(), ModelError> {
        let position = ordered_ids
            .iter()
            .position(|candidate| *candidate == id)
            .ok_or_else(|| ModelError::BookmarkNotFound(id.to_string()))?;
        let neighbor = match direction {
            MoveDirection::Up => {
                if position == 0 {
                    return Ok(()); // already at the top: no-op
                }
                position - 1
            }
            MoveDirection::Down => {
                if position + 1 >= ordered_ids.len() {
                    return Ok(()); // already at the bottom: no-op
                }
                position + 1
            }
        };

        // Assign keys 0..n over the manual head so the swap is exact and the
        // remaining manual items keep their relative order. Auto items
        // (sort_key None) follow after the manual region.
        let mut manual_ids: Vec<Uuid> = ordered_ids
            .iter()
            .filter(|candidate| {
                self.bookmarks
                    .iter()
                    .any(|bookmark| bookmark.id == **candidate && bookmark.sort_key.is_some())
            })
            .cloned()
            .collect();

        if manual_ids.is_empty() {
            // First manual move: seed the manual region with the full current
            // order so the swap is deterministic.
            manual_ids = ordered_ids.to_vec();
        }

        // Position indices within manual_ids for the target and neighbor.
        let target_in_manual = manual_ids
            .iter()
            .position(|candidate| *candidate == id)
            .unwrap_or(manual_ids.len());
        let neighbor_in_manual = manual_ids
            .iter()
            .position(|candidate| *candidate == ordered_ids[neighbor])
            .unwrap_or(manual_ids.len());

        // If either is missing from the manual region (auto items beyond the
        // head), extend the manual region up to and including both.
        let extend_to = target_in_manual.max(neighbor_in_manual);
        while manual_ids.len() <= extend_to {
            // Pull the next auto item from ordered_ids not yet in manual_ids.
            let next = ordered_ids
                .iter()
                .find(|candidate| !manual_ids.contains(candidate))
                .ok_or_else(|| ModelError::BookmarkNotFound(id.to_string()))?;
            manual_ids.push(*next);
        }

        // Swap inside manual_ids.
        let target_in_manual = manual_ids
            .iter()
            .position(|candidate| *candidate == id)
            .ok_or_else(|| ModelError::BookmarkNotFound(id.to_string()))?;
        let neighbor_in_manual = manual_ids
            .iter()
            .position(|candidate| *candidate == ordered_ids[neighbor])
            .ok_or_else(|| ModelError::BookmarkNotFound(id.to_string()))?;
        manual_ids.swap(target_in_manual, neighbor_in_manual);

        // Persist the new keys.
        for (index, manual_id) in manual_ids.iter().enumerate() {
            if let Some(bookmark) = self
                .bookmarks
                .iter_mut()
                .find(|bookmark| bookmark.id == *manual_id)
            {
                bookmark.sort_key = Some(index as u64);
            }
        }
        Ok(())
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
    fn move_bookmark_up_swaps_manual_order() {
        use super::MoveDirection;
        let mut db = Database::default();
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("a");
        let b = temp.path().join("b");
        let c = temp.path().join("c");
        for dir in [&a, &b, &c] {
            std::fs::create_dir(dir).unwrap();
        }
        db.add_bookmark(a, Some("a".into()), None).unwrap();
        db.add_bookmark(b, Some("b".into()), None).unwrap();
        db.add_bookmark(c, Some("c".into()), None).unwrap();
        let ids: Vec<uuid::Uuid> = db.bookmarks.iter().map(|bm| bm.id).collect();
        // Move "b" (index 1) up: it should pin [b, a, c].
        db.move_bookmark(ids[1], MoveDirection::Up, &ids).unwrap();
        assert_eq!(db.bookmarks[1].sort_key, Some(0));
        assert_eq!(db.bookmarks[0].sort_key, Some(1));
        assert_eq!(db.bookmarks[2].sort_key, Some(2));
    }

    #[test]
    fn move_bookmark_at_boundary_is_a_no_op() {
        use super::MoveDirection;
        let mut db = Database::default();
        let temp = tempfile::tempdir().unwrap();
        db.add_bookmark(temp.path().to_path_buf(), Some("solo".into()), None)
            .unwrap();
        let ids: Vec<uuid::Uuid> = db.bookmarks.iter().map(|bm| bm.id).collect();
        db.move_bookmark(ids[0], MoveDirection::Up, &ids).unwrap();
        db.move_bookmark(ids[0], MoveDirection::Down, &ids).unwrap();
        assert_eq!(db.bookmarks[0].sort_key, None); // untouched
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
