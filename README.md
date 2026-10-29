# aiyou

**https://geniusrise.github.io/you**

**Teach every AI agent who you are.**

aiyou reads your real conversations — Claude Code, OpenCode, Codex, Cursor, Copilot, Gemini CLI and more, plus ChatGPT / Claude.ai / Gemini dumps — and extracts your personality, taste and working style into a portable **`me` skill**. It keeps that profile current as you change, and **nothing updates until you approve it**. Every agent you run then talks to you, and does the work, the way you would.

```
chats ──► extract (LLM swarm) ──► proposed changes ──► you review ──► `me` skill ──► every agent
```

Local-first: the store is a plain git repo under `~/.aiyou`, so your profile history is diffable. No daemon, no server, no network — except when *you* run `aiyou profile`.

## Install

```bash
# Arch / Manjaro (AUR)
yay -S aiyou

# Debian / Ubuntu
sudo dpkg -i aiyou_0.3.0_amd64.deb   # from GitHub releases

# Fedora / RHEL
sudo dnf install aiyou-0.3.0-1.x86_64.rpm

# From source
cargo install --git https://github.com/geniusrise/you
# shell completions: aiyou --shell bash|zsh|fish
```

## Quickstart

```bash
aiyou init                # create ~/.aiyou (git repo + config)
aiyou sync                # harvest all local harness chats (incremental, idempotent)
aiyou status              # what's in the store

# import your hosted dumps (re-import newer dumps any time — no duplicates)
aiyou import chatgpt   ~/Downloads/chatgpt-export.zip
aiyou import claude-ai ~/Downloads/claude-export/conversations.json
aiyou import gemini    ~/Downloads/takeout/MyChat.json

# extract: map-reduce over new chats -> a *proposal*, not a change
export OPENAI_API_KEY=...          # or any OpenAI-compatible provider, see below
aiyou profile
aiyou review                       # see exactly what would change
aiyou accept                       # or: aiyou accept <id>... / aiyou reject [<id>...]

# put your `me` skill to work
aiyou install --target claude      # skill dir: claude | opencode | crush | codex
aiyou install --target claude-md   # managed block: claude-md | gemini | codex-md | agents-md
aiyou install --file ./AGENTS.md   # any context file (your own text is never touched)
aiyou install --print              # or pipe it anywhere

aiyou doctor                       # what was found, is the LLM key set, anything pending
```

## Keep it current (cron)

`aiyou sync` is incremental and idempotent. `aiyou profile --if-changed` only analyzes new or changed chats and **stages** its proposal; `aiyou status` tells you when something is waiting. Print ready-made crontab lines:

```bash
aiyou cron
# 0 3 * * * AIYOU_HOME=/home/you/.aiyou /usr/local/bin/aiyou sync
# 0 4 * * 0 AIYOU_HOME=/home/you/.aiyou /usr/local/bin/aiyou profile --if-changed
```

Add a git remote in `~/.aiyou` yourself if you want off-machine backups — aiyou never pushes.

## What gets stored

```
~/.aiyou/
  aiyou.toml                     # config
  chats/<source>/<project>/      # per session: <id>.jsonl (canonical) + <id>.md (rendered)
  imports/manifest.json          # every dump ever imported (hash, date, counts)
  profile/
    traits.json                  # canonical machine-readable trait profile
    SKILL.md                     # your `me` skill, rendered from the accepted traits
    proposal.json                # pending changes awaiting `aiyou accept` / `reject`
    state.json                   # content hashes of chats already absorbed
    rejected.json                # trait ids you declined (never re-proposed)
    runs/<ts>.json               # audit trail of each profile run
```

- `.jsonl` = line 1 session header, then one message per line, sorted by timestamp — stable, diff-friendly.
- `.md` = human-readable render, **secrets redacted by default** (API keys, tokens). Set `redact_all = true` to redact the canonical JSONL too.
- Sources are read-only; deleted-from-source sessions are kept in the store (archive semantics).

## Supported sources

| Harvest (read from disk) | Import (from dumps) |
| --- | --- |
| Claude Code, OpenCode, Codex, Charm Crush, Gemini CLI, Pi, Antigravity, Goose, GitHub Copilot (CLI + VS Code chat), Cursor, Amp, Continue, Zed | ChatGPT (`conversations.json` / .zip), Claude.ai (`conversations.json`), Gemini (Takeout JSON) |

Missing a source (e.g. you don't use Codex)? It's skipped with no errors. Paths are overridable in `aiyou.toml`.

## How extraction and maintenance work

`aiyou profile` shards your chats into batches (~120k chars each) and runs N concurrent workers that extract grounded trait candidates (personality, communication, preferences, workflow, tech-stack, values, goals — each with evidence quotes and confidence). A reduce pass merges them with your existing profile, and a deterministic diff turns the result into **proposed changes**: `+` add, `~` update, `-` retire.

- **You decide.** Changes wait in `profile/proposal.json` until `aiyou accept` (all or by id) or `aiyou reject`. Rejected traits are remembered and never proposed again. Prefer autopilot? `aiyou profile --yes` or `auto_accept = true`.
- **Nothing is lost by accident.** Traits the model doesn't mention are kept; only traits it explicitly retires (contradicted by newer evidence) are proposed for removal, and you approve that too.
- **Incremental by content.** A chat is re-analyzed only when its content hash changed. A failed batch leaves its chats unmarked, so they're retried next run.

Any OpenAI-compatible endpoint works:

```toml
[profile]
provider_base_url = "https://api.openai.com/v1"
api_key_env       = "OPENAI_API_KEY"
model             = "gpt-5.2"
workers           = 4
map_context_chars = 120000
exclude_sources   = []   # e.g. ["chatgpt"] to leave imports out
```

Point `provider_base_url` at OpenRouter, a local vLLM, or an Anthropic-compat proxy — anything speaking `/chat/completions` with `response_format: json_object`.

Every run writes an audit record (`profile/runs/`), and every accept/reject is a git commit, so your profile's history is versioned too.

## Privacy

- Everything stays local; LLM calls happen only during `aiyou profile`, only to the endpoint you configure.
- Markdown renders are secret-redacted before they're written (and therefore before they're ever sent to the LLM).
- Exclude sources from profiling via `exclude_sources`.

## Adding a new harness

Implement the three-method `Adapter` trait in `src/adapters/` (`name`, `discover`, `read`), register it in `registry()` (src/sync.rs), add a fixture + test. `discover` returns fingerprints (mtime^size or row versions); the sync engine handles all incrementals, writing, and git.

## Development

See [FEATURES.md](FEATURES.md) for the roadmap (near-term: integrations only) and [AGENTS-MAINTENANCE.md](AGENTS-MAINTENANCE.md) for the maintenance guide.

```bash
cargo test          # full suite: adapters, importers, pipeline, e2e (mock LLM, no network)
cargo clippy -- -D warnings
```
