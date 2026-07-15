---
name: create-plan
description: Use ONLY when user types /create-plan or says "create plan". Read last open github issue and writes docs/PLAN.md with implementation sub-steps.
---

# Create docs/PLAN.md file from last open github issue

Read last github issue: `gh issue view $(gh issue list --state open --json number -L 1 -q '.[0].number') --json title -q '.title'` and write a `docs/PLAN.md` with:
- task sections with descriptive headers
- concrete sub-steps (each with a checkbox)
- specific file paths and function names where changes go
- verification commands to confirm correctness

# Update issue body

Edit the issue body: `gh issue edit $(gh issue list --state open --json number -L 1 -q '.[0].number') -F docs/PLAN.md`

# IMPORTANT: NEVER execute the PLAN unless explicitly asked. 

## Example

Input: `/create-plan`

→ If last open issue has a pendent task named `Fix typo in readme.`, create a new docs/PLAN.md file:
```markdown
# PLAN

Task: Fix typo in readme.

## Files modified

- README.md

## Subtasks

[ ] - Read all README.md
[ ] - Find typo issues
[ ] - Correct issues without changing the meaning.
[ ] - Review README.md
[ ] - Create changelog in docs/<datetime_ref>-<sequential_number>-<short_explain_task>.md file with decisions and explain the actions executed
```

At last, update this issue body
