//! Project configuration: the optional `nono.toml` at the project root.
//!
//! Everything here is opt-in. A project with no `nono.toml` gets the default
//! (an empty config), so the common case stays config-free. The point of this
//! file is the asset pipeline: a `[build]` section can run shell commands before
//! the site compiles (Tailwind, esbuild, whatever) and link the stylesheets they
//! produce. nono itself still ships nothing dynamic; the build step just decides
//! what static CSS/JS lands in `static/` and the head.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub build: BuildSection,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct BuildSection {
    /// Shell commands run, in order, from the project root before the site is
    /// compiled. They run on every build, including each `nono dev` reload, so
    /// keep them quick (compile assets here; do `npm install` once by hand).
    ///
    /// Two path tokens are substituted before a command runs, so you needn't
    /// hardcode absolute paths: `<rootDir>` is the project root and `<publicDir>`
    /// is its `static/` directory (whose contents become the site root).
    pub steps: Vec<String>,
}

impl Config {
    /// Load `nono.toml` from the project root. A missing file is not an error:
    /// it just means the defaults (no build steps, no linked styles).
    pub fn load(project: &Path) -> Result<Config> {
        let path = project.join("nono.toml");
        match std::fs::read_to_string(&path) {
            Ok(src) => toml::from_str(&src).with_context(|| format!("parsing {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }
}
