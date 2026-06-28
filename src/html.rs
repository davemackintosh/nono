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
    /// Content that belongs to a named document region (`head`, later `footer`,
    /// ...) rather than where it sits in the tree. Any component can emit one
    /// from anywhere; `extract_hoists` gathers them, in document order, before
    /// serialisation, so they never reach the body. This is what lets nested
    /// components each contribute to the `<head>`.
    Hoist { region: String, children: Vec<Html> },
}

/// HTML void elements that must not have a closing tag.
fn is_void(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

/// The set of lowercase names we treat as real HTML elements. Capitalised
/// names are components and never reach serialisation.
pub fn known_html_tags() -> BTreeSet<&'static str> {
    [
        "html",
        "head",
        "body",
        "title",
        "meta",
        "link",
        "style",
        "script",
        "div",
        "span",
        "p",
        "a",
        "img",
        "ul",
        "ol",
        "li",
        "nav",
        "header",
        "footer",
        "main",
        "article",
        "section",
        "aside",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "time",
        "br",
        "hr",
        "em",
        "strong",
        "code",
        "pre",
        "blockquote",
        "figure",
        "figcaption",
        "table",
        "thead",
        "tbody",
        "tr",
        "td",
        "th",
        "small",
        "button",
        "label",
        "input",
    ]
    .into_iter()
    .collect()
}

/// Pull every `Html::Hoist` out of the tree, wherever it sits and however deeply
/// nested, grouping the contents by region name in document order. Returns the
/// collected regions and the body with the hoist markers removed. A region's
/// content is itself hoist-free (nested hoists are flattened up).
pub fn extract_hoists(
    nodes: Vec<Html>,
) -> (std::collections::BTreeMap<String, Vec<Html>>, Vec<Html>) {
    let mut regions: std::collections::BTreeMap<String, Vec<Html>> =
        std::collections::BTreeMap::new();
    let mut body = Vec::new();
    collect_hoists(nodes, &mut regions, &mut body);
    (regions, body)
}

fn collect_hoists(
    nodes: Vec<Html>,
    regions: &mut std::collections::BTreeMap<String, Vec<Html>>,
    body: &mut Vec<Html>,
) {
    for node in nodes {
        match node {
            Html::Hoist { region, children } => {
                // The region's content may itself contain hoists (a head block
                // wrapping a component that hoists); flatten those up too.
                let mut inner_body = Vec::new();
                collect_hoists(children, regions, &mut inner_body);
                regions.entry(region).or_default().extend(inner_body);
            }
            Html::Element {
                tag,
                attrs,
                children,
            } => {
                let mut inner_body = Vec::new();
                collect_hoists(children, regions, &mut inner_body);
                body.push(Html::Element {
                    tag,
                    attrs,
                    children: inner_body,
                });
            }
            other => body.push(other),
        }
    }
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
        // A hoist that survived to serialisation has no home (no extract pass, or
        // an unknown region): drop it rather than dumping head content in the body.
        Html::Hoist { .. } => {}
        Html::Element {
            tag,
            attrs,
            children,
        } => {
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

/// Wrap body HTML in a complete document. `head_extra` is pre-rendered HTML
/// collected from `head { }` blocks across the component tree (links, meta, and
/// so on); it lands in the document head before the inline `stylesheet {}` CSS,
/// so a page's own `stylesheet {}` still has the last word.
pub fn document(title: &str, head_extra: &str, css: &str, body: &str) -> String {
    let mut out = String::from("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    out.push_str("<meta charset=\"utf-8\" />\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n");
    out.push_str("<title>");
    escape_into(title, &mut out);
    out.push_str("</title>\n");
    if !head_extra.is_empty() {
        out.push_str(head_extra);
        out.push('\n');
    }
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
