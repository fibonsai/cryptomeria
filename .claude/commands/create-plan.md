---
name: create-plan
description: Read the last open GitHub issue and write a PLAN.md with implementation sub-steps, then post it as an issue comment (or edit the body if empty)
---

# Create PLAN.md from the last open GitHub issue

The plan is stored as an **issue comment** — `docs/PLAN.md` is a local working copy.

## Steps

### 1. Get the last open issue and all its context

```bash
ISSUE=$(gh issue list --state open --json number -L 1 -q '.[0].number')
gh issue view "$ISSUE" --json title,body,comments
```

Read all available context: title, existing body, and previous comments.

### 2. Write `docs/PLAN.md`

Using the full context (title, body, comments), write `docs/PLAN.md` with:

- A `# PLAN` header followed by the task title
- **Files to modify** — list every file that will be changed
- **Subtasks** — one section per logical change, each with:
  - A descriptive header
  - Issues found (specific problems in the current state)
  - Concrete `- [ ]` sub-steps with file paths
  - Verification commands to confirm correctness
- **Verification** — shell commands to validate the result
- **Changelog** — the final workflow: post changelog as an issue comment, then close the issue

### 3. Store the plan

If the issue body is empty, edit it with the plan:

```bash
gh issue edit "$ISSUE" -F docs/PLAN.md
```

If the issue body already has content, post the plan as a comment instead:

```bash
gh issue comment "$ISSUE" -F docs/PLAN.md
```

This makes the plan accessible to `/execute-plan`.

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
