# Origin MCP Tool Surface Design

**Date**: 2026-03-21
**Status**: Active — authoritative reference for all MCP surface decisions
**Applies to**: `origin-mcp` (the standalone MCP server crate)
**MCP spec version**: 2025-11-25

## Summary

Origin exposes exactly four MCP tools: **remember**, **recall**, **context**, **forget**. This document establishes why, how tool descriptions and annotations are written, and the decision framework for future changes.

---

## 1. Philosophy: What Origin Exposes vs Hides

### 1.1 The Four-Tool Surface

Reduced from 12 tools (one per REST endpoint) to 4 primary actions. The rationale:

**Agents are not power users.** The REST API serves the desktop UI and developers who understand Origin internals (entity resolution, knowledge graph traversal, confidence formulas). Agents perform worse when given this level of control — they call wrong tools, pass bad parameters, and duplicate work the backend does automatically.

**The backend is smarter than the agent.** Origin auto-classifies memory types via LLM, resolves entities and links to the graph on ingest, computes confidence from trust level and stability tier, schedules decay based on memory type, generates recaps, and reranks search results. None of these require agent participation.

**Types drive mechanical behavior, not agent decisions.** The five memory types (identity, preference, fact, decision, goal) each have stability tiers controlling confidence floors, decay rates, and supersede behavior. The agent can optionally hint at the type; the backend validates or auto-classifies. This is Origin's core differentiator versus competitors:

| System | Types | Agent controls types? |
|--------|-------|-----------------------|
| Mem0 | 3 conceptual | Partially |
| Zep | Graph-native | No (graph IS the model) |
| Letta | 4 storage tiers | Yes (explicit tier selection) |
| Origin | 5 mechanical types | No (auto-classified, hints accepted) |

### 1.2 Minimal Knobs, Maximum Automation

Each tool has exactly one required parameter. All others are optional hints the backend may override or ignore.

| Tool | Required | Optional (hints) |
|------|----------|-------------------|
| remember | content | memory_type, domain, entity, confidence, supersedes |
| recall | query | limit, memory_type, domain, entity |
| context | *(none)* | topic, limit |
| forget | memory_id | *(none)* |

---

## 2. Tool Naming

### 2.1 Verb Names from User Vocabulary

Tools use imperative verbs users naturally say: **remember**, **recall**, **forget**. Not `store_memory`, `search_memory`, `delete_memory`.

Why:
1. **Natural language alignment.** Users say "remember this" — the tool name IS the trigger word.
2. **Token efficiency.** Short names consume fewer tokens in tool lists and reduce selection ambiguity.
3. **Competitive differentiation.** Mem0 uses `add`/`search`/`delete`. Zep uses `add_memory`/`search_memory`. Letta uses `core_memory_append`. Origin's names sound like what a person would say.

### 2.2 Why "context" (a Noun)

`context` is a term of art in MCP (Model **Context** Protocol). It signals to agents and developers that this tool provides session context. It's the only noun in the set, which helps it stand out as the "start here" tool.

---

## 3. MCP vs REST-Only Decision Framework

### 3.1 The Litmus Test

An endpoint becomes an MCP tool **only if all four** are true:

1. An agent would use it **autonomously** (not just when a human asks for a specific admin operation)
2. The agent can provide **correct parameters** without knowing Origin internals
3. Getting it **wrong is safe** (or the tool has built-in guardrails)
4. It **cannot be automated** as backend behavior triggered by an existing tool

### 3.2 Current Decisions

| REST Endpoint | MCP? | Rationale |
|---------------|------|-----------|
| POST /api/memory/store | **remember** | Core write path |
| POST /api/memory/search | **recall** | Core read path |
| POST /api/chat-context | **context** | Session orientation |
| DELETE /api/memory/delete/{id} | **forget** | Explicit user request |
| GET /api/profile | No (open, see §7.1) | See open decisions |
| POST /api/memory/entities | No | Auto-linked on ingest |
| POST /api/memory/relations | No | Auto-linked on ingest |
| POST /api/memory/observations | No | Auto-linked on ingest |
| POST /api/memory/confirm/{id} | No | Human-in-the-loop via UI |
| POST /api/memory/reclassify/{id} | No | Admin correction via UI |
| GET /api/agents | No | UI dashboard data |
| POST /api/sweep | No | Runs automatically on schedule |
| GET /api/memory/stats | No | UI dashboard data |
| GET /api/health | No | HTTP /health on MCP server itself |

### 3.3 Adding a New MCP Tool — Ask First

- Can this be folded into an existing tool as an optional parameter?
- Can the backend do this automatically when an existing tool is called?
- Will the agent actually call this without being explicitly asked?

If the answer to either of the first two is yes, do not add a tool. If the answer to the third is no, do not add a tool.

---

## 4. Server Instructions

### 4.1 Role in MCP Protocol

Per the MCP spec (2025-11-25), `InitializeResult.instructions` is:

> "Instructions on how to use the server and its features, potentially aiding the LLM's understanding of available tools."

This is the behavioral contract: it tells the agent **WHEN** to use each tool, not **WHAT** each tool does (that's the description's job).

### 4.2 Current Instructions

```
Origin is a personal agent memory layer. Use it proactively — don't wait to be asked.

START EVERY SESSION with context — it returns the user's identity memories,
preferences, goals, and relevant memories so you know who you're talking to.

WHEN TO USE:
- Session start or topic shift → context (returns identity + preferences + relevant memories)
- User says 'remember/save/store/note/don't forget' → remember
- User asks 'do you remember/what did I say/recall' → recall
- You learn something important about the user → remember
- User says 'forget/delete/remove' → forget

CHOOSING recall vs context:
- context: broad orientation, session start, "who is this user?", "catch me up"
- recall: specific query ("what's Alice's role?", "database preferences")

Agent settings (trust level, enabled) are managed by the user via the Settings UI.
```

### 4.3 Instruction Design Principles

1. **Tell the agent WHEN, not HOW.** Instructions are a routing table, not a user manual.
2. **Proactive by default.** "Use it proactively" is the most important sentence.
3. **One entry point for session start.** `context` is unambiguously the first call.
4. **Clarify overlapping tools.** `recall` and `context` both return memories — the distinction must be crisp.
5. **Defer admin to UI.** Prevents agents from trying to change their own trust level.

---

## 5. Tool Description Patterns

### 5.1 Format

Per the MCP spec, tool descriptions are "hints to the model." Every description follows:

```
[One sentence: what it does].
Use when: [trigger phrases].
[One sentence: what backend auto-does].
```

Trigger-based descriptions work because LLMs match descriptions against conversation by keyword overlap. Listing "remember this", "save this", "don't forget" directly in the description creates high overlap when those phrases appear in user messages.

### 5.2 Current Descriptions

**remember:**
> Store a memory, fact, preference, or decision. Use when: 'remember this', 'save this', 'store this', 'note this', 'don't forget', 'keep track of', 'record this', or when you learn something important about the user. System auto-classifies type, detects entities, and links to knowledge graph.

**recall:**
> Search memories and knowledge. Use when: 'do you remember', 'what do you know about', 'search memory', 'find memories', 'look up', 'what did I say about', 'any notes on'. Returns memories, entity facts, and graph context in a single ranked list.

**context:**
> Load conversation-relevant context including identity memories, preferences, goals, corrections, and topic-relevant memories. Call this FIRST at session start. Also use on topic shifts or 'catch me up on', 'what's the background on'.

**forget:**
> Delete a memory. Use when: 'forget this', 'delete that', 'remove this', 'that's wrong'. Cleans up entity links.

### 5.3 Anti-Patterns

- **Parameter lists in descriptions.** JSON schema already describes parameters. Repeating them wastes tokens and creates drift.
- **Implementation details.** "Calls /api/memory/store with POST" means nothing to an agent.
- **Claiming capabilities that don't exist.** Don't say "returns profile" if it returns identity memories.

---

## 6. Annotation & Metadata Strategy

### 6.1 MCP Spec (2025-11-25) Tool Fields

The spec defines these fields on the `Tool` type:

| Field | Required | Our usage |
|-------|----------|-----------|
| `name` | Yes | `remember`, `recall`, `context`, `forget` (1-128 chars, `[A-Za-z0-9_-.]`) |
| `title` | No | Set on all tools — human-readable display name for UIs |
| `description` | No | Set on all tools — trigger-based hints for LLM |
| `inputSchema` | Yes | JSON Schema from schemars derive |
| `outputSchema` | No | Not used — tools return unstructured text content |
| `annotations` | No | Set on all tools — behavioral hints |
| `execution.taskSupport` | No | Not set (defaults to `"forbidden"`) — no async tasks needed |
| `icons` | No | Not set — optional, cosmetic |

### 6.2 Annotation Defaults

Per the spec, `ToolAnnotations` are "hints, not guaranteed to provide a faithful description of tool behavior." Key defaults:

| Annotation | Default if unset | Implication |
|-----------|-----------------|-------------|
| `read_only_hint` | `false` | Tools assumed to write by default |
| `destructive_hint` | **`true`** | Tools assumed destructive if not explicitly set `false` |
| `idempotent_hint` | `false` | Tools assumed non-idempotent |
| `open_world_hint` | **`true`** | Tools assumed to touch external world |

The spec's own example: *"the world of a web search tool is open, whereas that of a memory tool is [closed]."*

### 6.3 Origin's Annotations

| Tool | title | read_only | destructive | idempotent | open_world |
|------|-------|-----------|-------------|------------|------------|
| remember | "Remember" | false | **false** (explicit) | false | false |
| recall | "Recall" | true | *(n/a)* | *(n/a)* | false |
| context | "Context" | true | *(n/a)* | *(n/a)* | false |
| forget | "Forget" | false | true | true | false |

Critical: `remember` must explicitly set `destructive_hint = false`. If omitted, the spec default is `true`, and clients may require user confirmation for every store operation.

### 6.4 Future Considerations from 2025-11-25 Spec

- **`outputSchema`**: `recall` and `context` could benefit from structured output (`structuredContent` + `outputSchema`) for clients that want typed results. Deferred until a client needs it.
- **`execution.taskSupport`**: If any tool becomes long-running (e.g., context with large memory sets), we could opt into async task execution. Not needed today.
- **Tool execution errors**: The spec distinguishes protocol errors (JSON-RPC) from tool execution errors (`isError: true`). We should return `isError: true` for "memory not found" in forget, rather than success with a text message.

### 6.5 Transport-Level Enforcement

Annotations are hints for well-behaved agents. Transport gating is enforcement for untrusted ones:

- **HTTP mode:** `forget` blocked entirely (returns error message)
- **HTTP mode:** `source_agent` forcefully overridden with configured agent name
- **HTTP mode:** `user_id` locked to configured value

---

## 7. Open Decisions

### 7.1 Profile Data

**Status: Deferred.**

The `context` tool returns identity-type memories from the `chunks` table but NOT the structured profile record (name, display_name, email, bio, avatar_path) from the `profiles` table. These are different data sources:

- A user who onboarded with name "Lucian" has that in `/api/profile`
- If they have no identity-type memories, `context` returns nothing about who they are
- If they stored "My name is Lucian" as an identity memory, `context` returns that

**Options under consideration:**

| Option | Trade-off |
|--------|-----------|
| Fold profile into `/api/chat-context` response | Single call; requires backend change |
| Add `profile` as 5th MCP tool | Clean separation; increases surface |
| Auto-create identity memories from profile | Profile flows through existing pipeline; dual-write complexity |
| Leave it out | Zero work; agents may not know user's name on first session |

Decision will be made when we have data on whether agents need the profile record or whether identity memories are sufficient.

### 7.2 Future Tool Candidates

Only two under consideration:
1. **`profile`** (read-only) — see above
2. **`status`** (read-only) — memory count, agent list, system health. Low priority.

No additional write tools planned. Philosophy: one way in (remember), one way out (forget), two ways to read (recall for specific, context for broad).

### 7.3 Deprecation Strategy

For future tool removals:
1. Keep registered for 2 minor versions with helpful error directing to replacement
2. Remove from server instructions immediately (new sessions stop using it)
3. Remove tool registration after grace period

The 12→4 reduction was done in one commit because Origin had no public MCP consumers. Future removals must be graceful.
