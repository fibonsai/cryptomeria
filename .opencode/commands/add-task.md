---
name: add-task
description: Add a task to a GitHub issue with corrected grammar and formatting
---

# Add Task

Create a GitHub issue from the text provided after the `/add-task` slash command.

## Rules

1. **Read the input** — take the text after `/add-task` as the task description ($ARGUMENTS)
2. **Fix silently** — correct grammar, spelling, and punctuation without comment
3. **Capitalize** the first letter of the description
4. **End with a single period** (no trailing whitespace)
5. **Use imperative phrasing** — match existing tasks ("Add...", "Implement...", "Fix...")
6. **Preserve exactly** — technical terms, file paths, CLI flags, formulas, code identifiers
7. **Keep on one line** — no newlines in the issue title
8. **Do not add scope** the user didn't mention
9. **If ambiguous** — ask for clarification (don't invent details)

## Execution

### 1. Sync main branch

```bash
git checkout main && git pull --rebase origin main
```

**ABORT** if the pull fails (conflicts, network error, etc.).

### 2. Create the issue with a descriptive body

Determine if the task requires a detailed body:
- If the task is simple and the title fully captures the intent, the body may be left empty (`-b ""`).
- If the task is complex or requires context, expand the fixed description into a detailed body that explains the goal, constraints, or technical la-out.

Write the final text as the issue body:

```bash
gh issue create -t "<fixed description>" -b "<enhanced body or empty string>"
``` 

**ABORT ON ERROR** — if `gh` returns a non-zero exit, stop immediately. Never retry.

## Examples

| Input | Created issue title |
|-------|-------------------|
| `/add-task fix typo in readme` | `Fix typo in readme.` |
| `/add-task implement OKX websocket client for LOB and trades` | `Implement OKX WebSocket client for LOB and trades.` |
| `/add-task add order management system with OKX REST api integration` | `Add order management system with OKX REST API integration.` |
