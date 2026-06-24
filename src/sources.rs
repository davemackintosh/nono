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

    // Render markdown body to HTML.
    use pulldown_cmark::{html, Options, Parser as MdParser};
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    let parser = MdParser::new_ext(body, opts);
    let mut html_out = String::new();
    html::push_html(&mut html_out, parser);

    map.insert("html".into(), Value::Str(html_out));
    map.insert("path".into(), Value::Str(rel.to_string()));
    map.insert("slug".into(), Value::Str(slug_of(rel)));

    // Default kind/draft if not given, so `match` and filters always work.
    map.entry("kind".into())
        .or_insert_with(|| Value::Str("Essay".into()));
    map.entry("draft".into()).or_insert(Value::Bool(false));

    Ok(Value::Map(map))
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
    // Sort by date descending if present, else by slug.
    out.sort_by(|a, b| {
        let da = a.get_field("date").map(|v| v.to_string()).unwrap_or_default();
        let db = b.get_field("date").map(|v| v.to_string()).unwrap_or_default();
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
            if name.starts_with(prefix) && name.ends_with(suffix) && name.len() >= prefix.len() + suffix.len() {
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

/// `lastfm.recent(user = "dave", limit = 10)` -> a List of track Maps.
///
/// Reads the API key from the NONO_LASTFM_KEY environment variable. If the key
/// is absent or the request fails, returns an Err; the caller decides whether
/// to fall back (see eval's handling of `or =`). This keeps the network failure
/// quarantined to the data-source boundary.
pub fn lastfm_recent(user: &str, limit: u32) -> Result<Value> {
    let key = std::env::var("NONO_LASTFM_KEY")
        .map_err(|_| anyhow!("NONO_LASTFM_KEY not set; cannot fetch Last.fm data"))?;

    let url = format!(
        "https://ws.audioscrobbler.com/2.0/?method=user.getrecenttracks&user={}&api_key={}&format=json&limit={}",
        urlencode(user),
        urlencode(&key),
        limit
    );

    let resp: serde_json::Value = ureq::get(&url)
        .call()
        .context("calling Last.fm")?
        .into_json()
        .context("parsing Last.fm JSON")?;

    let tracks = resp
        .get("recenttracks")
        .and_then(|r| r.get("track"))
        .and_then(|t| t.as_array())
        .ok_or_else(|| anyhow!("unexpected Last.fm response shape"))?;

    let mut out = Vec::new();
    for t in tracks {
        let mut m = BTreeMap::new();
        let artist = t
            .get("artist")
            .and_then(|a| a.get("#text"))
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let name = t.get("name").and_then(|s| s.as_str()).unwrap_or("").to_string();
        let album = t
            .get("album")
            .and_then(|a| a.get("#text"))
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let now_playing = t
            .get("@attr")
            .and_then(|a| a.get("nowplaying"))
            .and_then(|s| s.as_str())
            .map(|s| s == "true")
            .unwrap_or(false);

        m.insert("artist".into(), Value::Str(artist));
        m.insert("name".into(), Value::Str(name));
        m.insert("album".into(), Value::Str(album));
        m.insert("now_playing".into(), Value::Bool(now_playing));
        out.push(Value::Map(m));
    }
    Ok(Value::List(out))
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
