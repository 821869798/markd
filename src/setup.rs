use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;

use crate::shell::{self, Shell};

pub const BLOCK_START: &str = "# >>> mkd initialize >>>";
pub const BLOCK_END: &str = "# <<< mkd initialize <<<";

static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupAction {
    Install,
    Update,
    Remove,
    Unchanged,
}

impl SetupAction {
    pub fn changes_profile(self) -> bool {
        matches!(self, Self::Install | Self::Update | Self::Remove)
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Install => "install managed block",
            Self::Update => "update managed block",
            Self::Remove => "remove managed block",
            Self::Unchanged => "no change needed",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SetupRequest {
    pub shell: Option<Shell>,
    pub remove: bool,
    pub dry_run: bool,
    pub yes: bool,
}

#[derive(Debug)]
pub struct SetupPlan {
    pub shell: Shell,
    pub profile: PathBuf,
    pub action: SetupAction,
    pub rendered_block: String,
}

impl SetupPlan {
    pub fn apply(&self) -> Result<(), SetupError> {
        match self.action {
            SetupAction::Install | SetupAction::Update => {
                install_managed_block(&self.profile, shell::init_script(self.shell))
            }
            SetupAction::Remove => remove_managed_block(&self.profile),
            SetupAction::Unchanged => Ok(()),
        }
    }
}

#[derive(Debug, Error)]
pub enum SetupError {
    #[error("shell could not be detected; specify one of bash, zsh, fish, or powershell")]
    UnknownShell,
    #[error("unknown shell in SHELL: {0}")]
    UnsupportedShell(String),
    #[error("cannot locate the home directory for {0}")]
    MissingHome(&'static str),
    #[error(
        "PowerShell profile query failed; run `mkd init powershell` and add its output manually"
    )]
    PowerShellProfileQuery,
    #[error("profile contains malformed, duplicate, or nested mkd managed block markers")]
    MalformedBlock,
    #[error("profile changed while setup was preparing the update: {0}")]
    ConcurrentChange(PathBuf),
    #[error("failed to {operation} {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub success: bool,
    pub stdout: Vec<u8>,
}

pub trait Environment {
    fn var_os(&self, name: &str) -> Option<OsString>;
    fn run(&self, program: &str, args: &[&str]) -> io::Result<CommandOutput>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemEnvironment;

impl Environment for SystemEnvironment {
    fn var_os(&self, name: &str) -> Option<OsString> {
        env::var_os(name)
    }

    fn run(&self, program: &str, args: &[&str]) -> io::Result<CommandOutput> {
        let output = Command::new(program).args(args).output()?;
        Ok(CommandOutput {
            success: output.status.success(),
            stdout: output.stdout,
        })
    }
}

pub fn detect_shell(environment: &impl Environment) -> Result<Shell, SetupError> {
    if let Some(shell) = environment.var_os("SHELL") {
        return parse_shell_path(&shell);
    }

    #[cfg(windows)]
    if query_powershell_profile(environment).is_ok() {
        return Ok(Shell::Powershell);
    }

    Err(SetupError::UnknownShell)
}

pub fn locate_profile(shell: Shell, environment: &impl Environment) -> Result<PathBuf, SetupError> {
    if let Some(profile) = environment.var_os("MKD_SHELL_PROFILE") {
        return Ok(PathBuf::from(profile));
    }

    match shell {
        Shell::Bash => Ok(home_directory(environment, "bash")?.join(".bashrc")),
        Shell::Zsh => Ok(home_directory(environment, "zsh")?.join(".zshrc")),
        Shell::Fish => Ok(home_directory(environment, "fish")?
            .join(".config")
            .join("fish")
            .join("config.fish")),
        Shell::Powershell => query_powershell_profile(environment),
    }
}

pub fn create_plan(
    request: &SetupRequest,
    environment: &impl Environment,
) -> Result<SetupPlan, SetupError> {
    let shell = match request.shell {
        Some(shell) => shell,
        None => detect_shell(environment)?,
    };
    let profile = locate_profile(shell, environment)?;
    let rendered_block = render_managed_block(shell::init_script(shell));
    let current = read_optional(&profile)?;
    let block = match current.as_deref() {
        Some(bytes) => inspect_block(bytes)?,
        None => None,
    };
    let action = if request.remove {
        if block.is_some() {
            SetupAction::Remove
        } else {
            SetupAction::Unchanged
        }
    } else {
        match current {
            None => SetupAction::Install,
            Some(ref bytes) => {
                let updated = installed_bytes(bytes, block, rendered_block.as_bytes());
                if updated == *bytes {
                    SetupAction::Unchanged
                } else if block.is_some() {
                    SetupAction::Update
                } else {
                    SetupAction::Install
                }
            }
        }
    };

    Ok(SetupPlan {
        shell,
        profile,
        action,
        rendered_block,
    })
}

pub fn install_managed_block(path: &Path, script: &str) -> Result<(), SetupError> {
    let rendered = render_managed_block(script);
    inspect_block(rendered.as_bytes())?;
    let current = read_optional(path)?;
    let block = match current.as_deref() {
        Some(bytes) => inspect_block(bytes)?,
        None => None,
    };
    let updated = installed_bytes(
        current.as_deref().unwrap_or_default(),
        block,
        rendered.as_bytes(),
    );
    if current.as_deref() == Some(updated.as_slice()) {
        return Ok(());
    }
    replace_profile(path, current.as_deref(), &updated)
}

pub fn remove_managed_block(path: &Path) -> Result<(), SetupError> {
    let Some(current) = read_optional(path)? else {
        return Ok(());
    };
    let Some(block) = inspect_block(&current)? else {
        return Ok(());
    };
    let mut updated = Vec::with_capacity(current.len() - (block.end - block.start));
    updated.extend_from_slice(&current[..block.start]);
    updated.extend_from_slice(&current[block.end..]);
    replace_profile(path, Some(&current), &updated)
}

fn parse_shell_path(value: &OsStr) -> Result<Shell, SetupError> {
    let path = Path::new(value);
    let name = path
        .file_name()
        .unwrap_or(value)
        .to_string_lossy()
        .to_ascii_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name);
    match name {
        "bash" => Ok(Shell::Bash),
        "zsh" => Ok(Shell::Zsh),
        "fish" => Ok(Shell::Fish),
        "pwsh" | "powershell" => Ok(Shell::Powershell),
        _ => Err(SetupError::UnsupportedShell(
            value.to_string_lossy().into_owned(),
        )),
    }
}

fn home_directory(
    environment: &impl Environment,
    shell: &'static str,
) -> Result<PathBuf, SetupError> {
    environment
        .var_os("HOME")
        .or_else(|| environment.var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or(SetupError::MissingHome(shell))
}

fn query_powershell_profile(environment: &impl Environment) -> Result<PathBuf, SetupError> {
    const ARGS: &[&str] = &["-NoProfile", "-Command", "$PROFILE.CurrentUserCurrentHost"];
    for program in ["pwsh", "powershell"] {
        let Ok(output) = environment.run(program, ARGS) else {
            continue;
        };
        if !output.success {
            continue;
        }
        let Ok(stdout) = String::from_utf8(output.stdout) else {
            continue;
        };
        let profile = stdout.trim_end_matches(['\r', '\n']);
        if !profile.is_empty() {
            return Ok(PathBuf::from(profile));
        }
    }
    Err(SetupError::PowerShellProfileQuery)
}

#[derive(Debug, Clone, Copy)]
struct BlockRange {
    start: usize,
    end: usize,
}

fn inspect_block(bytes: &[u8]) -> Result<Option<BlockRange>, SetupError> {
    let starts = occurrences(bytes, BLOCK_START.as_bytes());
    let ends = occurrences(bytes, BLOCK_END.as_bytes());
    if starts.is_empty() && ends.is_empty() {
        return Ok(None);
    }
    if starts.len() != 1 || ends.len() != 1 || starts[0] >= ends[0] {
        return Err(SetupError::MalformedBlock);
    }

    let start = starts[0];
    let end_marker = ends[0];
    validate_marker_line(bytes, start, BLOCK_START.len())?;
    let end = validate_marker_line(bytes, end_marker, BLOCK_END.len())?;
    Ok(Some(BlockRange { start, end }))
}

fn occurrences(bytes: &[u8], needle: &[u8]) -> Vec<usize> {
    bytes
        .windows(needle.len())
        .enumerate()
        .filter_map(|(index, window)| (window == needle).then_some(index))
        .collect()
}

fn validate_marker_line(
    bytes: &[u8],
    marker_start: usize,
    marker_len: usize,
) -> Result<usize, SetupError> {
    if marker_start > 0 && bytes[marker_start - 1] != b'\n' {
        return Err(SetupError::MalformedBlock);
    }
    let marker_end = marker_start + marker_len;
    match bytes.get(marker_end..) {
        Some([]) => Ok(marker_end),
        Some([b'\n', ..]) => Ok(marker_end + 1),
        Some([b'\r', b'\n', ..]) => Ok(marker_end + 2),
        _ => Err(SetupError::MalformedBlock),
    }
}

fn render_managed_block(script: &str) -> String {
    let mut rendered = format!("{BLOCK_START}\n{script}");
    if !script.ends_with('\n') {
        rendered.push('\n');
    }
    rendered.push_str(BLOCK_END);
    rendered.push('\n');
    rendered
}

fn installed_bytes(current: &[u8], block: Option<BlockRange>, rendered: &[u8]) -> Vec<u8> {
    match block {
        Some(block) => {
            let mut updated =
                Vec::with_capacity(current.len() - (block.end - block.start) + rendered.len());
            updated.extend_from_slice(&current[..block.start]);
            updated.extend_from_slice(rendered);
            updated.extend_from_slice(&current[block.end..]);
            updated
        }
        None => {
            let needs_newline = !current.is_empty() && !current.ends_with(b"\n");
            let mut updated =
                Vec::with_capacity(current.len() + usize::from(needs_newline) + rendered.len());
            updated.extend_from_slice(current);
            if needs_newline {
                updated.push(b'\n');
            }
            updated.extend_from_slice(rendered);
            updated
        }
    }
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, SetupError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(io_error("read profile", path, source)),
    }
}

fn replace_profile(path: &Path, original: Option<&[u8]>, updated: &[u8]) -> Result<(), SetupError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.exists() {
        fs::create_dir_all(parent)
            .map_err(|source| io_error("create profile directory", parent, source))?;
    }

    let original_permissions = if original.is_some() {
        Some(
            fs::metadata(path)
                .map_err(|source| io_error("read profile metadata", path, source))?
                .permissions(),
        )
    } else {
        None
    };

    if let Some(original) = original {
        let current = fs::read(path).map_err(|source| io_error("re-read profile", path, source))?;
        if current != original {
            return Err(SetupError::ConcurrentChange(path.to_path_buf()));
        }
        let backup = backup_path(path);
        fs::copy(path, &backup)
            .map_err(|source| io_error("back up profile to", &backup, source))?;
        let backup_file = OpenOptions::new()
            .write(true)
            .open(&backup)
            .map_err(|source| io_error("open profile backup", &backup, source))?;
        backup_file
            .sync_all()
            .map_err(|source| io_error("sync profile backup", &backup, source))?;
        let backup_bytes = fs::read(&backup)
            .map_err(|source| io_error("verify profile backup", &backup, source))?;
        if backup_bytes != original {
            return Err(SetupError::ConcurrentChange(path.to_path_buf()));
        }
    }

    let (temporary_path, mut temporary_file) = create_temporary_file(parent, path)?;
    let write_result = (|| {
        temporary_file
            .write_all(updated)
            .map_err(|source| io_error("write temporary profile", &temporary_path, source))?;
        temporary_file
            .flush()
            .map_err(|source| io_error("flush temporary profile", &temporary_path, source))?;
        temporary_file
            .sync_all()
            .map_err(|source| io_error("sync temporary profile", &temporary_path, source))?;
        if let Some(permissions) = original_permissions {
            temporary_file
                .set_permissions(permissions)
                .map_err(|source| {
                    io_error("set temporary profile permissions", &temporary_path, source)
                })?;
        }
        drop(temporary_file);
        match original {
            Some(expected) => {
                let current = fs::read(path).map_err(|source| {
                    io_error("re-read profile before replacement", path, source)
                })?;
                if current != expected {
                    return Err(SetupError::ConcurrentChange(path.to_path_buf()));
                }
            }
            None if path.exists() => {
                return Err(SetupError::ConcurrentChange(path.to_path_buf()));
            }
            None => {}
        }
        fs::rename(&temporary_path, path)
            .map_err(|source| io_error("replace profile", path, source))?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    write_result
}

fn create_temporary_file(parent: &Path, profile: &Path) -> Result<(PathBuf, File), SetupError> {
    let profile_name = profile.file_name().unwrap_or_else(|| OsStr::new("profile"));
    for _ in 0..100 {
        let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut name = OsString::from(".");
        name.push(profile_name);
        name.push(format!(".mkd-{}.{}.tmp", std::process::id(), sequence));
        let temporary_path = parent.join(name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => return Ok((temporary_path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => {
                return Err(io_error(
                    "create temporary profile",
                    &temporary_path,
                    source,
                ));
            }
        }
    }
    Err(io_error(
        "create temporary profile",
        parent,
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "temporary file names exhausted",
        ),
    ))
}

fn backup_path(path: &Path) -> PathBuf {
    let mut backup = path.as_os_str().to_os_string();
    backup.push(".mkd-backup");
    PathBuf::from(backup)
}

fn io_error(operation: &'static str, path: &Path, source: io::Error) -> SetupError {
    SetupError::Io {
        operation,
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[derive(Default)]
    struct FakeEnvironment {
        variables: HashMap<String, OsString>,
        commands: HashMap<String, io::Result<CommandOutput>>,
    }

    impl Environment for FakeEnvironment {
        fn var_os(&self, name: &str) -> Option<OsString> {
            self.variables.get(name).cloned()
        }

        fn run(&self, program: &str, _args: &[&str]) -> io::Result<CommandOutput> {
            match self.commands.get(program) {
                Some(Ok(output)) => Ok(output.clone()),
                Some(Err(error)) => Err(io::Error::new(error.kind(), error.to_string())),
                None => Err(io::Error::new(io::ErrorKind::NotFound, "not found")),
            }
        }
    }

    #[test]
    fn shell_detection_uses_the_shell_basename() {
        let mut environment = FakeEnvironment::default();
        environment
            .variables
            .insert("SHELL".into(), OsString::from("/usr/local/bin/zsh"));
        assert_eq!(detect_shell(&environment).unwrap(), Shell::Zsh);
    }

    #[test]
    fn unknown_shell_is_an_explicit_error() {
        let mut environment = FakeEnvironment::default();
        environment
            .variables
            .insert("SHELL".into(), OsString::from("/bin/tcsh"));
        assert!(matches!(
            detect_shell(&environment),
            Err(SetupError::UnsupportedShell(_))
        ));
    }

    #[test]
    fn profile_override_is_used_exactly() {
        let mut environment = FakeEnvironment::default();
        environment.variables.insert(
            "MKD_SHELL_PROFILE".into(),
            OsString::from("relative/custom profile"),
        );
        assert_eq!(
            locate_profile(Shell::Fish, &environment).unwrap(),
            PathBuf::from("relative/custom profile")
        );
    }

    #[test]
    fn unix_shell_profiles_follow_the_home_defaults() {
        let mut environment = FakeEnvironment::default();
        environment
            .variables
            .insert("HOME".into(), OsString::from("/home/tester"));
        assert_eq!(
            locate_profile(Shell::Bash, &environment).unwrap(),
            PathBuf::from("/home/tester/.bashrc")
        );
        assert_eq!(
            locate_profile(Shell::Zsh, &environment).unwrap(),
            PathBuf::from("/home/tester/.zshrc")
        );
        assert_eq!(
            locate_profile(Shell::Fish, &environment).unwrap(),
            PathBuf::from("/home/tester/.config/fish/config.fish")
        );
    }

    #[test]
    fn powershell_profile_query_falls_back_to_windows_powershell() {
        let mut environment = FakeEnvironment::default();
        environment.commands.insert(
            "powershell".into(),
            Ok(CommandOutput {
                success: true,
                stdout: b"C:\\Users\\tester\\profile.ps1\r\n".to_vec(),
            }),
        );
        assert_eq!(
            locate_profile(Shell::Powershell, &environment).unwrap(),
            PathBuf::from(r"C:\Users\tester\profile.ps1")
        );
    }

    #[cfg(windows)]
    #[test]
    fn powershell_is_detected_without_a_shell_variable_on_windows() {
        let mut environment = FakeEnvironment::default();
        environment.commands.insert(
            "pwsh".into(),
            Ok(CommandOutput {
                success: true,
                stdout: b"C:\\profile.ps1\r\n".to_vec(),
            }),
        );
        assert_eq!(detect_shell(&environment).unwrap(), Shell::Powershell);
    }
}
