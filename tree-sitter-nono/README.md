# tree-sitter-nono

A [tree-sitter](https://tree-sitter.github.io/tree-sitter/) grammar for Nono, the
DSL that the spite-driven static site generator in the parent directory compiles.
It exists so editors can highlight, fold, and navigate `.nono` files without
anyone hand-rolling a regex highlighter at midnight.

It mirrors the real grammar in `../src/nono.pest`, and is checked against every
`.nono` file in `../examples`.

## What it parses

Everything the language has: components and parameters, `pages/` and `layouts/`
markup, slots (`Slot()`, named fills), control flow (`for` / `if` / `else` /
`match`), data-source calls (`glob`, `markdown`, `lastfm.recent`), `const`
bindings, the `stylesheet { ... }` block, string interpolation, and escapes.

## Build and test

```sh
npm install            # pulls in tree-sitter-cli
npm run generate       # grammar.js -> src/parser.c
npm test               # runs the corpus in test/corpus/

# parse a file (needs this dir on tree-sitter's parser-directories config)
npx tree-sitter parse ../examples/torture/pages/index.nono
```

The generated parser under `src/` is committed, so you can consume the grammar
without running the CLI at all.

## Two things worth knowing

The language has **no operator precedence**: expressions fold left to right, so
`1 + 2 * 3` is `9`, not `7`. The grammar reflects that with a flat,
left-associative `binary_expression`. This is deliberate in Nono, not a bug here.

Keywords are **reserved**: `component`, `const`, `for`, `in`, `if`, `else`,
`match`, `stylesheet`, `Slot`, `true`, `false`, `nil`. The pest grammar is
slightly looser (it will, in a couple of positions, let you name something
`for`), but nobody does that, and reserving them is what lets `Slot()` be a slot
rather than an element invocation. If you somehow have a component called `Slot`,
this is the one place the two grammars disagree, and also you should not.

## Editor setup

`queries/highlights.scm` provides syntax highlighting. For Neovim with
nvim-treesitter, point a custom parser config at this directory and drop the
query in your runtime path; for Helix, add a `[[language]]` entry with
`name = "nono"` and `grammar = "nono"`. The `tree-sitter.json` here declares the
`source.nono` scope and the `.nono` file type.
