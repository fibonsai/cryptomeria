---
name: add-task
description: Add a task to docs/TODO.md with corrected grammar and formatting
---
# Add Task

Add a task to `docs/TODO.md` with corrected grammar and formatting.

## Instructions

Read the string after `/add-task` and append a task to `docs/TODO.md` with these rules:

1. **Read the input string** after `/add-task`
2. **Fix grammar/spelling/punctuation silently** - do not call out corrections
3. **Capitalize the first letter**
4. **End with a single period** (no trailing whitespace)
5. **Use imperative/gerund phrasing** matching existing tasks (e.g., "Add...", "Implement...", "Fix...", "Optimize...", "Create...", "Modify... to...")
6. **Preserve exactly**: technical terms, file paths, CLI flags, formulas, code identifiers, parquet column names
7. **Do not add scope** the user didn't mention
6. **If ambiguous** → ask for clarification (don't invent details)
7. **Keep on one line**
8. **Format**: `[ ] - <fixed string>.` appended as the last line of `docs/TODO.md`
9. **No shell/Python scripts** - use the Edit tool to append directly

IMPORTANT: NEVER execute the task unless explicitly asked. Just create/update tasks in docs/TODO.md


## Examples

Input: `/add-task fix typo in readme`
→ Appends: `[ ] - Fix typo in readme.`

Input: `/add-task implement OKX websocket client for LOB and trades`
→ Appends: `[ ] - Implement OKX WebSocket client for LOB and trades.`

Input: `/add-task add order management system with OKX REST api integration`
→ Appends: `[ ] - Add order management system with OKX REST API integration.`

Input: `/add-task fix bug in order matching engine`
→ Appends: `[ ] - Fix bug in order matching engine.`
