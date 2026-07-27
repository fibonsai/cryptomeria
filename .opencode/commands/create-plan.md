---
name: create-plan
description: Read the last open GitHub issue and write a PLAN.md with implementation sub-steps, then store it in the issue body (if empty) or as a new comment
---

# Create PLAN.md from the last open GitHub issue

The plan is stored in the **issue body** (if previously empty) or as a **new comment** (if body already has content or plan is being updated). `docs/PLAN.md` is a local working copy.

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

If the plan changes behavior or introduces a new approach (not a simple refactor or bug fix), add a subtask to **Create an ADR** documenting the decision. The ADR will be created and uploaded directly to GitHub Wiki (not to `docs/`) during `/execute-plan`, not executed here.

### 3. Check if the issue body is empty

```bash
BODY=$(gh issue view "$ISSUE" --json body -q '.body')
```

If the body is empty or null, store the plan there (first plan for this issue):

```bash
gh issue edit "$ISSUE" -F docs/PLAN.md
```

If the body already has content, or if this is an update to an existing plan, post the plan as a new comment instead:

```bash
gh issue comment "$ISSUE" -F docs/PLAN.md
```

**IMPORTANT — Plan updates must always be the full plan.**
- Every plan update **must** repeat the complete plan in full, never just the changes.
- Never edit or replace a previous plan comment — always post a new comment.
- The most recent comment containing `# PLAN` is the canonical source for `/execute-plan`.

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
 
 ### N. Create ADR (IMPORTANT: ADR will be created but uploaded to GitHub Wiki, not docs/)

 Issues found:
 - <why an ADR is needed>

- [ ] Create ADR-<N> documenting the decision
- [ ] Upload ADR to GitHub Wiki in the appropriate category (not to `docs/`)
- [ ] Add entry to wiki Topic-Index.md
- [ ] **Do NOT create the ADR in `docs/` — the wiki is the canonical location**

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
