use std::fs;
use std::path::Path;

use assert_cmd::Command;
use mkd::cli::run_select_with;
use mkd::model::Database;
use mkd::shell::{Shell, init_script};
use mkd::ui::UiError;
use mkd::ui::UiRunner;
use mkd::ui::app::Outcome;
use predicates::prelude::*;

#[test]
fn selection_transaction_outputs_absolute_path_and_records_visit() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("db.json");
    let store = mkd::store::Store::at(data.clone());
    let mut database = Database::default();
    database
        .add_bookmark(temp.path().to_path_buf(), Some("repo".into()), None)
        .unwrap();
    let id = database.bookmarks[0].id;
    store.save(&database).unwrap();
    let now = chrono::Utc::now();
    let output = run_select_with(&store, FakeUi(Some(id)), now).unwrap();
    assert_eq!(
        output,
        format!("{}\n", temp.path().canonicalize().unwrap().display())
    );
    let saved = store.load().unwrap();
    assert_eq!(saved.bookmarks[0].visit_count, 1);
    assert_eq!(saved.bookmarks[0].last_visited_at, Some(now));
}

#[test]
fn cancelled_selection_has_empty_output() {
    let temp = tempfile::tempdir().unwrap();
    let store = mkd_store(temp.path());
    let output = run_select_with(&store, FakeUi(None), chrono::Utc::now()).unwrap();
    assert_eq!(output, "");
}

#[test]
fn vanished_selection_fails_without_recording_visit_or_output() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("gone");
    fs::create_dir(&path).unwrap();
    let store = mkd_store(temp.path());
    let mut database = Database::default();
    database.add_bookmark(path.clone(), None, None).unwrap();
    let id = database.bookmarks[0].id;
    store.save(&database).unwrap();
    fs::remove_dir(&path).unwrap();
    let error = run_select_with(&store, FakeUi(Some(id)), chrono::Utc::now()).unwrap_err();
    assert!(error.to_string().contains("no longer exists"));
    assert_eq!(store.load().unwrap().bookmarks[0].visit_count, 0);
}

#[test]
fn save_failure_fails_without_output_or_visit_update() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("db.json");
    let store = mkd_store(temp.path());
    let mut database = Database::default();
    database
        .add_bookmark(temp.path().to_path_buf(), None, None)
        .unwrap();
    let id = database.bookmarks[0].id;
    store.save(&database).unwrap();
    fs::remove_file(&data).unwrap();
    fs::create_dir(&data).unwrap();
    let error = run_select_with(&store, FakeUi(Some(id)), chrono::Utc::now()).unwrap_err();
    assert!(error.to_string().contains("cannot"));
}

struct FakeUi(Option<uuid::Uuid>);

impl UiRunner for FakeUi {
    fn run(&mut self, _database: Database) -> Result<Outcome, UiError> {
        Ok(self.0.map_or(Outcome::Cancelled, |id| Outcome::Selected {
            id,
            path: std::path::PathBuf::new(),
        }))
    }
}

fn mkd_store(root: &Path) -> mkd::store::Store {
    mkd::store::Store::at(root.join("db.json"))
}
fn mkd(data_file: &Path) -> Command {
    let mut command = Command::cargo_bin("mkd").unwrap();
    command.env("MKD_DATA_FILE", data_file);
    command
}

#[test]
fn init_command_output_matches_generated_script_exactly() {
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
        assert!(output.status.success());
        assert_eq!(output.stdout, init_script(shell).as_bytes());
        assert!(output.stderr.is_empty());
    }
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
fn newline_path_is_rejected_without_creating_database() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("db.json");
    mkd(&data)
        .args(["add", "bad\nname"])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("unsupported newline"));
    assert!(!data.exists());
}

#[test]
fn final_selection_rejects_newline_path_from_hand_edited_database() {
    let temp = tempfile::tempdir().unwrap();
    let store = mkd_store(temp.path());
    let mut database = Database::default();
    database
        .add_bookmark(temp.path().to_path_buf(), Some("repo".into()), None)
        .unwrap();
    let id = database.bookmarks[0].id;
    database.bookmarks[0].path = std::path::PathBuf::from("safe\nname");
    store.save(&database).unwrap();
    let error = run_select_with(&store, FakeUi(Some(id)), chrono::Utc::now()).unwrap_err();
    assert!(error.to_string().contains("unsupported newline"));
    assert_eq!(store.load().unwrap().bookmarks[0].visit_count, 0);
}

#[cfg(unix)]
#[test]
fn final_selection_rejects_symlink_to_newline_path() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let store = mkd_store(temp.path());
    let target = temp.path().join("target\nwith-newline");
    let link = temp.path().join("safe-link");
    fs::create_dir(&target).unwrap();
    symlink(&target, &link).unwrap();

    let mut database = Database::default();
    database
        .add_bookmark(link, Some("repo".into()), None)
        .unwrap();
    let id = database.bookmarks[0].id;
    store.save(&database).unwrap();

    let error = run_select_with(&store, FakeUi(Some(id)), chrono::Utc::now()).unwrap_err();
    assert!(error.to_string().contains("unsupported newline"));
    assert_eq!(store.load().unwrap().bookmarks[0].visit_count, 0);
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
        .stderr(predicate::str::contains("requires a terminal"));
    mkd(&data)
        .arg("__select")
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("requires a terminal"));
}
