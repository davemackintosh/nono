# Nono

**No, no, I'll do it myself.**

A spite-driven static site generator. I asked two friends for advice on stable,
popular frameworks for rebuilding my blog. They joked I'd go and build my own
instead. Here we are.

Nono compiles a directory of `.nono` files and Markdown content into static
HTML. It ships **nothing** to the browser: no runtime, no JavaScript, no
hydration, no accounts, no cloud. Every input is resolved at build time. The
browser gets documents.

## The language in one screen

```nono
// lib/components.nono — shared components + stylesheet
stylesheet {
  Prose {
    max-width = 65ch
    line-height = 1.6
  }
}

component Post(title: string, date: string) {
  article {
    h1 { "{title}" }
    time(datetime = date) { "{date}" }
    Slot()                       // the default (unnamed) slot
  }
}

component Layout(heading: string) {
  main  { h1 { "{heading}" } Slot() }
  aside { Slot(named = "sidebar", or = nil) }   // optional named slot
}
```

```nono
// pages/index.nono — a page is a component; the filesystem is the router
const recent_tracks = lastfm_recent(user = "davemackintosh", limit = 10)
const posts         = glob("content/posts/*.md")

component IndexPage {
  Layout(heading = "New World Code") {
    sidebar = {                  // fill a named slot, inline
      h2 { "Listening to" }
      for track in recent_tracks {
        Prose { "{track.artist["#text"]} - {track.name}" }
      }
    }

    for post in posts {          // the default slot: everything not named
      match post.kind {
        Note  => Prose { "{post.title}" }
        Essay => Post(title = post.title, date = post.date) {
          Prose { "{post.title}" }
        }
      }
    }
  }
}
```

## Design decisions

- **`=` binds, `:` types.** `title = "x"` is an argument; `title: string` is a
  type. They never collide.
- **Slots are holes; fills are arguments.** `Slot()` / `Slot(named = "x")` mark
  holes in a component. `name = { ... }` inside an invocation fills a named one;
  the unnamed trailing content fills `Slot()`. `or = nil` makes a slot optional.
- **`const` is bind-time.** Data sources run during evaluation and fold into the
  tree. The builtins are deliberately generic: `glob`, `markdown`, `http_get`,
  `env`. There is no `var` yet, because nothing at build time legitimately
  mutates. It arrives when something needs it.
- **Functions are userland, and so is the standard library.** `fn name(p: type)
  = expr` defines a value-returning function, distinct from a component (which
  returns markup). The core ships no service-specific builtin: `lastfm_recent`
  is a `fn` in the standard library (`src/std.nono`), built on `http_get` and
  `env`. Bracket indexing, `track.artist["#text"]`, reaches JSON keys that aren't
  valid identifiers.
- **The filesystem is the router, and it routes two things.** A `.nono` under
  `pages/` is a page that happens to be a component: `pages/about.nono` →
  `about/index.html` (vanity URLs), `pages/index.nono` → `index.html`. A `.md`
  under `content/` is a page that happens to be a document:
  `content/posts/leaving-react.md` → `posts/leaving-react/index.html`, wrapped in
  a layout. Either way, path segments must match `[a-z0-9-]` or it's a loud build
  error (which also saves you from the macOS-case-insensitive vs
  Linux-case-sensitive deploy footgun).
- **A page is exactly one component.** That's a `pages/`-and-`layouts/` rule, not
  a language one: a file under `lib/` may define as many components as it likes.
  Put two components in a page file and the build will tell you, in fairly direct
  terms, what it makes of you.
- **Both file types are load-bearing; everything else is scenery.** `static/` is
  copied verbatim, the rest is ignored.

## Layout

```
your-site/
  pages/        .nono files, one component each, routed to HTML
  layouts/      .nono files, one component each, wrap markdown pages
  lib/          shared components + stylesheet (loaded into every page)
  content/      .md files, each routed to HTML in its own right
  static/       copied verbatim into the output
```

### Two kinds of page

A `.nono` page is a component you write by hand. A `.md` page is a document that
gets wrapped in a layout. Same destination (an HTML file at a route), opposite
ownership: the `.nono` owns its markup, the `.md` owns its content and borrows
markup from a layout.

A markdown file picks its layout like so:

1. a `layout:` field in the frontmatter, resolved by filename, so `layout: post`
   means `layouts/post.nono`;
2. failing that, the name of its parent directory, verbatim, so anything under
   `content/posts/` defaults to `layouts/posts.nono`;
3. failing that, `layouts/default.nono`;
4. failing all of that, a build error that names exactly what it went looking for.

The layout is an ordinary component. Its parameters are filled from the
frontmatter, and the rendered markdown body drops into its `Slot()`:

```nono
// layouts/posts.nono: one component, selected by the file's location
component PostLayout(title: string, date: string) {
  main {
    article {
      h1 { "{title}" }
      time(datetime = date) { "{date}" }
      Slot()                 // the markdown body lands here
    }
  }
}
```

```
content/posts/leaving-react.md   ->   posts/leaving-react/index.html

---
title: How to leave React
date: 2026-06-20
---
After eleven years, I am tired. Here is what I did instead.
```

That body `Slot()` can sit in plain HTML, as above, or be handed down through
another component: a layout that reuses your `Post` component as
`Post(...) { Slot() }` works fine, the body travels through. And `draft: true` in
the frontmatter keeps a file out of the build entirely: no page, and it won't
show up in `glob` listings either, so half-finished thoughts can sit in
`content/` without leaking.

## Usage

```sh
cargo build --release
./target/release/nono new   --path path/to/new-site  # scaffold from the blog template
./target/release/nono build path/to/your-site --out path/to/output
./target/release/nono dev   path/to/your-site   # serve on http://127.0.0.1:6969
./target/release/nono parse path/to/file.nono   # dump the AST (debugging)
```

`nono new` copies the blog template (the same thing that lives under
`examples/blog`, baked into the binary) into a fresh directory. It refuses to
write into anything that already has files in it, so you can't point it at your
real site and lose everything.

`nono dev` rebuilds the whole site on every request, so you edit a file, hit
refresh, and see it. A build error renders in the browser instead of taking the
server down. The port is 6969 because of course it is; pass `--port` if you have
grown up.

The Last.fm source reads its API key from `NONO_LASTFM_KEY`. If it's unset or
the call fails, the source errors; pair it with `or =` on the slot, or a weekly
rebuild cadence, so a flaky API never takes down the build.

## Editor support

There's a tree-sitter grammar in [`tree-sitter-nono/`](tree-sitter-nono/) for
syntax highlighting and structural editing of `.nono` files. It tracks the real
grammar and is checked against every example here.

## Status

A toy. It exists to annoy two specific people and to be the easiest possible
backend for a compiler pipeline I'm building elsewhere. It works; it is not
trying to be your framework. No, no.
