use std::env;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PathError {
    #[error("cannot determine the current directory: {0}")]
    CurrentDirectory(#[source] std::io::Error),
    #[error("home directory is not available")]
    HomeDirectoryUnavailable,
    #[error("path does not exist: {path}: {source}")]
    Canonicalize {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("path is not a directory: {0}")]
    NotDirectory(PathBuf),
    #[error("application data directory is not available")]
    DataDirectoryUnavailable,
}

pub fn normalize_directory(input: &Path) -> Result<PathBuf, PathError> {
    let base = env::current_dir().map_err(PathError::CurrentDirectory)?;
    normalize_directory_from(input, &base)
}

pub fn normalize_directory_from(input: &Path, base: &Path) -> Result<PathBuf, PathError> {
    let expanded = expand_home(input)?;
    let candidate = if expanded.is_absolute() {
        expanded
    } else {
        base.join(expanded)
    };
    let canonical = candidate
        .canonicalize()
        .map_err(|source| PathError::Canonicalize {
            path: candidate,
            source,
        })?;
    if !canonical.is_dir() {
        return Err(PathError::NotDirectory(canonical));
    }
    Ok(canonical)
}

pub fn default_data_file() -> Result<PathBuf, PathError> {
    let project_dirs =
        ProjectDirs::from("", "", "mkd").ok_or(PathError::DataDirectoryUnavailable)?;
    Ok(project_dirs.data_local_dir().join("bookmarks.json"))
}

fn expand_home(input: &Path) -> Result<PathBuf, PathError> {
    let mut components = input.components();
    let first = components.next();
    let is_tilde = matches!(first, Some(std::path::Component::Normal(value)) if value == "~");
    if !is_tilde {
        return Ok(input.to_path_buf());
    }

    let home = dirs_home().ok_or(PathError::HomeDirectoryUnavailable)?;
    let mut expanded = home;
    expanded.extend(components);
    Ok(expanded)
}

fn dirs_home() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        env::var_os("USERPROFILE").map(PathBuf::from).or_else(|| {
            let drive = env::var_os("HOMEDRIVE")?;
            let path = env::var_os("HOMEPATH")?;
            Some(PathBuf::from(drive).join(path))
        })
    }
    #[cfg(not(windows))]
    {
        env::var_os("HOME").map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use directories::ProjectDirs;
    use std::path::Path;

    #[test]
    fn default_data_file_uses_local_data_directory() {
        let project_dirs = ProjectDirs::from("", "", "mkd").unwrap();
        let expected = project_dirs.data_local_dir().join("bookmarks.json");
        assert_eq!(super::default_data_file().unwrap(), expected);
    }

    #[test]
    fn normalize_relative_directory_to_absolute_path() {
        let temp = tempfile::tempdir().unwrap();
        let result = super::normalize_directory_from(Path::new("."), temp.path()).unwrap();
        assert_eq!(result, temp.path().canonicalize().unwrap());
    }
}
