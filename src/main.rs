//! nono — No, no, I'll do it myself.
//!
//! A spite-driven static site generator. Compiles a directory of `.nono` files
//! and markdown content into static HTML. Ships nothing to the browser.

use anyhow::Result;
use clap::{Parser, Subcommand};
use nono::{build, parser};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "nono", version, about = "No, no, I'll do it myself.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build the site.
    Build {
        /// Project directory (contains pages/, lib/, static/).
        #[arg(default_value = ".")]
        project: PathBuf,
        /// Output directory.
        #[arg(short, long, default_value = "out")]
        out: PathBuf,
    },
    /// Parse a single .nono file and print its AST (debugging).
    Parse {
        file: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Build { project, out } => {
            let cfg = build::BuildConfig { project, out };
            build::build(&cfg)?;
        }
        Command::Parse { file } => {
            let src = std::fs::read_to_string(&file)?;
            let parsed = parser::parse(&src)?;
            println!("{:#?}", parsed);
        }
    }
    Ok(())
}
