---
type: concept
title: Copied Page
description: An ordinary concept page living under an exported subtree that Wenlan's own sync should prune.
---

# Copied Page

This page simulates a concept that Wenlan itself exported back into the bundle
under an `exported/` subtree. It carries ordinary OKF frontmatter and a few
sentences of prose so it looks like any other concept file. The import sync is
expected to recognize and prune this subtree rather than re-ingesting Wenlan's
own output.
