---
type: concept
title: Link Cases
description: Test fixture exercising every normalize_link_target rule.
verified:
  by: openwiki/0.4.3
  at: 2026-09-16T00:00:00Z
sources:
  - resource: repo://src/server.ts#L40-L82
  - resource: ../workflows/wiki-finalization.md
---

# Link Cases

This concept page exists purely to exercise every rule in `normalize_link_target`.
It links to real sibling pages, a broken page, a case-mismatched path, an
external URL, and a path that climbs above the bundle root. It also holds two
links that must never be counted because they sit inside a code span and a
fenced code block.

See the [finalization workflow](../workflows/wiki-finalization.md#heading) for
background, and the [OKF output page](/concepts/okf-output.md) for the
frontmatter fields. A [broken link](./does-not-exist.md) points nowhere, and a
[case-mismatched link](../Workflows/Wiki-Finalization.md) resolves to a
different id than the correctly-cased link above. An
[external link](https://example.com/x.md) is never a concept id.

A link in inline code, like `[not a link](../workflows/wiki-finalization.md)`,
must not be extracted. Neither should a link inside a fenced block:

```
[also not a link](../workflows/wiki-finalization.md)
```

Finally, a link that [escapes the bundle root](../../outside.md) resolves to
nothing.
