//! The HTML output tree and its serialiser.
//!
//! Evaluation produces a `Vec<Html>`; this module turns that into a string.
//! A small set of names are treated as raw HTML elements; everything else is a
//! user component that must have been defined and is expanded during eval, so
//! by the time we serialise, the tree contains only real HTML elements and text.

use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub enum Html {
    /// Escaped text content.
    Text(String),
    /// Pre-rendered raw HTML (e.g. markdown output). NOT escaped.
    Raw(String),
    Element {
        tag: String,
        attrs: Vec<(String, String)>,
        children: Vec<Html>,
    },
}

/// HTML void elements that must not have a closing tag.
fn is_void(tag: &str) -> bool {
    matches!(
        tag,
        "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input"
            | "link" | "meta" | "param" | "source" | "track" | "wbr"
    )
}

/// The set of lowercase names we treat as real HTML elements. Capitalised
/// names are components and never reach serialisation.
pub fn known_html_tags() -> BTreeSet<&'static str> {
    [
        "html", "head", "body", "title", "meta", "link", "style", "script",
        "div", "span", "p", "a", "img", "ul", "ol", "li", "nav", "header",
        "footer", "main", "article", "section", "aside", "h1", "h2", "h3",
        "h4", "h5", "h6", "time", "br", "hr", "em", "strong", "code", "pre",
        "blockquote", "figure", "figcaption", "table", "thead", "tbody", "tr",
        "td", "th", "small", "button", "label", "input",
    ]
    .into_iter()
    .collect()
}

pub fn render(nodes: &[Html]) -> String {
    let mut out = String::new();
    for n in nodes {
        render_node(n, &mut out);
    }
    out
}

fn render_node(node: &Html, out: &mut String) {
    match node {
        Html::Text(t) => escape_into(t, out),
        Html::Raw(r) => out.push_str(r),
        Html::Element { tag, attrs, children } => {
            out.push('<');
            out.push_str(tag);
            for (k, v) in attrs {
                out.push(' ');
                out.push_str(k);
                out.push_str("=\"");
                escape_attr_into(v, out);
                out.push('"');
            }
            if is_void(tag) {
                out.push_str(" />");
                return;
            }
            out.push('>');
            for c in children {
                render_node(c, out);
            }
            out.push_str("</");
            out.push_str(tag);
            out.push('>');
        }
    }
}

fn escape_into(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

fn escape_attr_into(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

/// Wrap body HTML in a complete document with the given title and stylesheet.
pub fn document(title: &str, css: &str, body: &str) -> String {
    let mut out = String::from("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    out.push_str("<meta charset=\"utf-8\" />\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n");
    out.push_str("<title>");
    escape_into(title, &mut out);
    out.push_str("</title>\n");
    if !css.is_empty() {
        out.push_str("<style>\n");
        out.push_str(css);
        out.push_str("</style>\n");
    }
    out.push_str("</head>\n<body>\n");
    out.push_str(body);
    out.push_str("\n</body>\n</html>\n");
    out
}
