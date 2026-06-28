//! `nono new` — scaffold a fresh project from the blog template.
//!
//! The template is the `examples/blog` site, baked into the binary at compile
//! time with `include_str!`. That means `nono new` works anywhere, with no repo
//! checkout and no network: the files you see under `examples/blog/` are exactly
//! what gets written out.

use anyhow::{bail, Context, Result};
use std::path::Path;

/// One template file: its path relative to the project root, and its contents.
/// Order doesn't matter; we create parent directories as we go.
const TEMPLATE: &[(&str, &str)] = &[
    (
        "pages/index.nono",
        include_str!("../examples/blog/pages/index.nono"),
    ),
    (
        "pages/about.nono",
        include_str!("../examples/blog/pages/about.nono"),
    ),
    (
        "layouts/posts.nono",
        include_str!("../examples/blog/layouts/posts.nono"),
    ),
    (
        "lib/components.nono",
        include_str!("../examples/blog/lib/components.nono"),
    ),
    (
        "content/posts/how-to-leave-react.md",
        include_str!("../examples/blog/content/posts/how-to-leave-react.md"),
    ),
    (
        "content/posts/a-quick-note.md",
        include_str!("../examples/blog/content/posts/a-quick-note.md"),
    ),
    (
        "content/posts/draft-thing.md",
        include_str!("../examples/blog/content/posts/draft-thing.md"),
    ),
    ("nono.toml", include_str!("../examples/blog/nono.toml")),
    (
        "static/styles.css",
        include_str!("../examples/blog/static/styles.css"),
    ),
    (
        ".github/workflows/deploy.yml",
        include_str!("../examples/blog/.github/workflows/deploy.yml"),
    ),
    (
        ".claude/skills/nono/SKILL.md",
        include_str!("../examples/blog/.claude/skills/nono/SKILL.md"),
    ),
];

/// Write the template into `path`. Refuses to clobber a non-empty directory: this
/// is a starting point, not a thing you point at your existing site by accident.
pub fn new_project(path: &Path) -> Result<usize> {
    if path.exists() {
        let mut entries =
            std::fs::read_dir(path).with_context(|| format!("reading {}", path.display()))?;
        if entries.next().is_some() {
            bail!(
                "{} already exists and isn't empty. Point `nono new` somewhere fresh.",
                path.display()
            );
        }
    }

    for (rel, contents) in TEMPLATE {
        let dest = path.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(&dest, contents).with_context(|| format!("writing {}", dest.display()))?;
    }

    Ok(TEMPLATE.len())
}
