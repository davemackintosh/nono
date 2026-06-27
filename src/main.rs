//! nono — No, no, I'll do it myself.
//!
//! A spite-driven static site generator. Compiles a directory of `.nono` files
//! and markdown content into static HTML. Ships nothing to the browser.

use anyhow::Result;
use clap::{Parser, Subcommand};
use nono::{build, parser, scaffold, serve};
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
    /// Scaffold a new project from the blog template.
    New {
        /// Where to write the new project. Must be empty or not yet exist.
        #[arg(short, long)]
        path: PathBuf,
    },
    /// Parse a single .nono file and print its AST (debugging).
    Parse { file: PathBuf },
    /// Build and serve the site over HTTP, rebuilding on every request.
    Dev {
        /// Project directory (contains pages/, lib/, content/).
        #[arg(default_value = ".")]
        project: PathBuf,
        /// Port to serve on. The default is deliberately juvenile.
        #[arg(short, long, default_value_t = 6969)]
        port: u16,
        /// Output directory for the dev build (defaults to a temp dir).
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Build { project, out } => {
            let cfg = build::BuildConfig { project, out };
            let stats = build::build(&cfg)?;
            println!(
                "{} nonos + {} md -> {} pages",
                stats.nono_pages,
                stats.content_pages,
                stats.nono_pages + stats.content_pages
            );
        }
        Command::New { path } => {
            let files = scaffold::new_project(&path)?;
            println!(
                "scaffolded {} files into {}. `cd {}` then `nono dev`.",
                files,
                path.display(),
                path.display()
            );
        }
        Command::Parse { file } => {
            let src = std::fs::read_to_string(&file)?;
            let parsed = parser::parse(&src)?;
            println!("{:#?}", parsed);
        }
        Command::Dev { project, port, out } => {
            let out = out.unwrap_or_else(|| std::env::temp_dir().join("nono-dev"));
            serve::serve(project, out, port)?;
        }
    }
    Ok(())
}
