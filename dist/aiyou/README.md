# aiyou

**https://geniusrise.github.io/you**

Export, maintain and analyze **all** your AI coding-agent chat history — Claude Code, OpenCode, Codex, Charm Crush, Gemini CLI, Pi — in one normalized, **git-versioned store** on disk. Import your ChatGPT / Claude.ai / Gemini dumps alongside them. Then run an LLM **swarm** over everything to distill your personality, preferences and workflow into a portable `SKILL.md` your agents can use to align with you.

Local-first: the store is a plain git repo under `~/.aiyou`. No daemon, no server, no network — except when *you* run `aiyou profile`.

## Install

```bash
# Arch / Manjaro (AUR)
yay -S aiyou

# Debian / Ubuntu
sudo dpkg -i aiyou_0.2.0_amd64.deb   # from GitHub releases

# Fedora / RHEL
sudo dnf install aiyou-0.2.0-1.x86_64.rpm

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

# run the swarm: map-reduce over all chats -> profile/traits.json + profile/SKILL.md
export OPENAI_API_KEY=...          # or any OpenAI-compatible provider, see below
aiyou profile

# install the generated skill into a harness
aiyou profile install --target claude      # or opencode | crush | codex, or --agents-dir <path>
```

## Keep it updated (cron)

`aiyou sync` is incremental and idempotent — safe to run as often as you like. Print ready-made crontab lines:

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
    SKILL.md                     # generated skill for your agents
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

## The profile swarm

`aiyou profile` shards your chats into batches (~120k chars each) and runs N concurrent workers (configurable) that extract grounded trait candidates (personality, communication, preferences, workflow, tech-stack, values, goals — each with evidence quotes and confidence). A reduce pass merges candidates with your existing `traits.json` into a canonical profile. Re-runs are **incremental**: only chats changed since the last run are mapped.

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

Every run writes an audit record (`profile/runs/`) and a git commit, so profile history is versioned too.

## Privacy

- Everything stays local; LLM calls happen only during `aiyou profile`, only to the endpoint you configure.
- Markdown renders are secret-redacted before they're written (and therefore before they're ever sent to the LLM).
- Exclude sources from profiling via `exclude_sources`.

## Adding a new harness

Implement the three-method `Adapter` trait in `src/adapters/` (`name`, `discover`, `read`), register it in `registry()` (src/sync.rs), add a fixture + test. `discover` returns fingerprints (mtime^size or row versions); the sync engine handles all incrementals, writing, and git.

## Development

```bash
cargo test          # full suite: adapters, importers, pipeline, e2e (mock LLM, no network)
cargo clippy -- -D warnings
```
