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
    write(&proj, "pages/index.nono", r#"component Home { main { "home" } }"#);
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

    build(&BuildConfig { project: proj, out: out.clone() }).expect("build failed");

    // Vanity URL: content/posts/hello.md -> posts/hello/index.html.
    let page = fs::read_to_string(out.join("posts/hello/index.html")).expect("post page missing");
    // Title from frontmatter, layout h1 from the param, body as raw HTML.
    assert!(page.contains("<title>Hello There</title>"), "got: {page}");
    assert!(page.contains("<h1>Hello There</h1>"), "got: {page}");
    assert!(page.contains("Body <em>text</em>"), "got: {page}");
}

#[test]
fn draft_md_is_not_published() {
    let root = scratch("draft");
    let proj = root.join("project");
    let out = root.join("out");
    write(&proj, "pages/index.nono", r#"component Home { main { "home" } }"#);
    write(&proj, "layouts/posts.nono", "component L { article { Slot() } }");
    write(
        &proj,
        "content/posts/secret.md",
        "---\ntitle: Secret\ndraft: true\n---\nshh\n",
    );

    build(&BuildConfig { project: proj, out: out.clone() }).expect("build failed");

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
    write(&proj, "pages/index.nono", r#"component Home { main { "h" } }"#);
    write(
        &proj,
        "layouts/fancy.nono",
        "component Fancy { section(class = \"fancy\") { Slot() } }",
    );
    write(&proj, "content/notes/n.md", "---\nlayout: fancy\n---\nhi\n");

    build(&BuildConfig { project: proj, out: out.clone() }).expect("build failed");

    let page = fs::read_to_string(out.join("notes/n/index.html")).expect("page missing");
    assert!(page.contains(r#"<section class="fancy">"#), "got: {page}");
    assert!(page.contains("<p>hi</p>"), "got: {page}");
}

#[test]
fn missing_layout_is_a_loud_error() {
    let root = scratch("missing_layout");
    let proj = root.join("project");
    let out = root.join("out");
    write(&proj, "pages/index.nono", r#"component Home { main { "home" } }"#);
    write(&proj, "content/posts/x.md", "---\ntitle: X\n---\nbody\n");

    let err = build(&BuildConfig { project: proj, out })
        .expect_err("build should fail with no layout")
        .to_string();
    assert!(err.contains("layout") && err.contains("posts"), "got: {err}");
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
