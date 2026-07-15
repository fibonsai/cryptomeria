---
name: add-task
description: Add a task to github issue with corrected grammar and formatting
---
# Add Task

Add a task to github issue with corrected grammar and formatting.

## Instructions

Read the string after `/add-task` and append a task to github issue with these rules:

1. **Read the input string** after `/add-task`
2. **Fix grammar/spelling/punctuation silently** - do not call out corrections
3. **Capitalize the first letter**
4. **End with a single period** (no trailing whitespace)
5. **Use imperative/gerund phrasing** matching existing tasks (e.g., "Add...", "Implement...", "Fix...", "Optimize...", "Create...", "Modify... to...")
6. **Preserve exactly**: technical terms, file paths, CLI flags, formulas, code identifiers, parquet column names
7. **Do not add scope** the user didn't mention
6. **If ambiguous** → ask for clarification (don't invent details)
7. **Keep on one line**
8. **Format**: `<fixed string>.`
9. **No shell/Python scripts** - use the `gh issue create -t "<fixed string>" -b ""` tool to create directly

IMPORTANT: NEVER execute the task unless explicitly asked. Just create/update the issue

ABORT ON ERROR: If "gh" return error, abort the execution (NEVER retry).


## Examples

Input: `/add-task fix typo in readme`
→ Execute: `gh issue create -t "Fix typo in readme." -b ""`

Input: `/add-task implement OKX websocket client for LOB and trades`
→ Execute: `gh issue create -t "Implement OKX WebSocket client for LOB and trades." -b ""`

Input: `/add-task add order management system with OKX REST api integration`
→ Execute: `gh issue create -t "Add order management system with OKX REST API integration." -b ""`

Input: `/add-task fix bug in order matching engine`
→ Execute: `gh issue create -t "Fix bug in order matching engine." -b ""`
