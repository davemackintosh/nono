# Nono — handover for Claude Code

You're picking up **Nono**, a spite-driven static site generator written in Rust.
It compiles a directory of `.nono` files (a small SwiftUI/Compose-flavoured DSL)
plus Markdown content into static HTML. It ships nothing to the browser: no JS,
no runtime, no hydration. Everything resolves at build time.

The project was scaffolded in an environment **without a Rust toolchain**, so the
grammar shape is verified (an equivalent PEG parses every example) but the Rust
itself has **never been compiled**. Your first job is to make `cargo build` and
`cargo test` pass, then confirm the examples build to correct HTML.

## Update, 2026-06-24: session one is done

It compiles. `cargo build` is clean, `cargo test` is green (13 evaluator tests +
5 build-driver tests), and all three `should-fail/` cases exit non-zero with
clear messages. The borrow checker had no notes after all; the two real bugs were
elsewhere:

- The grammar matched `field_access` before the `nil`/`true`/`false` literals, so
  those literals were swallowed as bare paths and failed at eval. Reordered, with
  a word boundary added.
- `match` on a string matched nothing, because `Value::tag()` didn't treat a
  string as its own tag. The blog's `<main>` was coming out silently empty. Fixed,
  and the weak test (it iterated empty data) is now backed by one that doesn't.

Then we took the design somewhere new, on purpose:

- **A page is exactly one component**, enforced for `pages/` and `layouts/` with a
  rude error. Torture's inline helpers moved to `lib/`.
- **Every `.md` is now a target HTML file**, wrapped in a layout. New `layouts/`
  special path; layout resolved by frontmatter `layout:`, else parent-dir name
  verbatim, else `layouts/default.nono`, else a loud error. Drafts are skipped,
  per-page titles come from frontmatter. See README and BUILDING for the full
  shape.

Known follow-ups left on the table: the nested-slot limitation (a layout can't
route the body through another component's slot), and whether drafts should also
drop out of index *listings* and not just output.

## First actions, in order

1. `cargo build` — fix whatever rustc surfaces. Expect local, mechanical issues,
   most likely borrow-checker nits in `src/eval.rs` (around `eval_nodes` threading
   its `scope`, or `expand_component`'s arg binding). Do NOT redesign anything to
   satisfy the borrow checker without flagging it; clone where needed, this is a
   build-time tool and performance is a non-issue at this scale.
2. `cargo test` — `tests/integration.rs` parses and evaluates small programs and
   asserts on HTML output. Make them green. If a test encodes wrong behaviour,
   tell me before changing the assertion.
3. Build the examples and eyeball the output:
   ```
   cargo run -- build examples/blog --out /tmp/blog-out      # needs NONO_LASTFM_KEY, or comment out the lastfm const + its loop
   cargo run -- build examples/torture --out /tmp/torture-out
   ```
4. Verify the failure cases actually fail loudly (non-zero exit, clear message):
   ```
   for d in examples/should-fail/*/; do cargo run -- build "$d" --out /tmp/x; echo "$d exit: $?"; done
   ```

## Architecture (read before touching anything)

Pipeline: **parse → evaluate → emit HTML**. Same shape as a real compiler with the
easiest possible backend (string concatenation). Files:

- `src/nono.pest` — the entire grammar (pest PEG). One screen.
- `src/ast.rs` — AST types, mirror the grammar.
- `src/parser.rs` — pest parse tree → AST.
- `src/value.rs` — runtime `Value` (build-time values: Str/Number/Bool/Nil/List/Map/Tagged).
- `src/sources.rs` — build-time data sources: `glob`, `markdown`, `lastfm.recent`.
  These run synchronously during eval (Go-style, no async). Last.fm key from
  `NONO_LASTFM_KEY`.
- `src/eval.rs` — the engine. Folds `for`/`if`/`match`, expands components, fills
  slots, resolves expressions + interpolation. The interesting file.
- `src/html.rs` — HTML node tree + serialiser + document wrapper + escaping.
- `src/build.rs` — filesystem router and build driver.
- `src/main.rs` / `src/lib.rs` — CLI (clap) and library root.
- `check_grammar.py` — a parsimonious PEG that approximates the grammar; run it
  if you change `nono.pest` to sanity-check example files still parse. NOT a
  substitute for the real parser, just a fast smoke test.

## Language design — these are settled, do not relitigate

- **`=` binds, `:` types.** `title = "x"` is an argument; `title: string` is a type.
- **Slots are holes, fills are arguments.** `Slot()` / `Slot(named = "x", or = nil)`
  mark holes in a component definition. `name = { ... }` inside an invocation block
  fills a named slot (this is the `NamedFill` node, extracted during
  `expand_component`). The unnamed trailing content fills `Slot()`. `or = nil`
  makes a slot optional / render-nothing-if-unfilled.
- **`const` is bind-time, block-local allowed.** There is intentionally NO `var`
  yet — don't add it until a real build-time mutation need appears. If you think
  you need it, flag it first.
- **Slot fills capture the caller's environment** (`SlotFills.capture`) so a fill
  referencing an enclosing `for` binding resolves correctly. Don't "simplify" this
  away.
- **Filesystem is the router.** `pages/about.nono` → `about/index.html` (vanity
  URLs); `pages/index.nono` → `index.html`. Page path segments must match
  `[a-z0-9-]` or it's a hard error (this also dodges the macOS-case-insensitive /
  Linux-case-sensitive deploy footgun).
- **`.nono` routes, `.md` is content, everything else is scenery.**
- **Attribute names allow hyphens** (`data-level`, `aria-label`) via the
  `attr_name` rule; plain `ident` (variables, params, component names) does not,
  so `a-b` stays subtraction. Keep that split.

## Known rough edges (deliberate — confirm before "fixing")

- **No operator precedence.** Expressions are left-to-right, so `1 + 2 * 3` == 9,
  not 7. A test documents this. If you add precedence (Pratt/climbing), make it a
  deliberate, announced change — for a templating language it may not even matter.
- `has_component` in `eval.rs` is unused public API — harmless, leave or wire it
  into a nicer "unknown component" error if you like.
- Markdown body is emitted as `Html::Raw` (unescaped) by design; frontmatter
  fields are escaped as normal text.

## Things that would genuinely improve it (only after green build + tests)

In rough priority order, but check with me before starting any of these:

1. A `nono watch` subcommand (rebuild on file change) — but keep full-rebuild
   semantics; this is a weekly-cadence toy, no incremental compilation needed.
2. Per-page `title` from the page component or frontmatter, instead of the current
   `derive_title` stub that just uses the component name.
3. A `markdown(file = ...)` node used directly in markup (the function exists in
   sources.rs; wire a clean node-level usage + test).
4. Better parse errors (pest spans → friendly messages with the source line).
5. List/map literals in the language so tests don't need `glob` of a missing dir
   to get an empty list.

## House style / preferences (mine)

- Comments preserve the **"why"**, not the "what". Keep the explanatory comments
  already in the source; match that voice.
- No em-dashes in any prose you write into the repo (README, comments, commit
  messages). I mean it.
- Conversational, British register, specific over abstract. Mild self-effacement
  fine. Don't write marketing voice.
- When you hit a real fork (a design decision, not a mechanical fix), STOP and ask
  rather than picking for me. Surface the tradeoff plainly.
- This is a toy that exists partly to wind up two friends. The README leans into
  that. Don't sand the personality off it.

## Definition of done for this first session

`cargo build` clean, `cargo test` green, `examples/blog` and `examples/torture`
produce sensible HTML, and all three `examples/should-fail/` cases exit non-zero
with clear messages. Report back what rustc made you change versus what was
already correct.

**Met** (see the 2026-06-24 update above), and then some.
