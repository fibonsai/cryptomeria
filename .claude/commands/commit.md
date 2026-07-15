---
name: commit
description: Commit the last task's changed sources, README, docs/TODO, and docs/changelog
---

Commit the work just executed (the last `/execute-plan` task) as a single git commit. 

Steps:

1. Run `git status` and `git diff --stat` to enumerate the currently changed and untracked files. Identify the files that belong to the last task: source files (`src/**`), tests (`tests/**`), `README.md`, `TODO.md`, and the new `docs/<YYYYMMDD>-<SEQ>-<brief>.md` changelog.
2. Stage ONLY those files explicitly with `git add <path>` for each one. Do not use `git add -A` or `git add .` — never sweep in unrelated files, secrets, `.env.local`, `.venv/`, `data/*.parquet`, `*.svg`, `__pycache__/`, or `.pytest_cache/` (these are gitignored and must stay untracked). If any path is gitignored, skip it silently.
3. Run `git status` again to confirm the staged set matches the task's files and nothing else.
4. Write a commit message in the project's existing style. Inspect `git log --oneline -10` first to match tone. Subject line: imperative, present tense, ≤72 chars, no trailing period. Body: a short summary of the WHY (one or two lines) and a bullet list of files changed with one-line descriptions.
5. Run `git commit -m \"<subject>\" -m \"<body>\"` (split the message into subject + body via multiple `-m` flags, or use a heredoc — pick whichever is cleanest). Do not push.
6. Run `git log -1 --stat` to confirm the commit landed correctly. If `git commit` was rejected by a hook, read the error, fix the issue, and create a new commit (do not amend).