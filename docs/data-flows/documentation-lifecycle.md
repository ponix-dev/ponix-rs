---
title: Documentation Lifecycle
description: How project documentation is created, discovered, and kept in sync with code changes through Claude Code skills.
category: data-flow
related-files:
  - .claude/skills/doc-format/SKILL.md
  - .claude/skills/doc-search/SKILL.md
  - .claude/skills/doc-gardening/SKILL.md
  - .claude/skills/preflight/SKILL.md
  - docs/INDEX.md
  - CLAUDE.md
last-updated: 2026-03-18
---

# Documentation Lifecycle

The documentation system uses three Claude Code skills to create, find, and maintain docs as the codebase evolves. This flow ensures docs stay accurate without requiring developers to remember to update them — the preflight checks catch staleness automatically before code ships.

## Overview

Documentation flows through four stages: **authoring** (creating or updating a doc), **discovery** (finding relevant docs), **gardening** (syncing docs with code changes), and **enforcement** (preflight checks before PR). Each stage is handled by a dedicated skill, and they compose together into a self-maintaining system.

## How It Works

```
                    ┌──────────────────────────┐
                    │  Developer writes code   │
                    └────────────┬─────────────┘
                                 │
              ┌──────────────────┼──────────────────┐
              │                  │                  │
     Wants to document    Has a question     Ready to push
              │                  │                  │
     ┌────────▼────────┐ ┌──────▼───────┐ ┌───────▼────────┐
     │  /doc-format    │ │ /doc-search  │ │  /preflight    │
     │                 │ │              │ │                │
     │ 1. Pick category│ │ 1. By crate  │ │ 1. fmt:check   │
     │ 2. Derive path  │ │ 2. By topic  │ │ 2. clippy      │
     │ 3. Read source  │ │ 3. By file   │ │ 3. unit tests  │
     │ 4. Write doc    │ │ 4. By category│ │ 4. doc-gardening│
     │ 5. Update INDEX │ │ 5. Full index│ │ 5. Report      │
     └────────┬────────┘ └──────┬───────┘ └───────┬────────┘
              │                  │                  │
              │           Found? ──No──→ Suggest    │
              │                          /doc-format │
              │                                     │
              │                          ┌──────────▼──────────┐
              │                          │  /doc-gardening     │
              │                          │                     │
              │                          │ 1. git diff main    │
              │                          │ 2. Map files→docs   │
              │                          │ 3. Analyze changes  │
              │                          │ 4. Update stale docs│
              │                          │ 5. Flag undocumented│
              │                          │ 6. Reconcile INDEX  │
              │                          │ 7. Report           │
              │                          └──────────┬──────────┘
              │                                     │
              ▼                                     ▼
     ┌─────────────────────────────────────────────────┐
     │                  docs/                           │
     │  INDEX.md ──────────────────────────────────┐    │
     │  architecture/{concept-slug}.md             │    │
     │  modules/{crate-name}.md        ◄───────────┘    │
     │  data-flows/{flow-slug}.md      (linked)         │
     │  operations/{task-slug}.md                       │
     └─────────────────────────────────────────────────┘
```

### 1. Authoring (`/doc-format`)

When a developer or Claude wants to document something:

1. **Determine category** — Is this a module (single crate), architecture (cross-cutting concept), data-flow (end-to-end path), or operation (how-to guide)?
2. **Derive file path** — Category determines the directory; the name follows a predictable pattern (e.g., `crates/analytics_worker/` → `docs/modules/analytics-worker.md`).
3. **Research source code** — Read the relevant crate to understand key types, design decisions, and how things connect.
4. **Fill the template** — Every doc uses the same frontmatter structure (`title`, `description`, `category`, `related-files`, `last-updated`) and section headings (`Overview`, `Key Concepts`, `How It Works`, etc.).
5. **Update INDEX.md** — Add a row to the appropriate category table so the doc is discoverable.

The canonical template ensures consistency — any doc can be found by its predictable path, and frontmatter enables programmatic search.

### 2. Discovery (`/doc-search`)

When someone needs information, the search follows a priority order:

1. **By crate** — Check `docs/modules/{crate-name}.md` directly (fastest path)
2. **By topic** — Grep `title` and `description` frontmatter across all docs
3. **By source file** — Grep `related-files` entries to find docs that reference a specific file
4. **By category** — List all docs in a category directory
5. **Full index** — Read `docs/INDEX.md` for the complete listing

If no doc exists, the skill suggests running `/doc-format` to create one, mentioning the appropriate category.

### 3. Gardening (`/doc-gardening`)

When code changes on a branch, docs may become stale. The gardening skill:

1. **Identifies changed files** — `git diff main...HEAD --name-only`, filtered to `crates/**/*.rs`
2. **Maps changes to docs** — By crate name (e.g., `crates/foo_bar/` → `docs/modules/foo-bar.md`) and by `related-files` frontmatter references
3. **Analyzes diffs** — Looks for new types/traits, changed signatures, new modules, config changes
4. **Updates stale docs** — Refreshes content and `last-updated` date for affected docs
5. **Flags undocumented crates** — Reports crates with changes that have no module doc, but doesn't auto-generate (creating thorough docs requires focused attention)
6. **Reconciles INDEX.md** — Ensures the index matches docs on disk

### 4. Enforcement (`/preflight`)

Doc gardening runs automatically as step 4 of the preflight checks, after format, clippy, and tests. This means stale docs are caught before a PR is created — the developer sees a report like:

```
Pre-flight checks:
- Format:        PASS
- Clippy:        PASS
- Tests:         PASS
- Doc Gardening: 2 updated, 0 created, 1 warning (analytics-worker.md references deleted file)
```

### Key Design Decisions

**Why frontmatter?** The YAML frontmatter (`title`, `description`, `category`, `related-files`, `last-updated`) enables programmatic discovery. `/doc-search` greps frontmatter fields to find relevant docs without reading full file contents. `related-files` creates a bidirectional link between source code and documentation.

**Why predictable file paths?** Module docs use the crate directory name with underscores converted to hyphens. This means you can find any crate's doc without consulting the index — just check `docs/modules/{crate-name}.md`. The convention removes guesswork.

**Why flag instead of auto-generate?** Doc gardening flags undocumented crates but doesn't auto-generate full module docs. Creating useful documentation requires reading source code, understanding design decisions, and writing for the right audience. A skeleton doc with wrong or shallow content is worse than no doc at all.

**Why integrate with preflight?** Making doc gardening part of the standard pre-push workflow ensures docs stay in sync as a side effect of normal development, not as a separate task that gets forgotten.

## Related Documentation

- [docs/INDEX.md](../INDEX.md) — Master index of all documentation
- [CLAUDE.md](../../CLAUDE.md) — Project-level documentation section with skill references
