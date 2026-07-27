# ADR-027: Documentation Refactoring — Extract README Sections to Dedicated Docs Files

## Context

The README had grown to 546 lines with five substantial reference sections embedded inline:
- "Project Structure" (46 lines, ASCII tree + module descriptions)
- "QuestDB Persistence" (80 lines, SQL schema + parameter tables)
- "Exchange Comparison" (100 lines, comparison tables + delivery model diagrams + pros/cons)
- "Grafana LOB Visualization" (60 lines, architecture diagram + metrics reference)
- "LOB Data Processing" (27 lines, processing rules + CLI)

These sections are reference material, not onboarding content. They also duplicated information already present in ADRs (e.g., QuestDB schema in ADR-003/004/008, Exchange Comparison in ADR-015/017/018, Grafana in ADR-006/009/011).

## Options Considered

1. **Keep in README** — no change
2. **Move to `docs/` as separate files** — each section becomes its own markdown file
3. **Move to GitHub Wiki only** — remove from repo, keep only in wiki
4. **Delete reference sections** — rely on ADRs + code

## Decision

Option 2: Extract each section to a dedicated file in `docs/`:
- `docs/project-structure.md`
- `docs/questdb-persistence.md`
- `docs/exchange-comparison.md`
- `docs/grafana-lob-visualization.md`
- `docs/lob-data-processing.md`

Replace each in README with a 2–3 sentence intro + link. Add a "Documentation" reference table at the end of README linking all docs files and the Wiki Topic Index.

Create `docs/documentation-topic-index.md` as a single entry point listing all docs + ADRs by category.

Upload all docs + ADRs to GitHub Wiki, create `Topic-Index.md` page, update `_Sidebar.md` and `Home.md` with links.

## Consequences

### Positive
- README shrinks from 546 → 256 lines (53% reduction), focuses on onboarding
- Each topic has room to grow with better formatting (Mermaid diagrams, tables)
- ADRs remain the canonical architecture decisions; docs become user-facing guides
- GitHub Wiki provides browsable version for non-developers
- Single topic index (`documentation-topic-index.md`) for repo + wiki mirror

### Negative
- Two places to update (repo `docs/` + GitHub Wiki) — mitigated by wiki being a git repo
- Links in README now relative (`docs/file.md`) — works on GitHub, may need adjustment for other render differently elsewhere

## Status

Accepted

## Related

- Issue #117
- PR #118
- ADR-006 (Grafana LOB Visualization)
- ADR-003, ADR-004, ADR-008 (QuestDB)
- ADR-015, ADR-017, ADR-018 (Exchanges)