//! Build-driver tests: these exercise the filesystem router end to end (temp
//! project in, HTML files out), which the in-memory `render()` tests in
//! integration.rs can't reach. The headline behaviour here is the inverse half
//! of the router: every `.md` is also a target HTML file.

use nono::build::{build, BuildConfig};
use std::fs;
use std::path::PathBuf;

/// A fresh, empty scratch directory unique to a test name.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nono-buildtest-{name}"));
    let _ = fs::remove_dir_all(&dir);
    dir
}

/// Write a file under `root`, creating parent dirs.
fn write(root: &PathBuf, rel: &str, contents: &str) {
    let full = root.join(rel);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, contents).unwrap();
}

#[test]
fn md_file_becomes_a_page() {
    let root = scratch("md_page");
    let proj = root.join("project");
    let out = root.join("out");
    write(
        &proj,
        "pages/index.nono",
        r#"component Home { main { "home" } }"#,
    );
    write(
        &proj,
        "layouts/posts.nono",
        "component PostLayout(title: string) { article { h1 { \"{title}\" } Slot() } }",
    );
    write(
        &proj,
        "content/posts/hello.md",
        "---\ntitle: Hello There\n---\nBody *text*.\n",
    );

    build(&BuildConfig {
        project: proj,
        out: out.clone(),
    })
    .expect("build failed");

    // Vanity URL: content/posts/hello.md -> posts/hello/index.html.
    let page = fs::read_to_string(out.join("posts/hello/index.html")).expect("post page missing");
    // Title from frontmatter, layout h1 from the param, body as raw HTML.
    assert!(page.contains("<title>Hello There</title>"), "got: {page}");
    assert!(page.contains("<h1>Hello There</h1>"), "got: {page}");
    assert!(page.contains("Body <em>text</em>"), "got: {page}");
}

#[test]
fn raw_html_in_markdown_passes_through() {
    // Embedded HTML in a post must survive verbatim (it's injected as Html::Raw),
    // and markdown inside a blank-line-separated raw block still renders. This
    // guards against anyone later turning on an HTML-escaping markdown option.
    let root = scratch("md_raw_html");
    let proj = root.join("project");
    let out = root.join("out");
    write(
        &proj,
        "pages/index.nono",
        r#"component Home { main { "h" } }"#,
    );
    write(
        &proj,
        "layouts/posts.nono",
        "component L(title: string) { article { Slot() } }",
    );
    write(
        &proj,
        "content/posts/embed.md",
        "---\ntitle: Embed\n---\n<details>\n<summary>more</summary>\n\nA **bold** word and <mark>inline</mark>.\n\n</details>\n\n<div class=\"callout\" data-kind=\"note\">raw</div>\n",
    );

    build(&BuildConfig {
        project: proj,
        out: out.clone(),
    })
    .expect("build failed");

    let page = fs::read_to_string(out.join("posts/embed/index.html")).expect("post missing");
    assert!(page.contains("<details>"), "block html: {page}");
    assert!(
        page.contains("<summary>more</summary>"),
        "block html: {page}"
    );
    assert!(page.contains("<mark>inline</mark>"), "inline html: {page}");
    assert!(
        page.contains(r#"<div class="callout" data-kind="note">raw</div>"#),
        "html attrs preserved: {page}"
    );
    // Markdown inside the raw block is still processed.
    assert!(
        page.contains("<strong>bold</strong>"),
        "md in raw block: {page}"
    );
}

#[test]
fn draft_md_is_not_published() {
    let root = scratch("draft");
    let proj = root.join("project");
    let out = root.join("out");
    write(
        &proj,
        "pages/index.nono",
        r#"component Home { main { "home" } }"#,
    );
    write(
        &proj,
        "layouts/posts.nono",
        "component L { article { Slot() } }",
    );
    write(
        &proj,
        "content/posts/secret.md",
        "---\ntitle: Secret\ndraft: true\n---\nshh\n",
    );

    build(&BuildConfig {
        project: proj,
        out: out.clone(),
    })
    .expect("build failed");

    assert!(
        !out.join("posts/secret/index.html").exists(),
        "a draft should not be published"
    );
}

#[test]
fn explicit_layout_field_wins() {
    // `layout:` in frontmatter overrides the directory-name default, and is
    // resolved by filename (so it need not match the component name inside).
    let root = scratch("explicit_layout");
    let proj = root.join("project");
    let out = root.join("out");
    write(
        &proj,
        "pages/index.nono",
        r#"component Home { main { "h" } }"#,
    );
    write(
        &proj,
        "layouts/fancy.nono",
        "component Fancy { section(class = \"fancy\") { Slot() } }",
    );
    write(&proj, "content/notes/n.md", "---\nlayout: fancy\n---\nhi\n");

    build(&BuildConfig {
        project: proj,
        out: out.clone(),
    })
    .expect("build failed");

    let page = fs::read_to_string(out.join("notes/n/index.html")).expect("page missing");
    assert!(page.contains(r#"<section class="fancy">"#), "got: {page}");
    assert!(page.contains("<p>hi</p>"), "got: {page}");
}

#[test]
fn missing_layout_is_a_loud_error() {
    let root = scratch("missing_layout");
    let proj = root.join("project");
    let out = root.join("out");
    write(
        &proj,
        "pages/index.nono",
        r#"component Home { main { "home" } }"#,
    );
    write(&proj, "content/posts/x.md", "---\ntitle: X\n---\nbody\n");

    let err = build(&BuildConfig { project: proj, out })
        .expect_err("build should fail with no layout")
        .to_string();
    assert!(
        err.contains("layout") && err.contains("posts"),
        "got: {err}"
    );
}

#[test]
fn bracket_indexing_reads_map_and_list() {
    // `post["title"]` reaches a map field by string key (needed for JSON keys
    // like "#text"), and `posts[0]` indexes a list. Tested through a real
    // markdown map so no network is involved.
    let root = scratch("indexing");
    let proj = root.join("project");
    let out = root.join("out");
    write(
        &proj,
        "content/posts/p.md",
        "---\ntitle: Hello\n---\nbody\n",
    );
    write(
        &proj,
        "layouts/posts.nono",
        "component L(title: string) { article { Slot() } }",
    );
    write(
        &proj,
        "pages/index.nono",
        r#"
        const posts = glob("content/posts/*.md")
        component Home {
          div {
            for post in posts { span { "{post["title"]}" } }
            p { "{posts[0]["title"]}" }
          }
        }
        "#,
    );

    build(&BuildConfig {
        project: proj,
        out: out.clone(),
    })
    .expect("build failed");

    let page = fs::read_to_string(out.join("index.html")).expect("index missing");
    assert!(
        page.contains("<span>Hello</span>"),
        "string key on map: {page}"
    );
    assert!(page.contains("<p>Hello</p>"), "list index then key: {page}");
}

#[test]
fn markdown_headings_drive_a_table_of_contents() {
    let root = scratch("toc");
    let proj = root.join("project");
    let out = root.join("out");
    write(
        &proj,
        "pages/index.nono",
        r#"component Home { main { "home" } }"#,
    );
    // The layout pulls the free `headings` list and hands it to the stdlib
    // TableOfContents component.
    write(
        &proj,
        "layouts/posts.nono",
        "component L(headings: list) { article { TableOfContents(headings = headings) Slot() } }",
    );
    write(
        &proj,
        "content/posts/guide.md",
        "---\ntitle: Guide\n---\n## First Section\nbody\n## First Section\nagain\n### Nested Bit\nmore\n",
    );

    build(&BuildConfig {
        project: proj,
        out: out.clone(),
    })
    .expect("build failed");

    let page = fs::read_to_string(out.join("posts/guide/index.html")).expect("post page missing");
    // Body headings carry slug ids.
    assert!(
        page.contains(r#"<h2 id="first-section">First Section</h2>"#),
        "got: {page}"
    );
    assert!(
        page.contains(r#"<h3 id="nested-bit">Nested Bit</h3>"#),
        "got: {page}"
    );
    // Duplicate heading text gets a unique id.
    assert!(
        page.contains(r#"<h2 id="first-section-1">First Section</h2>"#),
        "got: {page}"
    );
    // The TOC links to those same ids, with a depth class per level.
    assert!(page.contains(r#"<nav class="toc">"#), "got: {page}");
    assert!(
        page.contains(r##"<li class="toc-h2"><a href="#first-section">First Section</a></li>"##),
        "got: {page}"
    );
    assert!(
        page.contains(r##"<li class="toc-h2"><a href="#first-section-1">First Section</a></li>"##),
        "got: {page}"
    );
    assert!(
        page.contains(r##"<li class="toc-h3"><a href="#nested-bit">Nested Bit</a></li>"##),
        "got: {page}"
    );
}

#[test]
fn build_step_runs_with_path_tokens() {
    let root = scratch("build_steps");
    let proj = root.join("project");
    let out = root.join("out");
    write(
        &proj,
        "pages/index.nono",
        r#"component Home { main { "hi" } }"#,
    );
    // The step writes a stylesheet via the <publicDir> token (which resolves to
    // static/), proving steps run before the static copy and that the token is
    // substituted. nono links nothing itself, so we only check the file lands.
    write(
        &proj,
        "nono.toml",
        "[build]\nsteps = [\"mkdir -p <publicDir> && printf 'body{color:red}' > <publicDir>/generated.css\"]\n",
    );

    build(&BuildConfig {
        project: proj,
        out: out.clone(),
    })
    .expect("build failed");

    let css = fs::read_to_string(out.join("generated.css")).expect("generated css missing");
    assert!(css.contains("color:red"), "got: {css}");
}

#[test]
fn head_block_is_lifted_into_the_document_head() {
    let root = scratch("head_block");
    let proj = root.join("project");
    let out = root.join("out");
    // SiteHead emits a head{} block; the page uses it twice over, yet the link
    // must appear once and inside <head>, not the body. Proves hoists combine
    // across the component tree and land in the head.
    write(
        &proj,
        "lib/lib.nono",
        r#"component SiteHead { head { link(rel = "stylesheet", href = "/styles.css") } }"#,
    );
    write(
        &proj,
        "pages/index.nono",
        r#"component Home { SiteHead() main { "body text" SiteHead() } }"#,
    );

    build(&BuildConfig {
        project: proj,
        out: out.clone(),
    })
    .expect("build failed");

    let page = fs::read_to_string(out.join("index.html")).expect("index missing");
    let head_end = page.find("</head>").expect("no </head>");
    let body_start = page.find("<body>").expect("no <body>");
    let link = r#"<link rel="stylesheet" href="/styles.css" />"#;
    // The link sits in the head, before the body opens.
    let first = page.find(link).expect("link missing from document");
    assert!(first < head_end, "link should be in <head>: {page}");
    // Body carries the real content but no leaked link.
    assert!(page[body_start..].contains("body text"), "got: {page}");
    assert!(
        !page[body_start..].contains(link),
        "link leaked into body: {page}"
    );
}

#[test]
fn failing_build_step_aborts_the_build() {
    let root = scratch("build_step_fail");
    let proj = root.join("project");
    let out = root.join("out");
    write(
        &proj,
        "pages/index.nono",
        r#"component Home { main { "hi" } }"#,
    );
    write(&proj, "nono.toml", "[build]\nsteps = [\"exit 3\"]\n");

    let err = build(&BuildConfig { project: proj, out })
        .expect_err("a failing build step should fail the build")
        .to_string();
    assert!(err.contains("build step failed"), "got: {err}");
}

#[test]
fn page_with_two_components_is_rejected() {
    let root = scratch("two_components");
    let proj = root.join("project");
    let out = root.join("out");
    write(
        &proj,
        "pages/index.nono",
        r#"component A { div { "a" } } component B { div { "b" } }"#,
    );

    let err = build(&BuildConfig { project: proj, out })
        .expect_err("a page with two components should be rejected")
        .to_string();
    assert!(err.contains("exactly one component"), "got: {err}");
}
