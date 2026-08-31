use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;

fn mkd(data_file: &Path) -> Command {
    let mut command = Command::cargo_bin("mkd").unwrap();
    command.env("MKD_DATA_FILE", data_file);
    command
}

#[test]
fn add_and_list_bookmark_keep_data_and_status_on_separate_streams() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("db.json");

    mkd(&data)
        .args([
            "add",
            temp.path().to_str().unwrap(),
            "--name",
            "sandbox",
            "--category",
            "work",
        ])
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("Added bookmark 'sandbox'"));

    mkd(&data)
        .args(["list", "--category", "work"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("sandbox")
                .and(predicate::str::contains("work"))
                .and(predicate::str::contains("default").not()),
        )
        .stderr(predicate::str::is_empty());
}

#[test]
fn add_without_path_uses_current_directory() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("db.json");

    mkd(&data)
        .current_dir(temp.path())
        .args(["add", "--name", "here"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    let contents = fs::read_to_string(data).unwrap();
    let stored: serde_json::Value = serde_json::from_str(&contents).unwrap();
    assert_eq!(stored["bookmarks"][0]["name"], "here");
    assert_eq!(
        Path::new(stored["bookmarks"][0]["path"].as_str().unwrap()),
        temp.path().canonicalize().unwrap()
    );
}

#[test]
fn invalid_add_does_not_create_database() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("db.json");

    mkd(&data)
        .current_dir(temp.path())
        .args(["add", "missing-directory"])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("path does not exist"));

    assert!(!data.exists());
}

#[test]
fn rename_and_remove_bookmark() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("db.json");
    mkd(&data)
        .args(["add", temp.path().to_str().unwrap(), "--name", "old"])
        .assert()
        .success();

    mkd(&data)
        .args(["rename", "old", "new"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("Renamed bookmark 'old' to 'new'"));
    mkd(&data)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("new").and(predicate::str::contains("old").not()))
        .stderr(predicate::str::is_empty());

    mkd(&data)
        .args(["remove", "new"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("Removed bookmark 'new'"));
    mkd(&data)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::is_empty());
}

#[test]
fn category_add_and_list() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("db.json");

    mkd(&data)
        .args(["category", "add", "work"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("Added category 'work'"));
    mkd(&data)
        .args(["category", "list"])
        .assert()
        .success()
        .stdout("default\nwork\n")
        .stderr(predicate::str::is_empty());
}

#[test]
fn category_rename_updates_bookmarks() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("db.json");
    mkd(&data)
        .args([
            "add",
            temp.path().to_str().unwrap(),
            "--name",
            "repo",
            "--category",
            "work",
        ])
        .assert()
        .success();

    mkd(&data)
        .args(["category", "rename", "work", "personal"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "Renamed category 'work' to 'personal'",
        ));
    mkd(&data)
        .args(["list", "--category", "personal"])
        .assert()
        .success()
        .stdout(predicate::str::contains("repo"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn category_remove_moves_bookmarks_to_default() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("db.json");
    mkd(&data)
        .args([
            "add",
            temp.path().to_str().unwrap(),
            "--name",
            "repo",
            "--category",
            "work",
        ])
        .assert()
        .success();

    mkd(&data)
        .args(["category", "remove", "work"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("Removed category 'work'"));
    mkd(&data)
        .args(["list", "--category", "default"])
        .assert()
        .success()
        .stdout(predicate::str::contains("repo"))
        .stderr(predicate::str::is_empty());
    mkd(&data)
        .args(["category", "list"])
        .assert()
        .success()
        .stdout("default\n")
        .stderr(predicate::str::is_empty());
}

#[test]
fn help_describes_bookmark_and_category_commands() {
    Command::cargo_bin("mkd")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("add")
                .and(predicate::str::contains("list"))
                .and(predicate::str::contains("category")),
        )
        .stderr(predicate::str::is_empty());
}

#[test]
fn ambiguous_bookmark_name_is_an_error_and_does_not_modify_data() {
    let temp = tempfile::tempdir().unwrap();
    let one = temp.path().join("one");
    let two = temp.path().join("two");
    fs::create_dir_all(&one).unwrap();
    fs::create_dir_all(&two).unwrap();
    let data = temp.path().join("db.json");
    for path in [&one, &two] {
        mkd(&data)
            .args(["add", path.to_str().unwrap(), "--name", "repo"])
            .assert()
            .success();
    }
    let before = fs::read(&data).unwrap();

    mkd(&data)
        .args(["remove", "repo"])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("bookmark name is ambiguous: repo"));

    assert_eq!(fs::read(data).unwrap(), before);
}

#[test]
fn corrupt_database_is_reported_and_never_rewritten() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("db.json");
    fs::write(&data, "{broken").unwrap();

    mkd(&data)
        .args(["category", "add", "work"])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("database file is corrupt"));

    assert_eq!(fs::read_to_string(data).unwrap(), "{broken");
}

#[test]
fn unavailable_interactive_entries_fail_without_stdout_or_panic() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("db.json");

    mkd(&data)
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "interactive interface is not available",
        ));
    mkd(&data)
        .arg("__select")
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "interactive selection is not available",
        ));
}
