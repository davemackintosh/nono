---
title: In praise of shipping nothing
date: 2026-06-26
kind: Essay
summary: A blog is just documents. Here is the case for sending the browser exactly that and not one byte more.
---

Somewhere along the way we decided a page of text needed a megabyte of runtime to
render. I would like to gently disagree.

## What the browser actually wants

HTML. It is extraordinarily good at HTML. It has been rendering documents since
before most of our dependencies were born, and it asks for nothing in return.

## The cost of everything else

Every kilobyte of JavaScript is a kilobyte someone downloads, parses, and runs on
a phone on a train in a tunnel. For a blog. For *documents*.

### The compromise I made

The only build-time dynamism here is fetching data and compiling CSS. The output
is still just files. You could read them in a text editor and miss nothing.

## The result on this site

You are looking at it. View source if you like; there is nothing hiding.
