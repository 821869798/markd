use crate::model::Database;
use serde_json::Error as JsonError;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub struct Store {
    path: PathBuf,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("cannot read database file {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("database file is corrupt {path}: {source}")]
    CorruptData {
        path: PathBuf,
        #[source]
        source: JsonError,
    },
    #[error("cannot serialize database: {0}")]
    Serialize(#[from] JsonError),
    #[error("cannot create database parent directory {path}: {source}")]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot create temporary database file {path}: {source}")]
    CreateTemp {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot write temporary database file {path}: {source}")]
    WriteTemp {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot sync temporary database file {path}: {source}")]
    SyncTemp {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot replace database file {path}: {source}")]
    Replace {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl Store {
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Result<Database, StoreError> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Database::default()),
            Err(source) => {
                return Err(StoreError::Read {
                    path: self.path.clone(),
                    source,
                });
            }
        };
        if contents.trim().is_empty() {
            return Ok(Database::default());
        }
        serde_json::from_str(&contents).map_err(|source| StoreError::CorruptData {
            path: self.path.clone(),
            source,
        })
    }

    pub fn save(&self, database: &Database) -> Result<(), StoreError> {
        let serialized = serde_json::to_vec_pretty(database)?;
        if let Some(parent) = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|source| StoreError::CreateDirectory {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let temp_path = self.temporary_path();
        let result = self.write_and_replace(&temp_path, &serialized);
        if result.is_err() && !matches!(&result, Err(StoreError::CreateTemp { .. })) {
            let _ = fs::remove_file(&temp_path);
        }
        result
    }

    fn temporary_path(&self) -> PathBuf {
        let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("database.json");
        let temp_name = format!(".{name}.tmp-{}-{counter}", std::process::id());
        self.path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(temp_name)
    }

    fn write_and_replace(&self, temp_path: &Path, serialized: &[u8]) -> Result<(), StoreError> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temp_path)
            .map_err(|source| StoreError::CreateTemp {
                path: temp_path.to_path_buf(),
                source,
            })?;
        if let Err(source) = file.write_all(serialized).and_then(|_| file.flush()) {
            return Err(StoreError::WriteTemp {
                path: temp_path.to_path_buf(),
                source,
            });
        }
        file.sync_all().map_err(|source| StoreError::SyncTemp {
            path: temp_path.to_path_buf(),
            source,
        })?;
        drop(file);
        fs::rename(temp_path, &self.path).map_err(|source| StoreError::Replace {
            path: self.path.clone(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Database;
    use std::fs;

    #[test]
    fn save_then_load_round_trips_database() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("bookmarks.json");
        let store = Store::at(file);
        let mut db = Database::default();
        db.add_category("work").unwrap();
        db.add_bookmark(
            temp.path().to_path_buf(),
            Some("repo".into()),
            Some("work".into()),
        )
        .unwrap();
        store.save(&db).unwrap();
        assert_eq!(store.load().unwrap(), db);
    }

    #[test]
    fn corrupt_json_is_not_overwritten() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("bookmarks.json");
        fs::write(&file, "{broken").unwrap();
        let store = Store::at(file.clone());
        assert!(matches!(store.load(), Err(StoreError::CorruptData { .. })));
        assert_eq!(fs::read_to_string(file).unwrap(), "{broken");
    }

    #[test]
    fn empty_file_loads_as_default_database() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("bookmarks.json");
        fs::write(&file, "").unwrap();
        assert_eq!(Store::at(file).load().unwrap(), Database::default());
    }

    #[test]
    fn missing_file_loads_as_default_database() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("missing/bookmarks.json");
        assert_eq!(Store::at(file).load().unwrap(), Database::default());
    }

    #[test]
    fn first_save_creates_parent_directories() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("nested/data/bookmarks.json");
        Store::at(file.clone()).save(&Database::default()).unwrap();
        assert_eq!(Store::at(file).load().unwrap(), Database::default());
    }

    #[test]
    fn saving_replaces_existing_file_atomically() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("bookmarks.json");
        let store = Store::at(file.clone());
        store.save(&Database::default()).unwrap();
        let mut updated = Database::default();
        updated.add_category("work").unwrap();
        store.save(&updated).unwrap();
        assert_eq!(store.load().unwrap(), updated);
    }

    #[test]
    fn failed_replace_does_not_delete_existing_target() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        fs::create_dir(&target).unwrap();
        let store = Store::at(target.clone());
        assert!(store.save(&Database::default()).is_err());
        assert!(target.is_dir());
    }
}
