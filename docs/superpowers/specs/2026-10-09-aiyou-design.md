# aiyou — Design

**Date:** 2026-10-09
**Status:** Approved (user delegated decisions to recommended options)

## Purpose

A single Rust CLI, `aiyou`, that:

1. **Harvests** chat/session data from local coding-agent harnesses (Claude Code, OpenCode, Codex, Charm Crush, Gemini CLI, Pi) into one normalized, git-versioned store on disk. Idempotent and incremental, safe to run from cron.
2. **Imports** progressive dumps from hosted chat products (ChatGPT, Claude.ai, Gemini/Takeout), merging newer dumps over older ones without duplication.
3. **Profiles** the user by running a map-reduce LLM pipeline ("swarm") over all stored chats to extract personality, preferences, workflow and traits, and generates a portable `SKILL.md` the user can install into any agent harness. Re-runs are incremental as data evolves.

Everything is local-first: the store is a plain git repo; LLM calls happen only during `aiyou profile`.

## Non-goals (v1)

- No daemon, no server, no TUI.
- No remote sync/push (user can add a git remote themselves; `aiyou` never pushes).
- No back-mutation of harness data — read-only against sources.
- No embedding/vector search.

## Store layout

Default root: `~/.aiyou` (configurable via `AIYOU_HOME` or `--root`). It is a git repo.

```
~/.aiyou/
  aiyou.toml              # config (sources on/off, redaction, profile settings)
  chats/
    <source>/<project-slug>/
      <session-id>.jsonl   # canonical normalized form, one message per line
      <session-id>.md      # rendered markdown (for humans + LLM input)
  imports/
    manifest.json          # dump archive: file hash, import date, counts, per source
  profile/
    traits.json            # canonical machine-readable trait profile (merged)
    SKILL.md               # generated skill for agent harnesses
    runs/<ts>.json         # audit log of each profile run (inputs, outputs, cost)
```

`<source>` ∈ `claude-code | opencode | codex | crush | gemini-cli | pi | chatgpt | claude-ai | gemini-import`.

`<project-slug>` = slugified project path/title from the source (for imports: `default` or slugified conversation grouping).

## Canonical data model

```rust
struct SessionMeta { source, id, project, title, created_ms, updated_ms }
struct Msg { id, role (user|assistant|system|tool), ts_ms, text, /* tool name/summary optional */ }
```

- `.jsonl` line 1 is a header record `{"aiyou":"session", meta...}`; subsequent lines are messages, sorted by ts then id. Stable ordering ⇒ stable diffs.
- `.md` rendering: `# title`, source/date header, then `**user**:`/`**assistant**:` blocks. Tool events rendered as fenced summaries.
- Redaction (config, default on for `.md` only): mask obvious secrets (api keys, tokens) via regex set before writing. JSONL keeps originals (local-only store) unless `redact_all = true`.

## Commands

```
aiyou init                       # create store, config, git init, first commit
aiyou sync [--source S ...]      # harvest all/selected sources; commit changes
aiyou import <chatgpt|claude-ai|gemini> <path> [--source-tag T]
aiyou profile [--if-changed] [--model M] [--workers N]
aiyou profile install [--target claude|opencode|crush|codex|agents-dir <path>]
aiyou status                     # per-source counts, last sync, last profile, git status
aiyou cron --print               # print crontab lines (sync daily, profile weekly if changed)
```

## Subsystem 1: harvest (sync)

Adapter trait:

```rust
trait Adapter {
    fn name(&self) -> &'static str;
    fn discover(&self) -> anyhow::Result<Vec<DiscoveredSession>>; // paths/db rows
    fn read(&self, d: &DiscoveredSession) -> anyhow::Result<(SessionMeta, Vec<Msg>)>;
}
```

Discovery returns `(key, fingerprint)` where fingerprint = mtime+size (files) or row `time_updated`/count (db). The sync engine compares against existing store files; only re-reads/re-writes changed sessions. Deleted-from-source sessions are kept (store is append-mostly archive) unless `--prune`.

Adapters:

| Source | Where | Format notes |
|---|---|---|
| claude-code | `~/.claude/projects/<proj>/*.jsonl` | lines: `{type: user\|assistant, message:{role,content}, uuid, timestamp, isSidechain}`; skip `summary`/meta lines; sidechains → optional include (default on) |
| opencode | `~/.local/share/opencode/opencode.db` (SQLite), fallback `~/.local/share/opencode/storage/` | join `session`, `message`, `part`; parts carry text/tool payloads |
| codex | `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` | known format; skip-gracefully if absent |
| crush | scan `~/.local/share/crush/**` and `~/.config/crush/**` for session JSON/JSONL | skip-gracefully if absent |
| gemini-cli | `~/.gemini/tmp/<hash>/chats/*` + `~/.gemini/history/**.jsonl` | JSONL `{type: user\|model, ...}` |
| pi | scan `~/.pi/**` for session JSONL/JSON | skip-gracefully if absent |

All adapters: read-only; missing source = warning + skip, never error the whole sync. Corrupt file = warn + skip.

Git: after each sync, `git add -A && git commit -m "sync: <source> +A ~B"` if dirty. Shell out to `git` (simpler than libgit2). If git absent, still write files, warn once.

## Subsystem 2: import (progressive dumps)

`aiyou import chatgpt ~/Downloads/chatgpt-export.zip|dir`

- ChatGPT: `conversations.json` — array of conversations, `mapping` tree of messages; linearize tree by creation order.
- Claude.ai: `conversations.json` — array with `chat_messages[]` (or newer per-file layout; handle both).
- Gemini: Takeout JSON (MyActivity/MyChat) — best-effort; HTML fallback not in v1.

Progressive merge rules:

- Conversation identity = (source, external_id) if present else (source, slug(title) + hash(first user msg)).
- Message identity = external id else sha256(role+text+ts-truncated).
- Re-import with newer dump: upsert conversation file; new messages append; edited/deleted messages — newest dump wins (upsert by id). `imports/manifest.json` records each dump (sha256, path, mtime, counts) so identical re-import is a no-op and history is auditable.
- Zip inputs are extracted to a temp dir; raw dumps are **not** archived into the store (size).

## Subsystem 3: profile (swarm)

Config (`[profile]` in aiyou.toml):

```toml
[profile]
provider_base_url = "https://api.openai.com/v1"   # any OpenAI-compatible endpoint
api_key_env       = "OPENAI_API_KEY"               # or ANTHROPIC_API_KEY with anthropic base
model             = "gpt-5.2"                      # any
workers           = 4
map_context_chars = 120_000                        # target chars per map batch
```

Pipeline (`aiyou profile`):

1. **Select** all `.md` chat files; compare mtime vs last run (`profile/traits.json.updated_at`) → full run if none.
2. **Map (swarm):** shard new/changed chats into batches ≈ `map_context_chars`; `workers` concurrent LLM calls. Each returns strict JSON: `[{trait, category, evidence, confidence}]`, categories = personality, communication, preferences, workflow, tech-stack, values, goals.
3. **Reduce:** one LLM call merges batch results + existing `traits.json` → canonical profile: per trait `{id, statement, category, confidence 0-1, evidence: [{source, session, quote}], first_seen, last_seen}`. Drop traits with confidence < 0.3, keep ≤ 100 (top by confidence×recency).
4. **Render SKILL.md**: deterministic template from traits.json — frontmatter (`name: user-persona`, `description:`), sections per category, "do/don't" heuristics derived from workflow/preferences. No LLM needed for rendering (stable diffs).
5. **Audit:** write `profile/runs/<ts>.json` (batches, sessions covered, raw model outputs).

`--if-changed` exits 0 doing nothing if no chats changed since last run (cron-friendly). `install` copies `SKILL.md` to `~/.claude/skills/user-persona/SKILL.md`, `~/.config/opencode/skill/user-persona/SKILL.md`, crush/codex equivalents, or `--agents-dir`.

Privacy: profile sends rendered markdown (redacted per config) to the configured provider only; `redact_all = true` redacts JSONL too; sources can be excluded via `[profile.exclude_sources]`.

## Error handling

- Adapter failures: log warn, continue, nonzero exit only if *all* enabled sources fail.
- LLM failures in map: retry ×2 with backoff; failed batches logged in run audit; reduce proceeds with partial results (flagged).
- No network except `profile`.

## Testing

- Unit: each adapter parser against `tests/fixtures/<source>/` fixtures (small, committed); redaction; slug/id hashing; merge/dedup logic; SKILL.md renderer (snapshot).
- Integration: temp-store end-to-end: init → sync (fixture sources dir) → import fixture dump twice with an updated variant → assert no dupes; git has 3 commits.
- Profile: mock OpenAI-compatible server (axum) in tests for map/reduce; determinism of renderer.
- Manual on this machine: `aiyou init && aiyou sync && aiyou status`.

## Stack

clap(derive), serde+serde_json, toml, rusqlite(bundled), sha2, chrono, anyhow, regex, tokio, reqwest, flate2+zip(unzip via `zip` crate), tempfile, axum(tests only).

## Decomposition into implementation phases

1. Core: store, canonical model, git, config, status.
2. Harvest: claude-code, opencode, then codex/gemini-cli/crush/pi.
3. Import: chatgpt, claude-ai, gemini; manifest + progressive merge.
4. Profile: client, map/reduce, traits.json, SKILL.md renderer, install, cron helper.
