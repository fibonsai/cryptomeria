---
name: commit
description: Commit all files changed by the last issue resolution.
---

# Commit

Commit the work just executed (the last `/execute-plan` task) as a single git commit.

## Steps

### 1. Enumerate changed files

```bash
git status
git diff --stat
```

Identify the files that belong to the last task:

- Source files (`**/*.rs`, `**/*.py`)
- Test files (`**/tests/**`)
- `README.md`
- `AGENTS.md`
- `CLAUDE.md`
- `.claude/**`
- `.opencode/**`
- `docs/` (if any)

### 2. Stage only those files

```bash
git add <path>
```

Stage each file explicitly. **Do not use `git add -A` or `git add .`** — never sweep in:

- Unrelated files
- Secrets (`.env.local`)
- Build artifacts (`.venv/`, `__pycache__/`, `.pytest_cache/`)
- Data files (`data/*.parquet`, `*.svg`)

These are gitignored and must stay untracked. If any path is gitignored, skip it silently.

### 3. Verify the staged set

```bash
git status
```

Confirm the staged files match the task and nothing else.

### 4. Write a commit message

Inspect recent commits for tone:

```bash
git log --oneline -10
```

Format:
- **Subject**: imperative, present tense, ≤72 chars, no trailing period
- **Body**: short WHY summary (one or two lines) and a bullet list of files changed

```bash
git commit -m "<subject>" -m "<body>"
```

### 5. Confirm

```bash
git log -1 --stat
```

### Error handling

If a pre-commit hook rejects the commit, read the error, fix the issue, and create a new commit (do not amend).
