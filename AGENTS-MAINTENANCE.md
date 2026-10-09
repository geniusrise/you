# AGENTS-MAINTENANCE.md

Maintenance guide for AI agents (and humans) working on **aiyou**. Read this before changing anything. Priorities and roadmap live in `FEATURES.md` — if you're picking up new work, read that first; this file tells you *how*.

---

## What this project is

`aiyou` is a Rust CLI that:

1. **Harvests** chat history from local coding-agent harnesses into one normalized, git-versioned store (`~/.aiyou`).
2. **Imports** progressive dumps from hosted chat products (ChatGPT, Claude.ai, Gemini).
3. **Profiles** the user via an incremental LLM map-reduce ("swarm") to maintain `traits.json` + a portable `SKILL.md`.

Local-first, read-only against sources, secrets redacted, no network except `aiyou profile`.

**Repo:** `git@github.com:geniusrise/you.git` · branch **`master`** (never `main`) · version lives in `Cargo.toml` + `packaging/aur/PKGBUILD`.

---

## Layout map

```
src/
  lib.rs              # Cli (clap), Cmd dispatch, run(), status()
  config.rs           # Config, SourcesCfg, ProfileCfg, SOURCES (canonical source names)
  model.rs            # SessionMeta, Msg, stable_msg_id()
  store.rs            # write_session (jsonl+md), find_session, list_sessions, fingerprints path
  render.rs           # slug(), redact(), render_markdown()
  git.rs              # shell-out git helpers (init/commit_all/last_commit), warn-once if git missing
  sync.rs             # sync engine (fingerprint incrementals) + registry()  <-- all adapters wired here
  cron.rs             # crontab line printer
  adapters/
    mod.rs            # Adapter trait, DiscoveredSession
    scan.rs           # shared scan helpers: scan(), parse_scanned(), find_message_arrays()
    generic.rs        # GenericScan — zero-effort adapter for JSON/JSONL session formats
    claude_code.rs, opencode.rs, codex.rs, gemini_cli.rs, crush.rs, pi.rs,
    antigravity.rs, goose.rs, copilot.rs, cursor.rs
  importers/
    mod.rs            # import engine: manifest, identity rules, upsert_session(), resolve_path()
    chatgpt.rs, claude_ai.rs, gemini_takeout.rs
  profile/
    client.rs         # LlmClient (OpenAI-compatible, retry, salvage_json)
    pipeline.rs       # map/reduce swarm; run_with_client() is the testable entry
    traits.rs         # CATEGORIES, Trait, Evidence, ProfileDoc (normalize/cap)
    render_skill.rs   # deterministic SKILL.md renderer + install()
tests/                # one file per concern; fixtures in tests/fixtures/<source>/
packaging/aur/PKGBUILD, packaging/deb/control, packaging/rpm/aiyou.spec
.github/workflows/    # ci.yml, release.yml, pages.yml
docs/index.html       # the GitHub Pages website
```

---

## Hard invariants (do not break)

1. **Read-only sources.** Never write to harness data. Open SQLite with `OpenFlags::SQLITE_OPEN_READ_ONLY`. Live-locked DBs (VS Code running) may fail to open — that's fine: warn + skip, never abort the sync.
2. **Source names are canonical.** `config::SOURCES` is the store-directory vocabulary (`chats/<source>/…`). The CLI import arg `gemini` maps to store source `gemini-import`. Add new sources to `SOURCES` — adding one must default to *enabled* for old configs (see next rule).
3. **Config forward-compat.** `sources.enabled` is `Option<Vec<String>>` — **absent means all sources**. Never serialize it when None (`skip_serializing_if`). Every new config field needs a serde default so existing `~/.aiyou/aiyou.toml` files keep loading. New adapters must light up on old stores with no user action.
4. **Canonical session files.** `.jsonl` line 1 = header `{"aiyou":"session", source, id, project, title, created_ms, updated_ms}`, then messages sorted by `(ts_ms, id)`. File names are `slug(id)`. Stable ordering = stable git diffs. `find_session()` scans project dirs to locate the file (project slug is part of the path).
5. **Empty sessions are never written**, but their fingerprint IS recorded (so we don't re-read empty sources every sync). Engine logic lives in `sync.rs` `run_with_adapters`.
6. **Fingerprints** (`~/.aiyou/.fingerprints.json`, key `"<source>/<key>"`): file adapters use `mtime_ns ^ (size << 1)`; SQLite adapters use `(max_row_version) ^ (count << 40)`. Update the cache only after a successful write (or a skip-empty).
7. **Message identity (imports):** external id if present, else `stable_msg_id(role, text, ts)` = sha256. Conversation identity: `(source, external_id)` else `slug(title) + "-" + hash8(first user msg)`. Ids must be stable across progressive dumps or re-imports duplicate.
8. **Redaction before anything leaves the machine.** `render::redact()` runs at `.md` write time (default on) and on `.jsonl` only if `redact_all = true`. The profile swarm only ever reads the redacted `.md` files. If you add a new export path, redact it.
9. **No nested tokio runtimes.** Inside the pipeline's runtime use the async `LlmClient::chat()`. `chat_blocking()` (which creates its own `Runtime`) is only safe on plain threads — calling it inside the pipeline runtime panics ("Cannot start a runtime from within a runtime").
10. **`target/` must never be committed.** History was once polluted with 2.2 GiB of `target/` and had to be rewritten with `git filter-repo`. `.gitignore` has `/target`. If `git count-objects -vH` shows the pack growing past ~10 MB, stop and check what got committed.
11. **LLM strictness lives in two prompts** (`pipeline.rs`: `MAP_SYSTEM`, `REDUCE_SYSTEM`) plus the category enum in `traits.rs::CATEGORIES`. If you add/remove a category, update: `CATEGORIES`, both prompts, `ProfileDoc::normalize` (which validates categories), the SKILL renderer (iterates `CATEGORIES`), and `tests/profile_test.rs` fixture outputs.
12. **Tests never touch the network.** LLM calls go against an in-process axum mock (`tests/profile_test.rs` pattern). No test reads `~/.claude` or any real source dir — fixtures only, or built-in-test SQLite DBs created in `tempdir()`.

---

## Development loop

```bash
cargo test                              # full suite (~34 tests, all offline)
cargo clippy --all-targets -- -D warnings   # must be zero warnings
cargo build --release --locked          # Cargo.lock is committed; keep it updated
```

- TDD: write the failing test in the relevant `tests/*_test.rs` first, then implement.
- Commit style: `feat:`, `fix:`, `docs:`, `chore:`, `test:` — short, lowercase.
- Commit on `master` directly is fine for small changes; the user owns this repo and pushes are manual unless asked.

---

## Recipe: add a new harness adapter (the most common maintenance task)

1. **Recon first.** Find where the harness stores sessions on disk (`~/.<tool>`, `~/.config/<tool>`, `~/.local/share/<tool>`). Inspect a real file. Formats on this machine change — trust the file, not this doc.
2. **Decide the adapter type:**
   - Simple JSON with top-level `messages` array, or JSONL with one message-ish object per line → use `adapters::generic::GenericScan` — no new file needed, just a `registry()` entry (`sync.rs`) + config dir fields.
   - SQLite / bespoke format / needs filtering → new file `src/adapters/<name>.rs` implementing:
     ```rust
     impl Adapter for X {
         fn name(&self) -> &'static str { "<source>" }
         fn discover(&self) -> Result<Vec<DiscoveredSession>> { /* key + fingerprint per session */ }
         fn read(&self, key: &str) -> Result<(SessionMeta, Vec<Msg>)> { /* canonicalize */ }
     }
     ```
   - Message-ish arrays buried in arbitrary JSON (VS Code-style payloads): reuse `scan::find_message_arrays`.
3. **Wire it up (checklist):**
   - [ ] Add the source name to `config::SOURCES`
   - [ ] Add dir/db config fields to `SourcesCfg` (`Option`, with sane default path in `registry()`) + `Default` impl
   - [ ] Register in `sync::registry()` with default path(s) under `$HOME`
   - [ ] Fixture: `tests/fixtures/<source>/…` — small, realistic, **redacted** (no real user data)
   - [ ] Tests: parse fixture (roles, tool msgs, ts), missing-dir → `Ok(vec![])`, and a sync-into-store test
   - [ ] `README.md` sources table + `docs/index.html` chips (add `<span class="chip new">…`)
   - [ ] `cargo clippy --all-targets -- -D warnings` clean, all tests green
4. **Role conventions:** user text → `"user"`, assistant text → `"assistant"`, tool calls/results → role `"tool"` with `tool: Some(name)`. Truncate message text to 8000 chars, tool payload 2000. Titles: first user message, 60 chars. Projects: basename of cwd/workspace dir, else `"default"`.
5. **Missing source = `Ok(vec![])` + optional warn.** Never propagate discovery errors for a merely-absent source.

## Recipe: add a new import source

1. `src/importers/<name>.rs` with `parse_dump(root, exact: Option<&Path>, source) -> Result<Vec<(SessionMeta, Vec<Msg>)>>`.
2. Register in `importers::mod.rs`: match arm in `import_dump` + filename list in `find_dump_file`. If the CLI name differs from the store source name, map it (see `gemini → gemini-import`).
3. `resolve_path`/`find_dump_file` must prefer the exact input file over re-scanning the parent dir (a dir can contain several dump versions).
4. Fixture pair: original + updated variant; test: add → identical re-import is a no-op (`unchanged`), updated dump merges by message id with zero duplicates; manifest gains one entry per *new* dump hash.
5. Zips are extracted to a temp dir kept alive for the call — don't archive raw dumps into the store.

## Recipe: change the profile pipeline

- Prompt changes belong in `pipeline.rs` constants only. Keep `MAP_SYSTEM` demanding strict JSON and "HUMAN user only"; keep `REDUCE_SYSTEM` preserving ids on merge.
- `ProfileDoc::normalize`: drop confidence < 0.3, cap 100, categories validated against `CATEGORIES`.
- Every run writes: `traits.json`, `SKILL.md`, `runs/<ts>.json` audit, one git commit `profile: N traits from M sessions`.
- `run_with_client(store, cfg, Opts, &LlmClient)` is the test seam — never make the client a global.

---

## Release process

CI (`release.yml`) is wired to do this on `v*` tags, **but the geniusrise org currently has a billing lock** ("account is locked due to a billing issue") — until that's fixed, do it manually in this order:

1. Bump `version` in `Cargo.toml` (+ regenerate `Cargo.lock` via build), update `packaging/aur/PKGBUILD` `pkgver`, `docs/index.html` pill, README if versions appear.
2. `cargo test && cargo clippy --all-targets -- -D warnings`
3. `cargo build --release --locked && strip target/release/aiyou`
4. Build all three packages (see gotchas below), then:
   `gh release create vX.Y.Z dist/*.{tar.gz,deb,rpm} --repo geniusrise/you --generate-notes`
5. Push: `git push origin master && git tag vX.Y.Z && git push origin vX.Y.Z`
6. **AUR:** copy `packaging/aur` to a scratch dir, fill `sha256sums` with `curl -sL <tag tarball> | sha256sum`, run `makepkg --printsrcinfo > .SRCINFO` (and `makepkg -f` to verify it builds), `git init`, commit both files, `git push ssh://aur@aur.archlinux.org/aiyou.git master`. Once CI is unlocked, setting the `AUR_SSH_KEY` repo secret automates this step forever.

### Packaging gotchas (learned the hard way)

- **AUR:** `options=('!lto')` is mandatory — Manjaro/Arch `makepkg` injects `-flto=auto` into LDFLAGS which breaks Rust static linking (undefined sqlite3/zstd symbols). Use fresh `CARGO_HOME="$srcdir/cargo-home"`, `cargo build --release --locked --frozen`, generate completions from the built binary. Keep `check()` running `cargo test`.
- **deb:** built with a hand-rolled ar writer (no dpkg on Arch). ar header layout: name(16) mtime(12) uid(6) gid(6) mode(8) size(10) `\`\n`. Verify with `bsdtar -tf x.deb`.
- **rpm:** spec needs `%define debug_package %{nil}` (stripped binary → empty debugfiles.list failure) and `Source0: aiyou-binary.tar.gz` matching the tarball name you build in CI (`%setup -q -n aiyou-binary`).
- Binary packaging (deb/rpm) just ships the prebuilt stripped binary + completions; only the AUR package compiles from source.

---

## Website (GitHub Pages)

- Served from `docs/` on `master` → https://geniusrise.github.io/you/
- Single file `docs/index.html`, zero dependencies, no build step. Keep it that way.
- When adding sources: update the chips section. When releasing: bump the version pill.
- `pages.yml` deploys on pushes touching `docs/**`.

## Store on this machine (dogfooding)

`~/.aiyou` is a live store with real data (claude-code 204+, opencode 107+, gemini-cli, antigravity, goose). After adapter changes, run `cargo run -q -- sync` and inspect a rendered `.md` before committing. If a sync produces empty/garbage sessions (like the VS Code noise incident), fix the adapter AND clean the store (`rm -rf ~/.aiyou/chats/<bad-source>` + commit in the store repo).
