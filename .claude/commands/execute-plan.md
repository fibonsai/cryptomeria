---
name: execute-plan
description: Execute the plan stored in the GitHub issue body, step by step, then post a changelog comment and close the issue. If there is more than one open issue, STOP and ask the user how the open issue can be executed. In this case, read only the open issue indicated by the user, ignoring the others.
---

# Execute PLAN

The plan lives in the **GitHub issue body** (not `docs/PLAN.md`). The issue is the single source of truth.

## Steps

### 0. Pull main and review the plan for conflicts

```bash
git checkout main && git pull origin main
```

After pulling, check if any new changes on main affect the plan (e.g. files the plan modifies were changed upstream). If the plan needs updating:

1. Identify what changed in the conflicting files (`git diff HEAD@{1} -- <file>`)
2. Update the issue body with the revised plan
3. Re-read the plan from the updated issue body before proceeding

If there are no conflicts, proceed.

### 1. Get the issue and read all context

```bash
ISSUE=$(gh issue list --state open --json number -L 1 -q '.[0].number')
gh issue view "$ISSUE" --json title,body,comments
```

Read all available context: title, body, and comments. The plan may be in the body or in a comment. Identify the plan content and extract all task sections (lines starting with `### `) and their `- [ ]` sub-steps.

### 2. Create an exclusive branch from main and work in there

If the current branch is not `main`, ABORT this command, explaining that it is mandatory for the current branch to be `main` first.

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
- Understand the error and fix it
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

<what changed and why it matters>

## Files modified

- <path> — <what changed>

## Test results

<count> passed, <count> failed — <notes, e.g., coverage percentage or any regressions>."
```

### 6. Update the issue body's sub-steps with completed checkboxes

All `[ ]` sub-steps should now be `[x]`. Update the body:

```bash
gh issue edit "$ISSUE" -b "$(echo "$PLAN" | sed 's/- \[ \]/- [x]/g')"
```

### 7. Delete `docs/PLAN.md`

```bash
rm docs/PLAN.md
```

### 8. Create Architecture Decision Record (ADR)

Create an ADR doc in `docs/` with at least these sections:

* **Title**: A sequential number and an active-voice statement of the decision (e.g., ADR-001: Use PostgreSQL for primary database).
* **Context**: The forces, requirements, and background circumstances that prompted the decision.
* **Options Considered**: A list of serious alternatives, including their pros and cons.
* **Decision**: The chosen solution and a brief justification/rationale.
* **Consequences**: The positive and negative implications of the chosen path, including trade-offs.
* **Status**: Tracks the lifecycle stage of the choice (e.g., Proposed, Accepted, Rejected, or Superseded).

File name template: `docs/ADR-<sequential-number>-<YYYYMMDD>-<short-title-with-dashes>.md`

The sequential number should be one more than the highest existing ADR in `docs/`. The datetime is the date of creation in UTC.

Update `CLAUDE.md`, adding a link to each ADR under an **ADRs** section.

### 9. Create a PR from exclusive branch

1. PR title is the same as the issue title.
2. Add ref to issue in PR body — do **not** append `🤖 Generated with [Claude Code](https://claude.com/claude-code)` or any auto-attribution line
3. Execute /commit (check .claude/commands/commit.md) in this branch

Do not merge the PR (it will be checked by a human or another agent).

### 10. Close the issue

```bash
gh issue close "$ISSUE"
```

## Full workflow

```
pull main → read issue → execute plan → update README → post changelog → update issue body → delete PLAN.md → create ADR → create PR → close issue
```
