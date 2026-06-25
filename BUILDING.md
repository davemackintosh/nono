# Building Nono

## First build

```sh
cd path/to/nono
cargo build
cargo test          # tests/integration.rs + tests/build.rs
```

If `cargo` reports "command not found" despite rustup being installed, your
`~/.cargo/bin` shims may be pointing at nothing. `rustup which cargo` prints the
real binary; put that toolchain's `bin/` on your PATH and carry on.

Then build the example blog:

```sh
cargo run -- build examples/blog --out /tmp/blog-out
# open /tmp/blog-out/index.html
```

The Last.fm const needs a key, or it will error:

```sh
export NONO_LASTFM_KEY=your_key_here
cargo run -- build examples/blog --out /tmp/blog-out
```

To build without a key while poking at the rest, swap the
`const recent_tracks = lastfm_recent(...)` line in
`examples/blog/pages/index.nono` for an empty list, e.g.
`const recent_tracks = glob("no/such/dir/*.md")`. Everything else builds fine,
the "Listening to" sidebar just comes out empty.

`lastfm_recent` is not a builtin: it is a standard-library function in
`src/std.nono` (compiled into the binary, loaded into every project), written on
the generic builtins `http_get` and `env`. Define your own with
`fn name(p: type) = expr`, and reach awkward JSON keys with bracket indexing,
`track.artist["#text"]`.

## Live preview

```sh
cargo run -- dev examples/torture          # http://127.0.0.1:6969
cargo run -- dev examples/blog --port 8080 # if 6969 offends you
```

`nono dev` builds into a scratch dir and serves it with a tiny std-only HTTP
server. It rebuilds on every request, so editing a `.nono` or `.md` file and
refreshing shows the change. If a build fails, the error renders in the browser
(and prints to the terminal) rather than killing the server. Vanity URLs work as
you'd expect: `/posts/x` and `/posts/x/` both resolve to that page. Note the blog
example still wants `NONO_LASTFM_KEY` (or the empty-glob swap) to build at all.

## What the first compile turned up

It has now been compiled. `cargo build` is clean and `cargo test` is green. Since
the scaffolding was written without a toolchain to check it, here is what only
rustc and the tests could catch:

- The grammar tried `field_access` before the `nil` / `true` / `false` literals,
  and since those are all valid identifiers they got swallowed as one-element
  paths and then died at eval as "unknown name `nil`". Reordered, with a word
  boundary so `nil` stops eating the front of `nilable`.
- `match` on a string did nothing. `Value::tag()` knew how to tag a map (via its
  `kind` field) and a `Tagged` value, but not a bare string, so
  `match post.kind { Essay => ... }` matched no arm and rendered silence. A string
  is now its own tag. The test meant to cover this was iterating empty data, so it
  had been passing on a technicality.

The borrow checker, in the end, had no notes.

## Pages, layouts, content

Routing has two halves, and they own their markup in opposite directions.

- `pages/*.nono`: each file is **exactly one component**, rendered and routed:
  `pages/about.nono` becomes `about/index.html`, `pages/index.nono` stays
  `index.html`. More than one component in a page file is a hard error (the
  message does not mince words). Helpers belong in `lib/`.
- `content/**/*.md`: each markdown file is **also** a page, mirrored to a vanity
  URL: `content/posts/x.md` becomes `posts/x/index.html`. It is wrapped in a
  layout chosen by, in order: a `layout:` frontmatter field (resolved by
  filename), else the parent directory name verbatim, else `layouts/default.nono`,
  else a loud error. Frontmatter fills the layout's parameters; the rendered body
  fills its `Slot()` (which may be handed down through another component, so a
  layout can reuse your `Post` and write `Post(...) { Slot() }`). `draft: true`
  keeps a file out of the build: no page, and excluded from `glob` listings too.
- `lib/*.nono`: shared components and the stylesheet, loaded into every page. No
  one-component rule here: a lib file can hold as many components as you fancy.
- `layouts/*.nono`: one component each, same as pages, but selected by markdown
  files rather than routed directly.
- `static/`: copied verbatim.

Path segments (page filenames, content filenames, directories) must match
`[a-z0-9-]`. Anything else is rejected up front, which also dodges the
macOS-case-insensitive vs Linux-case-sensitive deploy footgun.

## The torture example

`examples/torture/` exercises the edge cases deliberately: nested `for`,
`if/else`, string escapes, adjacent interpolation, `match` with a wildcard,
arithmetic in interpolation, an unfilled optional slot, and (now) a markdown
file turning into its own page via `layouts/posts.nono`. Its helper components
live in `lib/`, because the page file gets exactly one.

```sh
cargo run -- build examples/torture --out /tmp/torture-out
# index.html, plus posts/one/index.html and posts/two/index.html
```

## The failure cases

`examples/should-fail/` contains three projects that MUST fail to build, each
testing one error path:

- `case-bad-filename/`: `About_Me.nono` violates `[a-z0-9-]`.
- `case-unknown-element/`: references an undefined component `Frobnicate`.
- `case-bad-glob/`: a glob with two `*`s.

Each should exit non-zero with a clear message. That is how you check the "error
loudly" promise holds:

```sh
cargo run -- build examples/should-fail/case-bad-filename --out /tmp/x ; echo "exit: $?"
```

Two more loud failures are covered by `tests/build.rs` rather than fixture
projects: a content page whose layout doesn't exist, and a page file with two
components in it.
