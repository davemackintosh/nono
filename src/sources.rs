//! Build-time data sources.
//!
//! These are the functions callable from `const` declarations and expressions:
//! `glob(...)`, `lastfm.recent(...)`, `markdown(...)`. They run synchronously
//! during evaluation (Go-style, no async) and return `Value`s. The markup layer
//! never knows whether a value came from disk or the network.

use crate::value::Value;
use anyhow::{anyhow, bail, Context, Result};
use std::collections::BTreeMap;
use std::path::Path;

/// Read a markdown file, split frontmatter from body, render body to HTML.
/// Returns a Map with keys: title, date, kind, draft, path, slug, html, plus
/// any other frontmatter fields verbatim.
pub fn read_markdown(root: &Path, rel: &str) -> Result<Value> {
    let full = root.join(rel);
    let raw = std::fs::read_to_string(&full)
        .with_context(|| format!("reading markdown {}", full.display()))?;
    parse_markdown_str(&raw, rel)
}

fn parse_markdown_str(raw: &str, rel: &str) -> Result<Value> {
    let (frontmatter, body) = split_frontmatter(raw);

    let mut map: BTreeMap<String, Value> = BTreeMap::new();

    if let Some(fm) = frontmatter {
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(fm).context("parsing frontmatter YAML")?;
        if let serde_yaml::Value::Mapping(m) = parsed {
            for (k, v) in m {
                if let serde_yaml::Value::String(key) = k {
                    map.insert(key, yaml_to_value(v));
                }
            }
        }
    }

    // Render the body to HTML, pulling a table of contents out as we go. We
    // collect the parser events, give every heading a unique slug id (mutating
    // the event so the emitted HTML carries `id="..."`) and record its
    // {level, text, id}, then serialise. The body's anchors and the TOC's links
    // come from the same slugs, so they can never drift apart.
    use pulldown_cmark::{html, Event, Options, Parser as MdParser};
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    let mut events: Vec<Event> = MdParser::new_ext(body, opts).collect();
    let headings = extract_headings(&mut events);
    let mut html_out = String::new();
    html::push_html(&mut html_out, events.into_iter());

    map.insert("html".into(), Value::Str(html_out));
    map.insert("headings".into(), Value::List(headings));
    map.insert("path".into(), Value::Str(rel.to_string()));
    map.insert("slug".into(), Value::Str(slug_of(rel)));

    // Default kind/draft if not given, so `match` and filters always work.
    map.entry("kind".into())
        .or_insert_with(|| Value::Str("Essay".into()));
    map.entry("draft".into()).or_insert(Value::Bool(false));

    Ok(Value::Map(map))
}

/// Walk the parsed events, give every heading a unique slug `id` (mutating the
/// heading event in place so `html::push_html` emits `id="..."`), and return the
/// table of contents as a List of Maps with `level`, `text` and `id`. The body
/// anchors and the TOC links are both built from these ids, so they always agree.
fn extract_headings(events: &mut [pulldown_cmark::Event]) -> Vec<Value> {
    use pulldown_cmark::{Event, Tag, TagEnd};

    let mut toc = Vec::new();
    // Slug -> times seen, so repeated heading text gets `-1`, `-2`, ... suffixes
    // and every id stays unique within the document.
    let mut seen: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();

    let mut i = 0;
    while i < events.len() {
        let level = match &events[i] {
            Event::Start(Tag::Heading { level, .. }) => *level as usize,
            _ => {
                i += 1;
                continue;
            }
        };

        // Concatenate the text (and inline code) between here and the matching
        // heading end; that's the visible heading label.
        let mut text = String::new();
        let mut j = i + 1;
        while j < events.len() {
            match &events[j] {
                Event::Text(t) | Event::Code(t) => text.push_str(t),
                Event::End(TagEnd::Heading(_)) => break,
                _ => {}
            }
            j += 1;
        }

        let id = unique_slug(&text, &mut seen);
        if let Event::Start(Tag::Heading { id: hid, .. }) = &mut events[i] {
            *hid = Some(id.clone().into());
        }

        let mut m = BTreeMap::new();
        m.insert("level".into(), Value::Number(level as f64));
        m.insert("text".into(), Value::Str(text));
        m.insert("id".into(), Value::Str(id));
        toc.push(Value::Map(m));

        i = j + 1;
    }

    toc
}

/// A GitHub-ish heading slug: lowercase, runs of non-alphanumerics become a
/// single dash, leading/trailing dashes trimmed. Empty text falls back to
/// `section` so an id is always produced.
fn slugify(text: &str) -> String {
    let mut s = String::new();
    let mut prev_dash = false;
    for c in text.chars() {
        if c.is_alphanumeric() {
            s.extend(c.to_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            s.push('-');
            prev_dash = true;
        }
    }
    let trimmed = s.trim_matches('-');
    if trimmed.is_empty() {
        "section".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Slugify, then disambiguate against ids already handed out in this document.
fn unique_slug(text: &str, seen: &mut std::collections::BTreeMap<String, usize>) -> String {
    let base = slugify(text);
    let n = seen.entry(base.clone()).or_insert(0);
    let id = if *n == 0 {
        base.clone()
    } else {
        format!("{}-{}", base, n)
    };
    *n += 1;
    id
}

fn slug_of(rel: &str) -> String {
    Path::new(rel)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

/// Split a `---`-delimited YAML frontmatter block from the markdown body.
fn split_frontmatter(raw: &str) -> (Option<&str>, &str) {
    let trimmed = raw.trim_start();
    if let Some(rest) = trimmed.strip_prefix("---") {
        // find the closing ---
        if let Some(end) = rest.find("\n---") {
            let fm = &rest[..end];
            let body_start = end + 4; // skip "\n---"
            let body = rest[body_start..].trim_start_matches('\n');
            return (Some(fm), body);
        }
    }
    (None, raw)
}

fn yaml_to_value(y: serde_yaml::Value) -> Value {
    match y {
        serde_yaml::Value::Null => Value::Nil,
        serde_yaml::Value::Bool(b) => Value::Bool(b),
        serde_yaml::Value::Number(n) => Value::Number(n.as_f64().unwrap_or(0.0)),
        serde_yaml::Value::String(s) => Value::Str(s),
        serde_yaml::Value::Sequence(seq) => {
            Value::List(seq.into_iter().map(yaml_to_value).collect())
        }
        serde_yaml::Value::Mapping(m) => {
            let mut map = BTreeMap::new();
            for (k, v) in m {
                if let serde_yaml::Value::String(key) = k {
                    map.insert(key, yaml_to_value(v));
                }
            }
            Value::Map(map)
        }
        serde_yaml::Value::Tagged(t) => yaml_to_value(t.value),
    }
}

/// `glob("content/posts/*.md")` -> a sorted List of markdown Maps.
/// Each entry is read and parsed exactly like `read_markdown`.
pub fn glob(root: &Path, pattern: &str) -> Result<Value> {
    let matches = glob_paths(root, pattern)?;
    let mut out = Vec::new();
    for rel in matches {
        let v = read_markdown(root, &rel)?;
        out.push(v);
    }
    // Drafts are unpublished, full stop: they get no page (see build.rs) and they
    // don't show up in listings either. Drop them here so `for post in posts`
    // never has to think about it.
    out.retain(|v| !matches!(v.get_field("draft"), Some(Value::Bool(true))));
    // Sort by date descending if present, else by slug.
    out.sort_by(|a, b| {
        let da = a
            .get_field("date")
            .map(|v| v.to_string())
            .unwrap_or_default();
        let db = b
            .get_field("date")
            .map(|v| v.to_string())
            .unwrap_or_default();
        db.cmp(&da)
    });
    Ok(Value::List(out))
}

/// Minimal glob supporting a single `*` in the final path segment.
/// Good enough for `content/posts/*.md`; errors loudly on anything fancier.
fn glob_paths(root: &Path, pattern: &str) -> Result<Vec<String>> {
    let star_count = pattern.matches('*').count();
    if star_count == 0 {
        // literal file
        if root.join(pattern).exists() {
            return Ok(vec![pattern.to_string()]);
        }
        return Ok(vec![]);
    }
    if star_count > 1 {
        bail!("glob supports at most one '*': {}", pattern);
    }

    let p = Path::new(pattern);
    let dir = p.parent().unwrap_or_else(|| Path::new(""));
    let file_pat = p
        .file_name()
        .and_then(|f| f.to_str())
        .ok_or_else(|| anyhow!("bad glob pattern: {}", pattern))?;

    // Split the file pattern around the single '*'.
    let (prefix, suffix) = file_pat
        .split_once('*')
        .ok_or_else(|| anyhow!("glob '*' must be in the filename: {}", pattern))?;

    let abs_dir = root.join(dir);
    let mut out = Vec::new();
    if abs_dir.is_dir() {
        for entry in std::fs::read_dir(&abs_dir)
            .with_context(|| format!("reading dir {}", abs_dir.display()))?
        {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(prefix)
                && name.ends_with(suffix)
                && name.len() >= prefix.len() + suffix.len()
            {
                let rel = if dir.as_os_str().is_empty() {
                    name.to_string()
                } else {
                    format!("{}/{}", dir.display(), name)
                };
                out.push(rel);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// `http_get("https://...")` -> the JSON response parsed into a Value (object ->
/// Map, array -> List, and so on). This is the generic network primitive:
/// services like Last.fm are built on top of it in the standard library, not in
/// the compiler core.
///
/// A 4xx/5xx response or a transport error becomes an Err, which aborts the
/// build with a clear message. A wrong username (Last.fm answers 404) or a dead
/// network should be loud, not silently empty.
pub fn http_get(url: &str) -> Result<Value> {
    let resp: serde_json::Value = ureq::get(url)
        .call()
        .with_context(|| format!("calling {}", url))?
        .into_json()
        .with_context(|| format!("parsing JSON from {}", url))?;
    Ok(json_to_value(resp))
}

/// `env("NONO_LASTFM_KEY")` -> the environment variable as a Str. Errors if the
/// variable is unset, so a missing secret fails the build with a clear message
/// instead of silently sending an empty value.
pub fn env_var(name: &str) -> Result<Value> {
    match std::env::var(name) {
        Ok(v) => Ok(Value::Str(v)),
        Err(_) => bail!("environment variable `{}` is not set", name),
    }
}

fn json_to_value(j: serde_json::Value) -> Value {
    match j {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => Value::Number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => Value::Str(s),
        serde_json::Value::Array(a) => Value::List(a.into_iter().map(json_to_value).collect()),
        serde_json::Value::Object(o) => {
            let mut map = BTreeMap::new();
            for (k, v) in o {
                map.insert(k, json_to_value(v));
            }
            Value::Map(map)
        }
    }
}
