use std::env;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use clap::{Args, Parser, Subcommand};

use crate::paths;
use crate::setup::{self, SetupRequest, SystemEnvironment};
use crate::shell;
use crate::store::Store;
use crate::ui::app::Outcome;
use crate::ui::{self, UiError, UiRunner};

pub use crate::shell::Shell;

#[derive(Debug, Parser)]
#[command(name = "mkd", version, about = "Manage directory bookmarks")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Add a directory bookmark.
    Add {
        /// Directory to bookmark; defaults to the current directory.
        path: Option<PathBuf>,
        /// Name used to identify the bookmark.
        #[arg(short, long)]
        name: Option<String>,
        /// Category assigned to the bookmark.
        #[arg(short, long)]
        category: Option<String>,
    },
    /// List directory bookmarks.
    List {
        /// Show only bookmarks in this category.
        #[arg(short, long)]
        category: Option<String>,
    },
    /// Remove a bookmark by UUID or unique name.
    Remove { bookmark: String },
    /// Rename a bookmark by UUID or unique name.
    Rename { bookmark: String, name: String },
    /// Manage bookmark categories.
    Category {
        #[command(subcommand)]
        command: CategoryCommand,
    },
    /// Print shell initialization code.
    Init { shell: Shell },
    /// Configure shell integration.
    Setup(SetupArgs),
    #[command(name = "__select", hide = true)]
    Select,
}

#[derive(Debug, Subcommand)]
pub enum CategoryCommand {
    /// Add a category.
    Add { name: String },
    /// List categories.
    List,
    /// Remove a category and move its bookmarks to default.
    Remove { name: String },
    /// Rename a category and update its bookmarks.
    Rename { old: String, new: String },
}

#[derive(Debug, Args)]
pub struct SetupArgs {
    /// Shell to configure; detected when omitted.
    pub shell: Option<Shell>,
    /// Remove mkd's managed initialization block.
    #[arg(long)]
    pub remove: bool,
    /// Print the planned change without writing files.
    #[arg(long)]
    pub dry_run: bool,
    /// Apply the change without prompting.
    #[arg(long)]
    pub yes: bool,
}

impl Cli {
    pub fn run(self) -> Result<()> {
        match self.command {
            Some(Command::Add {
                path,
                name,
                category,
            }) => add(path, name, category),
            Some(Command::List { category }) => list(category),
            Some(Command::Remove { bookmark }) => remove(&bookmark),
            Some(Command::Rename { bookmark, name }) => rename(&bookmark, &name),
            Some(Command::Category { command }) => category(command),
            Some(Command::Init { shell }) => {
                let stdout = io::stdout();
                let mut output = stdout.lock();
                output.write_all(shell::init_script(shell).as_bytes())?;
                Ok(())
            }
            Some(Command::Setup(arguments)) => setup_shell(arguments),
            Some(Command::Select) => select(),
            None => select(),
        }
    }
}

fn setup_shell(arguments: SetupArgs) -> Result<()> {
    let request = SetupRequest {
        shell: arguments.shell,
        remove: arguments.remove,
        dry_run: arguments.dry_run,
        yes: arguments.yes,
    };
    let plan = setup::create_plan(&request, &SystemEnvironment)?;

    println!("Shell: {}", plan.shell);
    println!("Profile: {}", plan.profile.display());
    println!("Action: {}", plan.action.description());

    if request.dry_run {
        println!("Managed block:\n{}", plan.rendered_block);
        return Ok(());
    }
    if !plan.action.changes_profile() {
        return Ok(());
    }

    if !request.yes {
        let stdin = io::stdin();
        if !stdin.is_terminal() {
            return Err(anyhow!(
                "refusing to modify a shell profile with non-terminal stdin; pass --yes to confirm"
            ));
        }
        eprint!("Apply this change? [y/N] ");
        io::stderr().flush()?;
        let mut answer = String::new();
        stdin.read_line(&mut answer)?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            eprintln!("No changes made.");
            return Ok(());
        }
    }

    plan.apply()?;
    Ok(())
}

fn add(path: Option<PathBuf>, name: Option<String>, category: Option<String>) -> Result<()> {
    let path = match path {
        Some(path) => path,
        None => env::current_dir().map_err(paths::PathError::CurrentDirectory)?,
    };
    let path = paths::normalize_directory(&path)?;
    let store = store()?;
    let mut database = store.load()?;
    let added_name = database.add_bookmark(path, name, category)?.name.clone();
    store.save(&database)?;
    eprintln!("Added bookmark '{added_name}'");
    Ok(())
}

fn list(category: Option<String>) -> Result<()> {
    let database = store()?.load()?;
    let stdout = io::stdout();
    let mut output = stdout.lock();
    for bookmark in database.bookmarks.iter().filter(|bookmark| {
        category
            .as_ref()
            .is_none_or(|category| bookmark.category == *category)
    }) {
        writeln!(
            output,
            "{}\t{}\t{}",
            bookmark.name,
            bookmark.path.display(),
            bookmark.category
        )?;
    }
    Ok(())
}

fn remove(selector: &str) -> Result<()> {
    let store = store()?;
    let mut database = store.load()?;
    let removed = database.remove_bookmark(selector)?;
    store.save(&database)?;
    eprintln!("Removed bookmark '{}'", removed.name);
    Ok(())
}

fn rename(selector: &str, name: &str) -> Result<()> {
    let store = store()?;
    let mut database = store.load()?;
    database.rename_bookmark(selector, name)?;
    store.save(&database)?;
    eprintln!("Renamed bookmark '{selector}' to '{name}'");
    Ok(())
}

fn category(command: CategoryCommand) -> Result<()> {
    let store = store()?;
    let mut database = store.load()?;
    match command {
        CategoryCommand::Add { name } => {
            database.add_category(&name)?;
            store.save(&database)?;
            eprintln!("Added category '{name}'");
        }
        CategoryCommand::List => {
            let stdout = io::stdout();
            let mut output = stdout.lock();
            for category in &database.categories {
                writeln!(output, "{category}")?;
            }
        }
        CategoryCommand::Remove { name } => {
            database.remove_category(&name)?;
            store.save(&database)?;
            eprintln!("Removed category '{name}'");
        }
        CategoryCommand::Rename { old, new } => {
            database.rename_category(&old, &new)?;
            store.save(&database)?;
            eprintln!("Renamed category '{old}' to '{new}'");
        }
    }
    Ok(())
}

fn select() -> Result<()> {
    let stdin = io::stdin();
    let stderr = io::stderr();
    if !stdin.is_terminal() || !stderr.is_terminal() {
        return Err(anyhow!(
            "interactive selection requires a terminal; interactive selection is not available in this environment"
        ));
    }
    let store = store()?;
    let output = run_select_with(&store, PersistentUi { store: &store }, Utc::now())?;
    if !output.is_empty() {
        let stdout = io::stdout();
        let mut writer = stdout.lock();
        writer.write_all(output.as_bytes())?;
    }
    Ok(())
}

struct PersistentUi<'a> {
    store: &'a Store,
}

impl UiRunner for PersistentUi<'_> {
    fn run(&mut self, database: crate::model::Database) -> Result<Outcome, UiError> {
        ui::run_with(database, |mutation| {
            let mut latest = self.store.load().map_err(|error| error.to_string())?;
            mutation.apply(&mut latest)?;
            self.store
                .save(&latest)
                .map_err(|error| error.to_string())?;
            Ok(latest)
        })
    }
}

/// Executes the selection transaction against a UI runner.
pub fn run_select_with<U: UiRunner>(
    store: &Store,
    mut ui_runner: U,
    now: DateTime<Utc>,
) -> Result<String> {
    let database = store.load()?;
    let outcome = ui_runner.run(database)?;
    let Outcome::Selected { id, .. } = outcome else {
        return Ok(String::new());
    };

    let mut latest = store.load()?;
    let bookmark = latest
        .bookmarks
        .iter()
        .find(|bookmark| bookmark.id == id)
        .ok_or_else(|| anyhow!("bookmark no longer exists: {id}"))?;
    paths::validate_path(&bookmark.path)?;
    if !bookmark.path.is_dir() {
        return Err(anyhow!(
            "directory no longer exists or is not a directory: {}",
            bookmark.path.display()
        ));
    }
    let path = bookmark.path.canonicalize().map_err(|error| {
        anyhow!(
            "cannot resolve selected directory {}: {error}",
            bookmark.path.display()
        )
    })?;
    latest.record_visit(id, now)?;
    store.save(&latest)?;
    Ok(format!("{}\n", path.display()))
}

fn store() -> Result<Store> {
    let path = match env::var_os("MKD_DATA_FILE") {
        Some(path) => PathBuf::from(path),
        None => paths::default_data_file()?,
    };
    Ok(Store::at(path))
}
