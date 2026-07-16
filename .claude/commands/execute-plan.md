---
name: execute-plan
description: Execute the plan stored in the GitHub issue body, step by step, then post a changelog comment and close the issue
---

# Execute PLAN

The plan lives in the **GitHub issue body** (not `docs/PLAN.md`). The issue is the single source of truth.

## Steps

### 1. Get the issue and read the plan

```bash
ISSUE=$(gh issue list --state open --json number -L 1 -q '.[0].number')
PLAN=$(gh issue view "$ISSUE" --json body -q '.body')
```

Read the plan body and identify all task sections (lines starting with `### `) and their `- [ ]` sub-steps.

### 2. Create an exclusive branch and work in there

branch name template: <tags separated by "-">/<short description replacing spaces with "-">

### 3. Execute each task section in order

For each section:

1. Read the issue and **Subtasks** bullets to understand the problems
2. Execute each `[ ]` sub-step in order, applying the intended fix to the listed files
3. After the section is complete, update the issue body with `[x]` for that section's sub-steps:
   ```bash
   gh issue edit "$ISSUE" -b "$(echo "$PLAN" | sed 's/- \[ \]/- [x]/')"
   ```

**On error** — if a sub-step fails (test, lint, command error):
- Report the failure and what caused it
- Undestand the error and fix it
- Do not continue to subsequent steps
- Do not close the issue

### 4. Update `README.md`

If the execution changed architecture, CLI flags, added/removed commands, or changed behavior, update `README.md` to reflect it.

### 5. Post the changelog as an issue comment

The changelog summarizes what was done. It must **not** contain a `# PLAN` section.

```bash
gh issue comment "$ISSUE" -b "# Changelog

Date: $(date -u +%Y-%m-%d)
Task: <issue title>

## Summary

<what was changed and why is important>

## Files modified

- <path> — <what changed>

## Test results

<count> passed, <count> failed — coverage notes."
```

### 6. Update the issue body, sub-steps, with completed checkboxes

All `[ ]` sub-steps should now be `[x]`. Update the body:

```bash
gh issue edit "$ISSUE" -b "$(echo "$PLAN" | sed 's/- \[ \]/- [x]/g')"
```

### 7. Delete `docs/PLAN.md`

```bash
rm docs/PLAN.md
```

### 8. Create a PR from exclusive branch

1. PR title is the same that Issue title.
2. Add ref to issue in PR body — do **not** append `🤖 Generated with [Claude Code](https://claude.com/claude-code)` or any auto-attribution line
3. Execute /commit (check .claude/commands/commit.md) in this branch

Do not merge PR (it will checked by human or another agent)

### 9. Close the issue

```bash
gh issue close "$ISSUE"
```

## Full workflow

```
issue body → execute → issue comment (changelog) → update issue body → create PR → close issue
```
