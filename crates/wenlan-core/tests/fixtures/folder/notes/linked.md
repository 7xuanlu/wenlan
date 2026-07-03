---
tags:
  - project
  - ingest
title: Linked Note
---

# Linked Note

This note lives in a subdirectory so the recursive directory walk has to reach
it. It points at [[Report]] and [[Antikythera Mechanism]] using Obsidian-style
wikilinks, which the shared markdown path parses additively without forking.

The unique marker word Zorblatt appears exactly once in this note so a search
can prove the markdown file's chunks were embedded and are retrievable after
folder ingestion. Frontmatter, inline #tags, and [[wikilinks]] all survive the
plain-markdown path because absent structures parse to nothing and present ones
are additive.
