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
    #[error("path contains unsupported newline characters: {0}")]
    UnsafeCharacters(PathBuf),
    #[error("application data directory is not available")]
    DataDirectoryUnavailable,
}

pub fn normalize_directory(input: &Path) -> Result<PathBuf, PathError> {
    let base = env::current_dir().map_err(PathError::CurrentDirectory)?;
    normalize_directory_from(input, &base)
}

pub fn normalize_directory_from(input: &Path, base: &Path) -> Result<PathBuf, PathError> {
    normalize_directory_from_with_home(input, base, dirs_home())
}

fn normalize_directory_from_with_home(
    input: &Path,
    base: &Path,
    home: Option<PathBuf>,
) -> Result<PathBuf, PathError> {
    validate_path(input)?;
    let expanded = expand_home_from(input, home)?;
    let candidate = if expanded.is_absolute() {
        expanded
    } else {
        base.join(expanded)
    };
    validate_path(&candidate)?;
    let canonical = candidate
        .canonicalize()
        .map_err(|source| PathError::Canonicalize {
            path: candidate,
            source,
        })?;
    validate_path(&canonical)?;
    let canonical = strip_verbatim_prefix(canonical);
    if !canonical.is_dir() {
        return Err(PathError::NotDirectory(canonical));
    }
    Ok(canonical)
}

/// Remove the Windows verbatim prefix (`\\?\`) that `canonicalize` produces.
/// Verbatim paths break `Set-Location`, `Get-ChildItem`, and other PowerShell
/// cmdlets, so every stored and emitted path uses the ordinary drive form.
pub(crate) fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(stripped) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{}", stripped))
    } else if let Some(stripped) = text.strip_prefix(r"\\?\") {
        PathBuf::from(stripped.to_string())
    } else {
        path
    }
}

pub fn validate_path(path: &Path) -> Result<(), PathError> {
    if path.to_string_lossy().contains('\n') || path.to_string_lossy().contains('\r') {
        return Err(PathError::UnsafeCharacters(path.to_path_buf()));
    }
    Ok(())
}
pub fn default_data_file() -> Result<PathBuf, PathError> {
    let project_dirs =
        ProjectDirs::from("", "", "mkd").ok_or(PathError::DataDirectoryUnavailable)?;
    Ok(project_dirs.data_local_dir().join("bookmarks.json"))
}

fn expand_home_from(input: &Path, home: Option<PathBuf>) -> Result<PathBuf, PathError> {
    let mut components = input.components();
    let first = components.next();
    let is_tilde = matches!(first, Some(std::path::Component::Normal(value)) if value == "~");
    if !is_tilde {
        return Ok(input.to_path_buf());
    }

    let home = home.ok_or(PathError::HomeDirectoryUnavailable)?;
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
    use std::path::{Path, PathBuf};

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
        assert_eq!(
            result,
            super::strip_verbatim_prefix(temp.path().canonicalize().unwrap())
        );
    }

    #[cfg(windows)]
    #[test]
    fn normalized_paths_never_carry_a_verbatim_prefix() {
        let temp = tempfile::tempdir().unwrap();
        let result = super::normalize_directory_from(Path::new("."), temp.path()).unwrap();
        let text = result.to_string_lossy();
        assert!(!text.starts_with(r"\\?\"));
        assert_eq!(
            result,
            super::strip_verbatim_prefix(temp.path().canonicalize().unwrap())
        );
    }

    #[cfg(windows)]
    #[test]
    fn verbatim_unc_paths_are_mapped_to_drive_form() {
        use super::strip_verbatim_prefix;
        assert_eq!(
            strip_verbatim_prefix(PathBuf::from(r"\\?\UNC\server\share\dir")),
            PathBuf::from(r"\\server\share\dir")
        );
        assert_eq!(
            strip_verbatim_prefix(PathBuf::from(r"\\?\D:\work")),
            PathBuf::from(r"D:\work")
        );
        assert_eq!(
            strip_verbatim_prefix(PathBuf::from(r"D:\already-plain")),
            PathBuf::from(r"D:\already-plain")
        );
    }

    #[test]
    fn paths_with_newlines_are_rejected_before_filesystem_access() {
        let temp = tempfile::tempdir().unwrap();
        let error =
            super::normalize_directory_from(Path::new("safe\nname"), temp.path()).unwrap_err();
        assert!(matches!(error, super::PathError::UnsafeCharacters(_)));
    }

    #[test]
    fn expanded_home_and_joined_base_paths_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let base_with_newline = temp.path().join("cwd\nwith-newline");
        let base_error =
            super::normalize_directory_from(Path::new("."), &base_with_newline).unwrap_err();
        assert!(matches!(base_error, super::PathError::UnsafeCharacters(_)));

        let home_with_newline = temp.path().join("home\rwith-newline");
        let home_error = super::normalize_directory_from_with_home(
            Path::new("~"),
            temp.path(),
            Some(home_with_newline),
        )
        .unwrap_err();
        assert!(matches!(home_error, super::PathError::UnsafeCharacters(_)));
    }

    #[cfg(unix)]
    #[test]
    fn canonical_symlink_target_with_newline_is_rejected() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target\nwith-newline");
        let link = temp.path().join("safe-link");
        std::fs::create_dir(&target).unwrap();
        symlink(&target, &link).unwrap();

        let error = super::normalize_directory_from(&link, temp.path()).unwrap_err();
        // On macOS the temp dir resolves through /var -> /private/var, so the
        // reported path may carry a different prefix; match on the tail.
        assert!(matches!(
            &error,
            super::PathError::UnsafeCharacters(path)
                if path.ends_with("target\nwith-newline")
        ));
    }
}
