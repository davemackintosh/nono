---
title: Building a site generator out of spite
date: 2026-06-25
kind: Essay
summary: What happens when a joke about not building your own framework gets taken to its logical, working conclusion.
---

The bit was simple: don't build your own framework. So I built my own framework.

## The premise

A static site generator that ships nothing to the browser. No JavaScript, no
runtime, no hydration, no accounts, no cloud. You write components in a small
SwiftUI-flavoured language and it emits plain HTML at build time. That's the
whole pitch, and it is both a joke and completely sincere.

## What it turned into

A real little compiler: parse, evaluate, emit. Along the way it grew components,
slots, optional props, a standard library, a table-of-contents generator, and a
build stage so I could pull in Tailwind. None of which I strictly needed.

### The part I'm smug about

The filesystem is the router and the markup reads almost like Elm. It should not
be this pleasant. That's the most annoying thing about it.

## Would I recommend this

No. Absolutely not. Do as I say, not as I did. But it works, and that was the
entire point.
