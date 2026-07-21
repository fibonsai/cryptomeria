# Cryptomeria — OpenCode Agent Guide

High-signal OpenCode-specific facts. For full repo conventions, read `CLAUDE.md` — this file covers only what's OpenCode-specific.

---

## Custom Commands (Slash Commands)

Defined in `.opencode/commands/`. Available in the TUI:

| Command | Action |
|---------|--------|
| `/add-task "<desc>"` | Create a GitHub issue |
| `/create-plan` | Read last open issue → write `docs/PLAN.md` with sub-steps |
| `/execute-plan` | Execute PLAN.md stepwise, update README, post changelog, delete PLAN.md |
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

For full details on tests, architecture, conventions, ADRs, and LOB semantics, see `CLAUDE.md`.
