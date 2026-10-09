# FEATURES.md

Roadmap and feature-addition strategy for **aiyou**. If you are an agent picking up work on this repo, this file is the source of truth for what gets built and when.

---

## Product thesis (the one thing we do)

**Extract data from past user interactions to know the user better, to serve them better.**

aiyou turns scattered AI chat history into one normalized store, and distills it into a portable profile (`SKILL.md`) that every agent the user runs can consume. Nothing near-term may expand beyond this thesis.

There are exactly two axes of near-term extension:

1. **More surfaces in** — where chat data lives: new harvest adapters, new import sources.
2. **More surfaces out** — where the profile gets consumed: new install targets for the generated skill.

---

## Near-term (v0.3 → 1.0): integrations & hardening only

### A. More surfaces in — harvest adapters

Each is a candidate adapter following the recipe in `AGENTS-MAINTENANCE.md` (recon first — trust the on-disk format, not this list):

| Candidate | Where the data likely lives | Notes |
| --- | --- | --- |
| Windsurf | `~/.codeium/windsurf` state dbs | same state.vscdb pattern as cursor |
| Amp | `~/.config/amp/sessions` | generic scan already points here; verify real format |
| aider | `~/.aider.chat.history.md`, `~/.aider/` | single markdown log — needs a small custom parser |
| Warp | `~/.warp` | their blocks/agents store |
| Jules / Droid / Factory | respective home dirs | confirm local persistence before promising |
| Zed (verified format) | `~/.config/zed/conversations` | generic scan guesses; verify against real files |
| Continue (real format) | `~/.continue/sessions` | current parser is best-effort; verify |
| Claude Code extras | todos, shell snapshots | sessions are covered; extras optional |

### B. More surfaces in — import sources

- Perplexity data export
- GitHub Copilot account export (if/when available as dumps)
- **Format drift tracking**: ChatGPT/Claude/Gemini export schemas change; fixtures must be refreshed when real dumps break parsers (the importers are tolerant, but identity rules must stay stable)

### C. More surfaces out — profile install targets

The skill currently installs to claude, opencode, crush, codex, `--agents-dir`. Add:

- `AGENTS.md` / `CLAUDE.md` injection (append a managed `<!-- aiyou:begin -->…<!-- aiyou:end -->` block — idempotent, never clobber user text)
- Gemini CLI `GEMINI.md` context
- goose, amp, continue, zed rule/context files
- Cursor rules (`.cursor/rules`)
- A plain `--print` of the skill to stdout for piping anywhere

### D. Core hardening (compatibility promises)

- **Store format versioning**: version the `.jsonl` session header (`"fmt": 1`) and the fingerprints cache; migrations for future changes. Goal: **version updates never break the store** — this is the gate to 1.0.
- `aiyou doctor` — per-source discovery report (found / empty / error / skipped, plus resolved paths) so "why is source X empty" is one command.
- Profile quality: redaction coverage tests, incremental correctness (only changed chats re-mapped), evidence quotes spot-checked.
- Stability: sync idempotency tests on every adapter; adapters must degrade gracefully.

**Near-term anti-scope**: no new subsystems, no daemon, no network beyond the configured profile endpoint.

---

## Deferred until ~1.0: utils built on the store

Semantic search, analytics (token/cost/session stats, activity heatmaps), an MCP server exposing the store+profile to agents, HTML/PDF digests, encryption-at-rest — these are **second-layer utilities** on a mature store.

**Gate for starting this layer** (all three required):

1. Core battle-tested across real machines and OS variants
2. Multiple minor releases with **zero store-compatibility breaks**
3. Adapter suite mature (the harvest list above largely done)

Do not pull items from this layer forward because they look fun. The store's trustworthiness is the product.

---

## Non-goals (permanent)

- No cloud sync, no aiyou-hosted anything, no auto-push to remotes
- No back-mutation of harness data — sources are read-only, always
- No vector DB / embeddings in the core
- No feature that sends unredacted data off-machine

---

## How a feature gets done

1. It must map to a section above (A/B/C/D) — or it's not near-term.
2. Follow the recipes in `AGENTS-MAINTENANCE.md` (adapter recipe, import recipe, install-surface pattern).
3. Definition of done: failing-test-first implementation, `cargo test` green, `cargo clippy --all-targets -- -D warnings` clean, README + `docs/index.html` updated if user-facing, store-compatibility preserved, committed with `feat:`/`docs:` prefix on `master`.