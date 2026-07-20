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

### 3. Create Architecture Decision Record (ADR) for behavior changes

**Always** create an ADR if the plan introduces a new approach, changes architecture, or modifies behavior. For simple refactors or bug fixes with no behavioral impact, skip this step.

If an ADR is needed:

- Check existing ADRs in `docs/` (`ls docs/ADR-*.md 2>/dev/null`)
- The sequential number is one more than the highest existing ADR (or 1 if none)
- File name template: `docs/ADR-<N>-<YYYYMMDD>-<short-title-with-dashes>.md`
- Include: Title, Context, Options Considered, Decision, Consequences, Status
- Add a link to the ADR in the plan's `## References` section
- Update `CLAUDE.md` with the ADR link under the **ADRs** section
- Stage and commit the new ADR and CLAUDE.md as part of the plan creation

### 4. Check if the issue body is empty

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

Each subsequent update to the plan **must** be a new comment with the complete updated plan — never edit a previous plan comment.

### 5. Commit and push

```bash
git add docs/ && git commit -m "Add ADR-<N>: <title>"
git push origin <branch>
```

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
