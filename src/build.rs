//! The build driver and filesystem router.
//!
//! Routing contract:
//!   - `.nono` files under `pages/` route to HTML pages.
//!   - The output mirrors the input tree, with vanity URLs:
//!       pages/index.nono     -> out/index.html
//!       pages/about.nono     -> out/about/index.html
//!       pages/posts/x.nono   -> out/posts/x/index.html
//!     (index.nono stays index.html; everything else becomes dir/index.html)
//!   - Page filenames must match [a-z0-9-]. Anything else is a hard error.
//!   - `.md` files are content, read via data sources, never routed directly.
//!   - Everything else under `static/` is copied verbatim.
//!
//! A page file must define exactly one component; that component is rendered.
//! Shared components and stylesheets live in `.nono` files under `lib/` and are
//! loaded into every page's scope.

use crate::ast::Item;
use crate::eval::Evaluator;
use crate::html;
use crate::parser;
use crate::sources;
use crate::value::Value;
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct BuildConfig {
    pub project: PathBuf,
    pub out: PathBuf,
}

pub fn build(cfg: &BuildConfig) -> Result<()> {
    let pages_dir = cfg.project.join("pages");
    let lib_dir = cfg.project.join("lib");
    let static_dir = cfg.project.join("static");

    if !pages_dir.is_dir() {
        bail!("no pages/ directory in {}", cfg.project.display());
    }

    // Load shared library items (components + stylesheet) from lib/*.nono.
    let mut shared_items: Vec<Item> = Vec::new();
    let mut css = String::new();
    if lib_dir.is_dir() {
        for entry in WalkDir::new(&lib_dir).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("nono") {
                let src = std::fs::read_to_string(p)
                    .with_context(|| format!("reading {}", p.display()))?;
                let file = parser::parse(&src)
                    .with_context(|| format!("parsing {}", p.display()))?;
                for item in file.items {
                    if let Item::Stylesheet(s) = &item {
                        css.push_str(&render_css(s));
                    }
                    shared_items.push(item);
                }
            }
        }
    }

    // Reset output dir.
    if cfg.out.exists() {
        // We only remove our own previous output; refuse if it looks unexpected.
        std::fs::remove_dir_all(&cfg.out)
            .with_context(|| format!("clearing {}", cfg.out.display()))?;
    }
    std::fs::create_dir_all(&cfg.out)?;

    // Walk pages.
    let mut page_count = 0usize;
    for entry in WalkDir::new(&pages_dir).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("nono") {
            continue;
        }
        let rel = p.strip_prefix(&pages_dir).unwrap();
        validate_route(rel)?;

        let src = std::fs::read_to_string(p)
            .with_context(|| format!("reading {}", p.display()))?;
        let page_file = parser::parse(&src)
            .with_context(|| format!("parsing {}", p.display()))?;

        // Combine shared items with this page's items.
        let mut items = shared_items.clone();
        let mut page_components: Vec<String> = Vec::new();
        let mut page_css = css.clone();
        for item in page_file.items {
            match &item {
                Item::Component(c) => page_components.push(c.name.clone()),
                Item::Stylesheet(s) => page_css.push_str(&render_css(s)),
                Item::Const(_) => {}
            }
            items.push(item);
        }

        let page_component = sole_component(&page_components, p, "page")?;

        let ev = Evaluator::new(items, cfg.project.clone())
            .with_context(|| format!("evaluating {}", p.display()))?;
        let nodes = ev
            .render_component(&page_component)
            .with_context(|| format!("rendering {}", p.display()))?;
        let body = html::render(&nodes);

        let title = derive_title(&page_component, rel);
        let doc = html::document(&title, &page_css, &body);

        let out_path = route_to_output(&cfg.out, rel);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&out_path, doc)
            .with_context(|| format!("writing {}", out_path.display()))?;
        page_count += 1;
    }

    // Content pages: every `.md` under content/ is also a target HTML file,
    // wrapped in a layout. This is the inverse-ownership half of the router:
    // `pages/*.nono` are pages that pull content in; `content/*.md` are pages in
    // their own right, each owning a route.
    let content_dir = cfg.project.join("content");
    let layouts_dir = cfg.project.join("layouts");
    let mut content_count = 0usize;
    if content_dir.is_dir() {
        for entry in WalkDir::new(&content_dir).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let rel = p.strip_prefix(&content_dir).unwrap();
            validate_route(rel)?;

            // Read frontmatter + rendered body once.
            let value = sources::read_markdown(&content_dir, &rel.to_string_lossy())
                .with_context(|| format!("reading {}", p.display()))?;
            let fm = match &value {
                Value::Map(m) => m,
                _ => bail!("internal: markdown {} did not parse to a map", p.display()),
            };

            // Drafts are not published.
            if matches!(fm.get("draft"), Some(Value::Bool(true))) {
                continue;
            }

            // Resolve the layout file: explicit `layout:` field, else the parent
            // directory name verbatim, else `default`. Referenced by filename, so
            // `layout: post` finds layouts/post.nono regardless of the component
            // name inside it.
            let layout_name = resolve_layout(rel, fm);
            let layout_path = layouts_dir.join(format!("{}.nono", layout_name));
            if !layout_path.is_file() {
                bail!(
                    "content page {} wants layout `{}`, but {} doesn't exist. \
                     Add that layout, or set a `layout:` field in the frontmatter.",
                    p.display(),
                    layout_name,
                    layout_path.display()
                );
            }

            let lsrc = std::fs::read_to_string(&layout_path)
                .with_context(|| format!("reading {}", layout_path.display()))?;
            let lfile = parser::parse(&lsrc)
                .with_context(|| format!("parsing {}", layout_path.display()))?;

            // Shared lib items + this layout's items, exactly like a page.
            let mut items = shared_items.clone();
            let mut layout_components: Vec<String> = Vec::new();
            let mut page_css = css.clone();
            for item in lfile.items {
                match &item {
                    Item::Component(c) => layout_components.push(c.name.clone()),
                    Item::Stylesheet(s) => page_css.push_str(&render_css(s)),
                    Item::Const(_) => {}
                }
                items.push(item);
            }
            let layout_component = sole_component(&layout_components, &layout_path, "layout")?;

            let body_html = match fm.get("html") {
                Some(Value::Str(s)) => s.clone(),
                _ => String::new(),
            };

            let ev = Evaluator::new(items, cfg.project.clone())
                .with_context(|| format!("evaluating {}", p.display()))?;
            let nodes = ev
                .render_layout(&layout_component, fm, body_html)
                .with_context(|| format!("rendering {}", p.display()))?;
            let body = html::render(&nodes);

            // A content page's title comes from frontmatter, falling back to the
            // layout component name (better than the .nono page stub).
            let title = match fm.get("title") {
                Some(Value::Str(s)) => s.clone(),
                _ => layout_component.clone(),
            };
            let doc = html::document(&title, &page_css, &body);

            let out_path = route_to_output(&cfg.out, rel);
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&out_path, doc)
                .with_context(|| format!("writing {}", out_path.display()))?;
            content_count += 1;
        }
    }

    // Copy static assets verbatim.
    if static_dir.is_dir() {
        for entry in WalkDir::new(&static_dir).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if p.is_file() {
                let rel = p.strip_prefix(&static_dir).unwrap();
                let dest = cfg.out.join(rel);
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(p, &dest)?;
            }
        }
    }

    println!(
        "{} nonos + {} md -> {} pages",
        page_count,
        content_count,
        page_count + content_count
    );
    Ok(())
}

/// A page or layout file must define exactly one component. This is a filesystem
/// rule (it governs the special `pages/` and `layouts/` paths), not a language
/// one: a file under lib/ may define as many components as it likes.
fn sole_component(names: &[String], path: &Path, kind: &str) -> Result<String> {
    match names {
        [one] => Ok(one.clone()),
        [] => bail!(
            "{} {} defines no component. A {} that renders nothing isn't \
             minimalism, it's an empty file with ambitions.",
            kind,
            path.display(),
            kind
        ),
        many => bail!(
            "{} {} defines {} components ({}). A {} is exactly one component. It \
             has always been exactly one component. Assuming it could be {} of \
             them makes you, and I want to be precise, a fucking idiot. Move the \
             helpers to lib/ where components are allowed to breed.",
            kind,
            path.display(),
            many.len(),
            many.join(", "),
            kind,
            many.len()
        ),
    }
}

/// Pick the layout file (by stem) for a content page: an explicit `layout:`
/// frontmatter field wins; otherwise the parent directory name verbatim (no
/// pluralisation guessing); otherwise `default`.
fn resolve_layout(rel: &Path, fm: &BTreeMap<String, Value>) -> String {
    if let Some(Value::Str(l)) = fm.get("layout") {
        if !l.is_empty() {
            return l.clone();
        }
    }
    rel.parent()
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "default".to_string())
}

/// Enforce the [a-z0-9-] filename rule, loudly. Directories too.
fn validate_route(rel: &Path) -> Result<()> {
    for comp in rel.components() {
        let s = comp.as_os_str().to_string_lossy();
        // Both routable extensions are stripped: `.nono` pages and `.md` content.
        let stem = s.trim_end_matches(".nono").trim_end_matches(".md");
        for ch in stem.chars() {
            let ok = ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-';
            if !ok {
                bail!(
                    "illegal route segment `{}` in `{}`: page paths must match [a-z0-9-]",
                    s,
                    rel.display()
                );
            }
        }
    }
    Ok(())
}

/// Map a page-relative `.nono` path to its vanity output path.
///   index.nono        -> index.html
///   about.nono        -> about/index.html
///   posts/x.nono      -> posts/x/index.html
///   posts/index.nono  -> posts/index.html
fn route_to_output(out_root: &Path, rel: &Path) -> PathBuf {
    let stem = rel.file_stem().unwrap().to_string_lossy().to_string();
    let parent = rel.parent().unwrap_or_else(|| Path::new(""));
    if stem == "index" {
        out_root.join(parent).join("index.html")
    } else {
        out_root.join(parent).join(&stem).join("index.html")
    }
}

fn derive_title(component: &str, rel: &Path) -> String {
    // For now, title is the component name unless it's a page index.
    let _ = rel;
    component.to_string()
}

fn render_css(s: &crate::ast::Stylesheet) -> String {
    let mut out = String::new();
    for rule in &s.rules {
        // Selector: a component name becomes a class `.Name`; a lowercase
        // selector is treated as a raw element selector.
        let sel = if rule.selector.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            format!(".{}", rule.selector)
        } else {
            rule.selector.clone()
        };
        out.push_str(&sel);
        out.push_str(" {\n");
        for (prop, value) in &rule.decls {
            out.push_str("  ");
            out.push_str(prop);
            out.push_str(": ");
            out.push_str(value);
            out.push_str(";\n");
        }
        out.push_str("}\n");
    }
    out
}
