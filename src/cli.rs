use std::env;
use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use clap::{Args, Parser, Subcommand};

use crate::paths;
use crate::shell;
use crate::store::Store;

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
pub struct SetupArgs {}

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
            Some(Command::Setup(_)) => Err(anyhow!("shell setup is not available yet")),
            Some(Command::Select) => Err(anyhow!("interactive selection is not available yet")),
            None => Err(anyhow!("interactive interface is not available yet")),
        }
    }
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

fn store() -> Result<Store> {
    let path = match env::var_os("MKD_DATA_FILE") {
        Some(path) => PathBuf::from(path),
        None => paths::default_data_file()?,
    };
    Ok(Store::at(path))
}
