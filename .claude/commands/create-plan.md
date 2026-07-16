---
name: create-plan
description: Read the last open GitHub issue and write a PLAN.md with implementation sub-steps, then store it in the issue body as the canonical source
---

# Create PLAN.md from the last open GitHub issue

The issue body is the **single source of truth** — `docs/PLAN.md` is a local working copy synced to the issue body.

## Steps

### 1. Get the last open issue

```bash
gh issue list --state open --json number -L 1 -q '.[0].number'
```

Capture the number — this is the issue whose body will hold the plan.

### 2. Write `docs/PLAN.md`

Read the issue title (`gh issue view <N> --json title -q '.title'`) and write `docs/PLAN.md` with:

- A `# PLAN` header followed by the task title
- **Files to modify** — list every file that will be changed
- **Subtasks** — one section per logical change, each with:
  - A descriptive header
  - Issues found (specific problems in the current state)
  - Concrete `- [ ]` sub-steps with file paths
  - Verification commands to confirm correctness
- **Verification** — shell commands to validate the result
- **Changelog** — the final workflow: post changelog as an issue comment, then close the issue

### 3. Sync the plan to the issue body

```bash
gh issue edit <N> -F docs/PLAN.md
```

This makes the issue the canonical source that `/execute-plan` reads from.

## Template

When drafting `docs/PLAN.md`, use this structure:

```markdown
# PLAN

Task: <issue title>

## Files to modify

- <path>

## Subtasks

### 1. <descriptive header>

Issues found:
- <problem in current state>

- [ ] <action with file paths>
- [ ] <next action>

### 2. ...

## Verification

```bash
<shell command to confirm correctness>
```

## Changelog

After execution, post a changelog as a comment on this issue (without a PLAN section), then close the issue.
```

## Important

- **Never execute the plan** — this command only creates and stores it
- Sub-steps use markdown checklist format: `- [ ]`
- Verification commands must be runnable shell snippets
