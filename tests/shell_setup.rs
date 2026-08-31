use assert_cmd::Command;
use mkd::shell::{Shell, init_script};
use predicates::prelude::*;

#[test]
fn bash_init_routes_arguments_and_changes_directory() {
    let script = init_script(Shell::Bash);
    assert!(script.contains("mkd() {"));
    assert!(script.contains("command mkd __select"));
    assert!(script.contains("builtin cd -- \"$dir\""));
    assert!(script.contains("command mkd \"$@\""));
    assert!(script.contains("[ -n \"$dir\" ]"));
    assert!(script.contains("|| return $?"));
}

#[test]
fn zsh_init_routes_arguments_and_changes_directory() {
    let script = init_script(Shell::Zsh);
    assert!(script.contains("mkd() {"));
    assert!(script.contains("command mkd __select"));
    assert!(script.contains("builtin cd -- \"$dir\""));
    assert!(script.contains("command mkd \"$@\""));
    assert!(script.contains("[ -n \"$dir\" ]"));
    assert!(script.contains("|| return $?"));
}

#[test]
fn fish_init_routes_arguments_and_preserves_selection_status() {
    let script = init_script(Shell::Fish);
    assert!(script.contains("function mkd"));
    assert!(script.contains("command mkd __select"));
    assert!(script.contains("command mkd $argv"));
    assert!(script.contains("set -l select_status $status"));
    assert!(script.contains("builtin cd -- \"$dir\""));
    assert!(script.contains("string length -- \"$dir\""));
}

#[test]
fn powershell_init_uses_application_and_literal_location() {
    let script = init_script(Shell::Powershell);
    assert!(script.contains("function mkd"));
    assert!(script.contains("Get-Command mkd -CommandType Application"));
    assert!(script.contains("__select"));
    assert!(script.contains("@Arguments"));
    assert!(script.contains("Set-Location -LiteralPath $dir"));
    assert!(script.contains("[string]::IsNullOrEmpty($dir)"));
    assert!(!script.contains("Invoke-Expression"));
    assert!(!script.contains("eval "));
}

#[test]
fn init_command_writes_only_script_to_stdout_and_nothing_to_stderr() {
    Command::cargo_bin("mkd")
        .unwrap()
        .args(["init", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("__select"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn init_accepts_all_supported_shell_names() {
    for shell in ["bash", "zsh", "fish", "powershell"] {
        Command::cargo_bin("mkd")
            .unwrap()
            .args(["init", shell])
            .assert()
            .success()
            .stdout(predicate::str::contains("mkd"))
            .stderr(predicate::str::is_empty());
    }
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
fn generated_scripts_have_no_unsafe_eval_target_path() {
    for shell in [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::Powershell] {
        let script = init_script(shell);
        assert!(!script.contains("eval \"$dir\""));
        assert!(!script.contains("eval $dir"));
    }
}
