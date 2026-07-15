---
name: execute-plan
description: Use ONLY when user types /execute-plan or says "execute plan". Runs PLAN.md step by step, updates docs, moves plan into changelog, deletes PLAN.md.
---

# Execute PLAN

Read `docs/PLAN.md`. Execute each task section in order, marking sub-steps as they complete. After all tasks are done:
1. Update `README.md` with any CLI changes, test counts, behavior.
2. Mark completed items `[x]` in `docs/TODO.md`.
3. Write a changelog at `docs/<YYYYMMDD>-<SEQ>-<brief_name>.md`.
4. Append PLAN contents as a `# PLAN` section to the latest changelog file.
5. Delete `PLAN.md`.