// ---------------------------------------------------------------------------
// Copyright:   (c) 2026 ff. Michael Amrhein (michael@adrhinum.de)
// License:     This program is part of a larger application. For license
//              details please read the file LICENSE.TXT provided together
//              with the application.
// ---------------------------------------------------------------------------
// $Source$
// $Revision$

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// CLI utility for managing the keymapperd configuration.
#[derive(Parser)]
#[command(name = "keymapper")]
#[command(version)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Configuration file management.
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Print the configuration file to stdout.
    List,

    /// Validate and diagnose the configuration.
    Check,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Config { command } => match command {
            ConfigCommands::List => cmd_config_list()?,
            ConfigCommands::Check => cmd_config_check()?,
        },
    }

    Ok(())
}

fn load_config() -> Result<(PathBuf, String), Box<dyn std::error::Error>> {
    let path =
        keymapperd::config_path::find_config_path().ok_or_else(|| {
            keymapperd::config_path::print_search_locations();
            "configuration file not found".to_string()
        })?;

    let contents = fs_err::read_to_string(&path)?;

    Ok((path, contents))
}

fn cmd_config_list() -> Result<(), Box<dyn std::error::Error>> {
    let (_path, contents) = load_config()?;
    print!("{contents}");
    Ok(())
}

fn cmd_config_check() -> Result<(), Box<dyn std::error::Error>> {
    let (path, contents) = load_config()?;

    let config = keymapperd::config::AppConfig::load_from_str(&contents)
        .map_err(|err| format!("failed to parse {}: {err}", path.display()))?;

    let diagnostics = config.check();

    if diagnostics.is_empty() {
        println!("{}: no issues found.", path.display());
    } else {
        println!("{}:", path.display());
        for (i, msg) in diagnostics.iter().enumerate() {
            println!("  {} {}", i + 1, msg);
        }
    }

    Ok(())
}
