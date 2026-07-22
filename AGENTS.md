# Cryptomeria — OpenCode Agent Guide

High-signal OpenCode-specific facts. For full repo conventions, read `CLAUDE.md` — this file covers only what's OpenCode-specific.

---

## Custom Commands (Slash Commands)

Defined in `.opencode/commands/`. Available in the TUI:

| Command | Action |
|---------|--------|
| `/add-task "<desc>"` | Create a GitHub issue |
| `/create-plan` | Read last open issue → write `docs/PLAN.md` with sub-steps → store in issue |
| `/execute-plan` | Execute PLAN.md stepwise → update docs → post changelog → delete PLAN.md → create ADR → create PR → close issue → return to main |
| `/commit` | Stage task-related files only, commit with project-style message (no push) |

These are ports of the original `.claude/commands/` equivalents. The `.claude/` versions are legacy and may diverge.

### Usage notes
- All four commands use `gh` (GitHub CLI) — must be authenticated
- Commands accept `$ARGUMENTS` and shell injection (`!`cmd``) in templates
- Never commit unless asked — `/commit` stages explicitly, avoids `git add -A`

---

## Config & Structure

| File | Purpose |
|------|---------|
| `opencode.json` | Provider (ollama), skills paths |
| `.opencode/commands/` | Custom slash commands (auto-discovered) |
| `.opencode/skills/` | Not yet populated (path configured in `opencode.json`) |

Neither `AGENTS.md` nor `.opencode/` is gitignored — they are trackable.

---

## Repo Quick Commands (from CLAUDE.md)

```bash
make dev    # uv sync --dev + cargo build
make check  # lint + test (both languages)
make quick  # format → lint → test
make lint   # ruff check + cargo clippy -D warnings
make test   # pytest python/ + cargo test (rs)
make format # ruff format + cargo fmt
```

For full details on tests, architecture, conventions, and LOB semantics, see `CLAUDE.md`.

---

## ADRs

| # | Title | File |
|---|-------|------|
| 001 | `tokio-tungstenite` for OKX WS | `docs/ADR-001-...` |
| 002 | `BTreeMap<OrderedFloat>` for LOB2 | `docs/ADR-002-...` |
| 003 | QuestDB with refinery for persistence | `docs/ADR-003-...` |
| 004 | Normalized LOB levels storage | `docs/ADR-004-...` |
| 005 | QuestDB persistence cleanup | `docs/ADR-005-...` |
| 006 | Grafana LOB visualization | `docs/ADR-006-...` |
| 007 | Data output flag | `docs/ADR-007-...` |
| 008 | QuestDB TTL for automatic retention | `docs/ADR-008-...` |
| 009 | Grafana Infinity datasource | `docs/ADR-009-...` |
| 010 | Move TTL execution to startup | `docs/ADR-010-...` |
| 011 | Serve /metrics as JSON | `docs/ADR-011-...` |
| 012 | Exponential backoff for WS reconnect | `docs/ADR-012-...` |
