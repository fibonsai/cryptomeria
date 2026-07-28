---
name: execute-plan
description: Execute the plan stored in the GitHub issue body, step by step, then post a changelog comment. If there is more than one open issue, STOP and ask the user how the open issue can be executed. In this case, read only the open issue indicated by the user, ignoring the others.
---

> **Tools first**: Before running shell commands, prefer dedicated tools (Read, Write, Edit, Grep, Glob, Bash) or skills over raw shell. Use Write/Read/Edit tools for file operations instead of `cat`/`echo`/`sed`. Only fall back to shell when no tool alternative exists.

# Execute PLAN

The plan lives in the **GitHub Issue** (not `docs/PLAN.md`). The issue is the single source of truth.

IMPORTANT: ALWAYS download the PLAN from Github Issue BEFORE read it, saving in docs/PLAN.md, overriding old version, if exists. This file is a temp/cache.

## Steps

### 0. Check current branch. If is not `main`, ABORT

```bash
git branch
```

If the current branch is not `main`, ABORT this command, explaining that it is mandatory for the current branch to be `main` first.

### 1. Fetch remote main branch and create an git worktree

Worktree name template: <tags separated by "-">/<short description replacing spaces with "-">

```bash
PROJECT_ROOT=$(pwd)
WORKTREE='<tags separated by "-">/<short description replacing spaces with "-">'
git fetch --all
git worktree add -b "$WORKTREE" "$WORKTREE" HEAD
cd $WORKTREE
git pull --rebase origin main
```

If rebase can conflict, RESOLVE it before. Abort if not possible resolve the rebase problem.

### 2. Get the issue and read all context

```bash
ISSUE=$(gh issue list --state open --json number -L 1 -q '.[0].number')
FULL=$(gh issue view "$ISSUE" --json title,body,comments)
```

Read all available context: title, body, and comments.

### 3. Review the plan for conflicts

Check if any new changes on main affect the plan (e.g. files the plan modifies were changed upstream). If the plan needs updating:

1. Identify what changed in the planned files (`git diff HEAD@{1} -- <file>`)
2. Add a new issue comment with the FULL revised plan

### 4. Find the most recent plan

The plan may be in the issue body or in a comment. Scan all comments for the most recent one containing a `# PLAN` header. Use that as the active plan. If no comment contains a plan, fall back to the issue body.

```bash
# Extract the latest plan (prefer comments over body, most recent wins)
PLAN=$(echo "$FULL" | jq -r '[.comments[] | select(.body | startswith("# PLAN"))] | last | .body // .body')
```

If the plan is outdated or missing, abort and run `/create-plan` first.

Extract all task sections (lines starting with `### `) and their `- [ ]` sub-steps from the plan.

### 5. Execute each task section in order

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

### 6. Update `README.md` and `AGENTS.md`

If the execution changed architecture, CLI flags, added/removed commands, or changed behavior, update `README.md` and `AGENTS.md` to reflect it. Use GitHub Wiki links (not `docs/` paths) for all documentation and ADR references.

### 7. Post the changelog as an issue comment

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

### 8. Update the issue body's sub-steps with completed checkboxes

All `[ ]` sub-steps should now be `[x]`. Post an updated plan comment with checkboxes marked complete. If the plan lives in the issue body, edit it:

```bash
gh issue edit "$ISSUE" -b "$(echo "$PLAN" | sed 's/- \[ \]/- [x]/g')"
```

If the plan lives in a comment, post a new comment with the completed plan.

### 9. Delete `docs/PLAN.md`

```bash
rm docs/PLAN.md
```

### 10. Create Architecture Decision Record (ADR) and upload to GitHub Wiki

Create an ADR and upload it directly to GitHub Wiki (not to `docs/`).

The ADR **must** contain at least these sections:

* **Title**: A sequential number and an active-voice statement of the decision (e.g., ADR-028: Upload ADRs to GitHub Wiki instead of docs/).
* **Context**: The forces, requirements, and background circumstances that prompted the decision.
* **Options Considered**: A list of serious alternatives, including their pros and cons.
* **Decision**: The chosen solution and a brief justification/rationale.
* **Consequences**: The positive and negative implications of the chosen path, including trade-offs.
* **Status**: Tracks the lifecycle stage of the choice (e.g., Proposed, Accepted, Rejected, or Superseded).

The sequential number should be one more than the highest existing ADR in the wiki Topic-Index. The datetime is the date of creation in UTC.

#### Category Mapping

Assign the new ADR to one of these categories based on its topic:

| Category | Topics |
|----------|--------|
| Core Architecture | Foundational technology choices, framework decisions, language/runtime choices |
| Exchange Integration | Exchange-specific modules, traits, instrument resolution, URL config |
| Persistence & Storage | Database choice, schema, migrations, retention policies |
| Metrics & Visualization | Prometheus, Grafana, dashboard layout, endpoint structure |
| Operations | Deployment, networking, signaling, reliability, workflow changes |

#### Upload steps

```bash
# Clone wiki repo
gh repo view --json name | xargs -I{} git clone https://github.com/fibonsai/{}.wiki.git /tmp/cryptomeria-wiki

# Write ADR file to wiki repo (using title as filename: ADR-<N>-<YYYYMMDD>-<short-title>.md)
cat > /tmp/cryptomeria-wiki/ADR-<N>-<YYYYMMDD>-<short-title>.md << 'EOF'
<ADR content>
EOF

# Add entry to Topic-Index.md in the appropriate category section
# File name in wiki, without path
# Inserted alphabetically by ADR number within the category

# If the ADR introduces a new category, add it to Topic-Index.md and _Sidebar.md

# Commit and push wiki changes
cd /tmp/cryptomeria-wiki
git add .
git commit -m "Add ADR-<N>: <short title>"
git push

# Clean up
rm -rf /tmp/cryptomeria-wiki
```

Do NOT create the ADR in `docs/`. The wiki is the canonical location.

### 11. Upload companion docs to GitHub Wiki

If the worktree contains companion markdown files beyond the ADR (e.g. CONTRIBUTIONS.md, CODE_OF_CONDUCT.md, SECURITY.md, LICENSE, `docs/*.md`), sync them to the wiki so sidebar links resolve.

```bash
# Identify companion markdown files created or modified in the worktree
# (exclude .opencode/, .claude/, node_modules/, .venv/)
COMPANION_FILES=$(git diff --name-only --diff-filter=ACM HEAD~1 HEAD -- '*.md' 'LICENSE' ':!.opencode/' ':!.claude/' 2>/dev/null || git ls-files --others --exclude-standard '*.md' 'LICENSE')

# If no companion files found (besides the ADR which is handled separately), skip
echo "$COMPANION_FILES" | grep -v -E 'ADR-|PLAN\.md' || { echo "No companion docs to sync"; exit 0; }

# Clone wiki repo
gh repo view --json name | xargs -I{} git clone "https://github.com/fibonsai/{}.wiki.git" /tmp/cryptomeria-wiki

# Copy each companion file to wiki
for FILE in $COMPANION_FILES; do
  case "$FILE" in
    LICENSE)
      # LICENSE has no .md extension — create LICENSE.md for wiki
      cp "$FILE" /tmp/cryptomeria-wiki/LICENSE.md
      ;;
    *.md)
      cp "$FILE" /tmp/cryptomeria-wiki/
      ;;
  esac
done

# Sync docs/ markdown files to wiki
if ls docs/*.md 2>/dev/null; then
  for DOC in docs/*.md; do
    cp "$DOC" /tmp/cryptomeria-wiki/
  done
fi

# Update _Sidebar.md if new top-level pages were added (governance, docs, etc.)
# This is a manual step — the agent must verify the sidebar reflects the new files

# Verify all sidebar links resolve to existing wiki pages
SIDEBAR_PAGES=$(grep -oP '\* \[\K[^\]]+' /tmp/cryptomeria-wiki/_Sidebar.md | grep -v '#' | tr ' ' '-')
BROKEN=""
for PAGE in $SIDEBAR_PAGES; do
  if [ ! -f "/tmp/cryptomeria-wiki/${PAGE}.md" ]; then
    BROKEN="$BROKEN $PAGE"
  fi
done
if [ -n "$BROKEN" ]; then
  echo "ERROR: Sidebar links without wiki pages:$BROKEN"
  echo "Create the missing pages before continuing."
  exit 1
fi
echo "OK: All sidebar links resolve to existing wiki pages"

# Commit and push wiki changes
cd /tmp/cryptomeria-wiki
git add .
git commit -m "Sync companion docs from repo"
git push

# Clean up
rm -rf /tmp/cryptomeria-wiki
```

### 12. Create a PR from exclusive branch

1. PR title is the same as the issue title.
2. PR body is a PLAN summary, explain what and how it fix the issue.
2. Add at ref to issue in PR body — do **not** append `🤖 Generated with [Claude Code](https://claude.com/claude-code)` or any other auto-attribution line
3. Execute /commit (check .opencode/commands/commit.md) in this branch

Do not merge the PR (it will be checked by a human or another agent).

### 13. Return to main branch

```bash
cd $PROJECT_ROOT
git worktree remove $WORKTREE
```

## Full workflow

```
check if in main branch → fetch remote main and create worktree → read issue → find latest plan → review plan → execute plan → update README + AGENTS.md → post changelog → update issue body/comments → delete PLAN.md → upload ADR to wiki → upload companion docs to wiki → verify sidebar links → create PR → close issue → return to main and remove worktree
```
