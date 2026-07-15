---
name: create-plan
description: Use ONLY when user types /create-plan or says "create plan". Reads TODO.md pending tasks and writes PLAN.md with implementation sub-steps.
---

# Create docs/PLAN.md file from docs/TODO.md

Read `TODO.md`, find all unchecked (`[ ] - `) tasks. For each, write a `PLAN.md` with:
- task sections with descriptive headers
- concrete sub-steps (each with a checkbox)
- specific file paths and function names where changes go
- verification commands to confirm correctness

IMPORTANT: NEVER execute the PLAN unless explicitly asked. 

## Example

Input: `/create-plan`

→ If docs/TODO.md has a pendent task named `[ ] - Fix typo in readme.`, create a new docs/PLAN.md file:
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


