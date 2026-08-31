use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Output};

use assert_cmd::Command;
use mkd::setup::{BLOCK_END, BLOCK_START, SetupError, install_managed_block, remove_managed_block};
use mkd::shell::{Shell, init_script};
use predicates::prelude::*;

#[test]
fn install_is_idempotent_and_preserves_user_content() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join(".bashrc");
    fs::write(&profile, b"export EDITOR=vim\n").unwrap();

    install_managed_block(&profile, "mkd() { :; }\n").unwrap();
    let installed = fs::read(&profile).unwrap();
    install_managed_block(&profile, "mkd() { :; }\n").unwrap();

    assert_eq!(fs::read(&profile).unwrap(), installed);
    let text = String::from_utf8(installed).unwrap();
    assert_eq!(text.matches(BLOCK_START).count(), 1);
    assert_eq!(text.matches(BLOCK_END).count(), 1);
    assert!(text.contains("export EDITOR=vim\n"));
}

#[test]
fn existing_managed_block_is_replaced_and_original_is_backed_up() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join(".bashrc");
    let original =
        format!("export EDITOR=vim\n{BLOCK_START}\nold code\n{BLOCK_END}\nalias g=git\n");
    fs::write(&profile, original.as_bytes()).unwrap();

    install_managed_block(&profile, "new code\n").unwrap();

    assert_eq!(
        fs::read(temp.path().join(".bashrc.mkd-backup")).unwrap(),
        original.as_bytes()
    );
    assert_eq!(
        fs::read_to_string(&profile).unwrap(),
        format!("export EDITOR=vim\n{BLOCK_START}\nnew code\n{BLOCK_END}\nalias g=git\n")
    );
}

#[test]
fn malformed_or_ambiguous_markers_refuse_to_modify_file() {
    let cases = [
        format!("alias g=git\n{BLOCK_START}\n"),
        format!("alias g=git\n{BLOCK_END}\n"),
        format!("{BLOCK_START}\none\n{BLOCK_END}\n{BLOCK_START}\ntwo\n{BLOCK_END}\n"),
        format!("{BLOCK_START}\none\n{BLOCK_START}\ntwo\n{BLOCK_END}\n{BLOCK_END}\n"),
        format!("{BLOCK_END}\ncode\n{BLOCK_START}\n"),
    ];

    for (index, original) in cases.into_iter().enumerate() {
        let temp = tempfile::tempdir().unwrap();
        let profile = temp.path().join(format!("profile-{index}"));
        fs::write(&profile, original.as_bytes()).unwrap();

        assert!(matches!(
            install_managed_block(&profile, "code\n"),
            Err(SetupError::MalformedBlock)
        ));
        assert_eq!(fs::read(&profile).unwrap(), original.as_bytes());
        assert!(matches!(
            remove_managed_block(&profile),
            Err(SetupError::MalformedBlock)
        ));
        assert_eq!(fs::read(&profile).unwrap(), original.as_bytes());
        assert!(
            !temp
                .path()
                .join(format!("profile-{index}.mkd-backup"))
                .exists()
        );
    }
}

#[test]
fn managed_marker_text_inside_the_script_is_rejected_without_writing() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile");

    assert!(matches!(
        install_managed_block(&profile, &format!("echo ok\n{BLOCK_END}\n")),
        Err(SetupError::MalformedBlock)
    ));
    assert!(!profile.exists());
}

#[test]
fn backup_failure_preserves_the_original_profile() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile");
    let original = b"user content\n";
    fs::write(&profile, original).unwrap();
    fs::create_dir(temp.path().join("profile.mkd-backup")).unwrap();

    assert!(install_managed_block(&profile, "code\n").is_err());
    assert_eq!(fs::read(&profile).unwrap(), original);
}

#[test]
fn remove_is_idempotent_and_only_removes_a_complete_block() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join(".zshrc");
    let installed = format!("before\n{BLOCK_START}\ncode\n{BLOCK_END}\nafter\n");
    fs::write(&profile, installed.as_bytes()).unwrap();

    remove_managed_block(&profile).unwrap();
    assert_eq!(fs::read_to_string(&profile).unwrap(), "before\nafter\n");
    assert_eq!(
        fs::read(temp.path().join(".zshrc.mkd-backup")).unwrap(),
        installed.as_bytes()
    );

    let once_removed = fs::read(&profile).unwrap();
    remove_managed_block(&profile).unwrap();
    assert_eq!(fs::read(&profile).unwrap(), once_removed);
    remove_managed_block(&temp.path().join("missing-profile")).unwrap();
}

#[test]
fn setup_cli_installs_and_updates_the_exact_override_profile() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("custom profile");
    fs::write(&profile, b"user content\n").unwrap();

    Command::cargo_bin("mkd")
        .unwrap()
        .env("MKD_SHELL_PROFILE", &profile)
        .args(["setup", "bash", "--yes"])
        .assert()
        .success();
    let first = fs::read(&profile).unwrap();
    assert!(String::from_utf8_lossy(&first).contains(BLOCK_START));

    Command::cargo_bin("mkd")
        .unwrap()
        .env("MKD_SHELL_PROFILE", &profile)
        .args(["setup", "bash", "--yes"])
        .assert()
        .success();
    assert_eq!(fs::read(&profile).unwrap(), first);
}

#[test]
fn setup_dry_run_previews_without_creating_any_paths() {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("not-created");
    let profile = parent.join("profile");

    Command::cargo_bin("mkd")
        .unwrap()
        .env("MKD_SHELL_PROFILE", &profile)
        .args(["setup", "fish", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains(BLOCK_START).and(predicate::str::contains(BLOCK_END)));

    assert!(!parent.exists());
    assert!(!PathBuf::from(format!("{}.mkd-backup", profile.display())).exists());
}

#[test]
fn dry_run_of_existing_profile_does_not_create_a_backup() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile");
    let original = b"user content\n";
    fs::write(&profile, original).unwrap();

    Command::cargo_bin("mkd")
        .unwrap()
        .env("MKD_SHELL_PROFILE", &profile)
        .args(["setup", "bash", "--dry-run"])
        .assert()
        .success();

    assert_eq!(fs::read(&profile).unwrap(), original);
    assert!(!temp.path().join("profile.mkd-backup").exists());
}

#[test]
fn removing_an_uninstalled_profile_is_non_terminal_safe_and_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("missing-profile");

    Command::cargo_bin("mkd")
        .unwrap()
        .env("MKD_SHELL_PROFILE", &profile)
        .args(["setup", "bash", "--remove"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no change needed"));

    assert!(!profile.exists());
}

#[test]
fn setup_refuses_non_terminal_modification_without_yes() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile");
    let original = b"user content\n";
    fs::write(&profile, original).unwrap();

    Command::cargo_bin("mkd")
        .unwrap()
        .env("MKD_SHELL_PROFILE", &profile)
        .args(["setup", "zsh"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--yes"));

    assert_eq!(fs::read(&profile).unwrap(), original);
    assert!(!temp.path().join("profile.mkd-backup").exists());
}

#[test]
fn yes_does_not_bypass_malformed_block_checks() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile");
    let original = format!("user content\n{BLOCK_START}\n");
    fs::write(&profile, original.as_bytes()).unwrap();

    Command::cargo_bin("mkd")
        .unwrap()
        .env("MKD_SHELL_PROFILE", &profile)
        .args(["setup", "bash", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("malformed"));

    assert_eq!(fs::read(&profile).unwrap(), original.as_bytes());
}

#[test]
fn bash_init_has_complete_routing_and_failure_guards() {
    let script = init_script(Shell::Bash);
    assert!(script.contains("mkd() {"));
    assert!(script.contains("command mkd __select"));
    assert!(script.contains("command mkd \"$@\""));
    assert!(script.contains("dir=\"$(command mkd __select)\" || return $?"));
    assert!(script.contains("if [ -n \"$dir\" ]"));
    assert!(script.contains("builtin cd -- \"$dir\""));
}

#[test]
fn zsh_init_has_complete_routing_and_failure_guards() {
    let script = init_script(Shell::Zsh);
    assert!(script.contains("mkd() {"));
    assert!(script.contains("command mkd __select"));
    assert!(script.contains("command mkd \"$@\""));
    assert!(script.contains("dir=\"$(command mkd __select)\" || return $?"));
    assert!(script.contains("if [ -n \"$dir\" ]"));
    assert!(script.contains("builtin cd -- \"$dir\""));
}

#[test]
fn fish_init_has_complete_routing_and_failure_guards() {
    let script = init_script(Shell::Fish);
    assert!(script.contains("function mkd"));
    assert!(script.contains("command mkd __select"));
    assert!(script.contains("command mkd $argv"));
    assert!(script.contains("return $status"));
    assert!(script.contains("set -l select_status $status"));
    assert!(script.contains("if test $select_status -ne 0"));
    assert!(script.contains("return $select_status"));
    assert!(script.contains("if test (string length -- \"$dir\") -gt 0"));
    assert!(script.contains("builtin cd -- \"$dir\""));
}

#[test]
fn powershell_init_has_application_lookup_and_failure_guards() {
    let script = init_script(Shell::Powershell);
    assert!(script.contains("function mkd"));
    assert!(script.contains("Get-Command mkd -CommandType Application"));
    assert!(script.contains("& $application.Path @Arguments"));
    assert!(script.contains("& $application.Path __select"));
    assert!(script.contains("$selectStatus = $LASTEXITCODE"));
    assert!(script.contains("if ($selectStatus -ne 0)"));
    assert!(script.contains("$global:LASTEXITCODE = $selectStatus"));
    assert!(script.contains("[string]::IsNullOrEmpty($dir)"));
    assert!(script.contains("Set-Location -LiteralPath $dir"));
    assert!(!script.contains("Invoke-Expression"));
}

#[test]
fn select_external_contract_is_named_select_but_stays_unimplemented() {
    Command::cargo_bin("mkd")
        .unwrap()
        .arg("__select")
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "interactive selection is not available",
        ));
}

#[test]
fn bash_function_forwards_arguments_status_and_selection_safely() {
    let Some(bash) = command_path("bash", &["--version"]) else {
        eprintln!("skipping Bash behavior test: bash is unavailable");
        return;
    };
    let fixture = FakeFixture::bash();
    let args = ["plain", "with space", "*literal?", "ユニコード"];

    let output = fixture.run_bash(
        &bash,
        "mkd \"$@\"; mkd_status=$?; printf '%s' \"$mkd_status\" > \"$FAKE_STATUS_FILE\"",
        &args,
        "",
        0,
        17,
    );
    assert_success("Bash argument forwarding", &output);
    assert_eq!(fs::read(&fixture.args_path).unwrap(), nul_join(&args));
    assert_eq!(fs::read(&fixture.status_path).unwrap(), b"17");

    let target = fixture.special_target();
    fs::create_dir_all(&target).unwrap();
    fixture.run_bash_selection(&bash, Some(&target), 0);
    assert_marker_at(&target.join("marker"), &target);

    fixture.run_bash_selection(&bash, None, 0);
    assert_marker_at(&fixture.root.path().join("marker"), fixture.root.path());

    fixture.run_bash_selection(&bash, Some(&target), 23);
    assert_marker_at(&fixture.root.path().join("marker"), fixture.root.path());
    assert!(!target.join("marker").exists());
    assert_eq!(fs::read(&fixture.status_path).unwrap(), b"23");
}

#[test]
fn powershell_function_forwards_arguments_status_and_selection_safely() {
    let Some(powershell) = command_path("pwsh", &["-NoProfile", "-Command", "exit 0"])
        .or_else(|| command_path("powershell", &["-NoProfile", "-Command", "exit 0"]))
    else {
        eprintln!("skipping PowerShell behavior test: PowerShell is unavailable");
        return;
    };
    let fixture = FakeFixture::powershell(&powershell);
    let args = ["plain", "with space", "*literal?", "ユニコード"];
    let quoted_args = args
        .iter()
        .map(|arg| format!("'{}'", arg.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(", ");
    let body = format!(
        "$forwarded = @({quoted_args}); mkd @forwarded; $mkdStatus = $LASTEXITCODE; [IO.File]::WriteAllText($env:FAKE_STATUS_FILE, $mkdStatus)"
    );

    let output = fixture.run_powershell(&powershell, &body, "", 0, 17);
    assert_success("PowerShell argument forwarding", &output);
    assert_eq!(fs::read(&fixture.args_path).unwrap(), nul_join(&args));
    assert_eq!(fs::read(&fixture.status_path).unwrap(), b"17");

    let target = fixture.special_target();
    fs::create_dir_all(&target).unwrap();
    fixture.run_powershell_selection(&powershell, Some(&target), 0);
    assert_marker_at(&target.join("marker"), &target);

    fixture.run_powershell_selection(&powershell, None, 0);
    assert_marker_at(&fixture.root.path().join("marker"), fixture.root.path());

    fixture.run_powershell_selection(&powershell, Some(&target), 23);
    assert_marker_at(&fixture.root.path().join("marker"), fixture.root.path());
    assert!(!target.join("marker").exists());
    assert_eq!(fs::read(&fixture.status_path).unwrap(), b"23");
}

#[test]
fn bash_selection_with_newline_path_does_not_change_directory() {
    let Some(bash) = command_path("bash", &["--version"]) else {
        eprintln!("skipping Bash newline selection test: bash is unavailable");
        return;
    };
    let fixture = FakeFixture::bash();
    let output = fixture.run_bash(
        &bash,
        "mkd; mkd_status=$?; printf '%s' \"$mkd_status\" > \"$FAKE_STATUS_FILE\"; : > \"$PWD/$MARKER_NAME\"",
        &[],
        "path-with-newline\nnot-a-directory",
        0,
        0,
    );
    assert_success("Bash newline selection", &output);
    assert_eq!(fs::read(&fixture.status_path).unwrap(), b"1");
    assert!(fixture.root.path().join("marker").is_file());
}

#[test]
fn init_command_stdout_is_exactly_the_generated_script() {
    for (name, shell) in [
        ("bash", Shell::Bash),
        ("zsh", Shell::Zsh),
        ("fish", Shell::Fish),
        ("powershell", Shell::Powershell),
    ] {
        let output = Command::cargo_bin("mkd")
            .unwrap()
            .args(["init", name])
            .output()
            .unwrap();
        assert!(output.status.success(), "init {name} failed: {output:?}");
        assert_eq!(output.stdout, init_script(shell).as_bytes(), "init {name}");
        assert!(
            output.stderr.is_empty(),
            "init {name} stderr: {:?}",
            output.stderr
        );
    }
}

#[test]
fn generated_scripts_have_no_unsafe_eval_target_path() {
    for shell in [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::Powershell] {
        let script = init_script(shell);
        assert!(!script.contains("eval \"$dir\""));
        assert!(!script.contains("eval $dir"));
    }
}

fn assert_success(context: &str, output: &Output) {
    assert!(
        output.status.success(),
        "{context} failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_marker_at(marker: &Path, expected_directory: &Path) {
    assert!(marker.is_file(), "expected marker at {}", marker.display());
    let actual = fs::canonicalize(marker.parent().unwrap()).unwrap();
    let expected = fs::canonicalize(expected_directory).unwrap();
    assert_eq!(actual, expected);
}

fn command_path(name: &str, args: &[&str]) -> Option<PathBuf> {
    ProcessCommand::new(name)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|_| PathBuf::from(name))
}

fn nul_join(args: &[&str]) -> Vec<u8> {
    let mut joined = Vec::new();
    for arg in args {
        joined.extend_from_slice(arg.as_bytes());
        joined.push(0);
    }
    joined
}

struct FakeFixture {
    root: tempfile::TempDir,
    bin_dir: PathBuf,
    init_script_path: PathBuf,
    args_path: PathBuf,
    status_path: PathBuf,
}

impl FakeFixture {
    fn bash() -> Self {
        let root = tempfile::tempdir().unwrap();
        let bin_dir = root.path().join("bin");
        fs::create_dir(&bin_dir).unwrap();
        let application = bin_dir.join("mkd");
        fs::write(&application, fake_bash_application()).unwrap();
        #[cfg(unix)]
        make_executable(&application);
        let init_script_path = root.path().join("init.bash");
        fs::write(&init_script_path, init_script(Shell::Bash)).unwrap();
        Self::new(root, bin_dir, init_script_path)
    }

    fn powershell(powershell: &Path) -> Self {
        let root = tempfile::tempdir().unwrap();
        let bin_dir = root.path().join("bin");
        fs::create_dir(&bin_dir).unwrap();
        #[cfg(windows)]
        {
            fs::write(
                bin_dir.join("mkd.cmd"),
                format!(
                    "@echo off\r\n\"{}\" -NoProfile -File \"%~dp0fake-mkd.ps1\" %*\r\n",
                    powershell.display()
                ),
            )
            .unwrap();
            fs::write(bin_dir.join("fake-mkd.ps1"), fake_powershell_application()).unwrap();
        }
        #[cfg(unix)]
        {
            let application = bin_dir.join("mkd");
            fs::write(&application, fake_bash_application()).unwrap();
            make_executable(&application);
        }
        let init_script_path = root.path().join("init.ps1");
        fs::write(&init_script_path, init_script(Shell::Powershell)).unwrap();
        Self::new(root, bin_dir, init_script_path)
    }

    fn special_target(&self) -> PathBuf {
        #[cfg(unix)]
        {
            self.root.path().join("folder with * wildcard-unicode")
        }
        #[cfg(windows)]
        {
            self.root
                .path()
                .join("folder with space")
                .join("ユニコード")
        }
    }

    fn new(root: tempfile::TempDir, bin_dir: PathBuf, init_script_path: PathBuf) -> Self {
        Self {
            args_path: root.path().join("args"),
            status_path: root.path().join("status"),
            root,
            bin_dir,
            init_script_path,
        }
    }

    fn path_env(&self) -> std::ffi::OsString {
        let old = env::var_os("PATH").unwrap_or_default();
        let mut paths = vec![self.bin_dir.clone()];
        paths.extend(env::split_paths(&old));
        env::join_paths(paths).unwrap()
    }

    fn run_bash(
        &self,
        bash: &Path,
        body: &str,
        args: &[&str],
        selection_path: &str,
        selection_status: u8,
        argument_status: u8,
    ) -> Output {
        let command = format!(
            "source \"$(cygpath -u \"$INIT_SCRIPT\" 2>/dev/null || printf '%s' \"$INIT_SCRIPT\")\"; {body}"
        );
        ProcessCommand::new(bash)
            .args(["--noprofile", "--norc", "-c", &command, "mkd-test"])
            .args(args)
            .current_dir(self.root.path())
            .env("PATH", self.path_env())
            .env("INIT_SCRIPT", &self.init_script_path)
            .env("FAKE_ARGS_FILE", &self.args_path)
            .env("FAKE_STATUS_FILE", &self.status_path)
            .env("FAKE_SELECT_PATH", selection_path)
            .env("FAKE_SELECT_STATUS", selection_status.to_string())
            .env("FAKE_ARGUMENT_STATUS", argument_status.to_string())
            .env("MARKER_NAME", "marker")
            .output()
            .unwrap()
    }

    fn run_bash_selection(&self, bash: &Path, target: Option<&Path>, selection_status: u8) {
        let marker_name = "marker";
        let body = "mkd; mkd_status=$?; printf '%s' \"$mkd_status\" > \"$FAKE_STATUS_FILE\"; : > \"$PWD/$MARKER_NAME\"";
        let expected_target = target.map(Path::to_path_buf);
        let target = target.map(bash_path).unwrap_or_default();
        let marker = self.root.path().join(marker_name);
        let _ = fs::remove_file(&marker);
        if let Some(target) = expected_target.as_ref() {
            let _ = fs::remove_file(target.join(marker_name));
        }
        let output = self.run_bash(bash, body, &[], &target, selection_status, 0);
        assert_success("Bash selection", &output);
        if selection_status == 0 && !target.is_empty() {
            assert!(expected_target.unwrap().join(marker_name).is_file());
        } else {
            assert!(marker.is_file());
        }
    }

    fn run_powershell(
        &self,
        powershell: &Path,
        body: &str,
        selection_path: &str,
        selection_status: u8,
        argument_status: u8,
    ) -> Output {
        let command = format!(". $env:INIT_SCRIPT; {body}");
        ProcessCommand::new(powershell)
            .args(["-NoProfile", "-Command", &command])
            .current_dir(self.root.path())
            .env("PATH", self.path_env())
            .env("INIT_SCRIPT", &self.init_script_path)
            .env("FAKE_ARGS_FILE", &self.args_path)
            .env("FAKE_STATUS_FILE", &self.status_path)
            .env("FAKE_SELECT_PATH", selection_path)
            .env("FAKE_SELECT_STATUS", selection_status.to_string())
            .env("FAKE_ARGUMENT_STATUS", argument_status.to_string())
            .output()
            .unwrap()
    }

    fn run_powershell_selection(
        &self,
        powershell: &Path,
        target: Option<&Path>,
        selection_status: u8,
    ) {
        let marker_name = "marker";
        let body = "mkd; $mkdStatus = $LASTEXITCODE; [IO.File]::WriteAllText($env:FAKE_STATUS_FILE, [string]$mkdStatus); [IO.File]::WriteAllText((Join-Path (Get-Location).Path $env:MARKER_NAME), 'x')";
        let expected_target = target.map(Path::to_path_buf);
        let target = target
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        let marker = self.root.path().join(marker_name);
        let _ = fs::remove_file(&marker);
        if let Some(target) = expected_target.as_ref() {
            let _ = fs::remove_file(target.join(marker_name));
        }
        let output = ProcessCommand::new(powershell)
            .args([
                "-NoProfile",
                "-Command",
                &format!(". $env:INIT_SCRIPT; {body}"),
            ])
            .current_dir(self.root.path())
            .env("PATH", self.path_env())
            .env("INIT_SCRIPT", &self.init_script_path)
            .env("FAKE_ARGS_FILE", &self.args_path)
            .env("FAKE_STATUS_FILE", &self.status_path)
            .env("FAKE_SELECT_PATH", &target)
            .env("FAKE_SELECT_STATUS", selection_status.to_string())
            .env("FAKE_ARGUMENT_STATUS", "0")
            .env("MARKER_NAME", marker_name)
            .output()
            .unwrap();
        assert_success("PowerShell selection", &output);
        if selection_status == 0 && !target.is_empty() {
            assert!(expected_target.unwrap().join(marker_name).is_file());
        } else {
            assert!(self.root.path().join(marker_name).is_file());
        }
    }
}

fn bash_path(path: &Path) -> String {
    #[cfg(windows)]
    {
        let output = ProcessCommand::new("cygpath")
            .args(["-u", &path.to_string_lossy()])
            .output()
            .expect("cygpath is required when Bash is available on Windows");
        assert!(output.status.success(), "cygpath failed: {output:?}");
        String::from_utf8(output.stdout)
            .unwrap()
            .trim_end_matches(['\r', '\n'])
            .to_owned()
    }
    #[cfg(not(windows))]
    {
        path.to_string_lossy().into_owned()
    }
}

fn fake_bash_application() -> &'static str {
    "#!/usr/bin/env bash\nif [ \"$1\" = __select ]; then\n    printf '%s' \"$FAKE_SELECT_PATH\"\n    exit \"${FAKE_SELECT_STATUS:-0}\"\nfi\n: > \"$FAKE_ARGS_FILE\"\nfor arg in \"$@\"; do printf '%s\\0' \"$arg\" >> \"$FAKE_ARGS_FILE\"; done\nexit \"${FAKE_ARGUMENT_STATUS:-0}\"\n"
}

#[cfg(windows)]
fn fake_powershell_application() -> &'static str {
    "param([Parameter(ValueFromRemainingArguments=$true)][string[]] $Arguments)\nif ($Arguments.Count -eq 1 -and $Arguments[0] -eq '__select') { [Console]::Write($env:FAKE_SELECT_PATH); exit [int]$env:FAKE_SELECT_STATUS }\n$bytes = [Text.UTF8Encoding]::new($false).GetBytes(($Arguments -join [char]0) + [char]0)\n[IO.File]::WriteAllBytes($env:FAKE_ARGS_FILE, $bytes)\nexit [int]$env:FAKE_ARGUMENT_STATUS\n"
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}
