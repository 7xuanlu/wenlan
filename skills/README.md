# Origin Skills

Claude Code workflow skills installed by the Origin plugin.

These skills keep the daily interface short:

```text
/init
/brief
/capture remember this decision...
/recall database preferences
/distill
/review
/forget mem_...
/handoff
```

The skills do not store data themselves. They guide Claude Code to use the local `origin-mcp` tools, which talk to the Origin daemon on `127.0.0.1:7878`.

## Files

| Skill | Purpose |
| --- | --- |
| `init` | Set up Origin and verify the local daemon/MCP path. |
| `brief` | Load working context at session start or topic shifts. |
| `capture` | Save one durable memory: decision, lesson, gotcha, preference, fact, or correction. |
| `recall` | Query Origin for focused context. |
| `distill` | Refresh wiki pages from accumulated memories. |
| `review` | Inspect pending memories before confirmation. |
| `forget` | Delete a memory by ID. |
| `handoff` | End-session capture for decisions, lessons, gotchas, and open threads. |

Plugin metadata lives in [`.claude-plugin`](../.claude-plugin/README.md).
