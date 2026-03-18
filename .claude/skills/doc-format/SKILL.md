---
description: Create or update project documentation in the docs/ folder using the canonical template. Use when the user says "document this", "write docs for", "doc-format", "create a doc", "add documentation", or any variation of writing or formatting project documentation. Also trigger when another skill (like doc-gardening) needs to create or update a doc — this skill defines the template and storage rules.
user-invocable: true
---

# Doc Format

Create or update a documentation file in `docs/` using the project's canonical template. Every doc follows the same structure so they're predictable and greppable by both humans and AI agents — consistency here means any developer (or Claude in a future conversation) can find what they need without guessing at file names or section headings.

## Template

All docs use this structure:

```markdown
---
title: <Human-readable title>
description: <One-line summary — this appears in INDEX.md>
crate: <crate name if module-specific — omit for cross-cutting docs>
category: <architecture | module | data-flow | operation>
related-files:
  - <path relative to repo root>
last-updated: <YYYY-MM-DD>
---

# <Title>
<What this is and why it exists — 2-3 sentences>

## Overview
<Role in the system, key responsibilities>

## Key Concepts
<3-7 most important types/traits/abstractions with one-line explanations>

## How It Works
<Detailed walkthrough — numbered steps, ASCII diagrams for data flows>

## Configuration
<Env vars, config fields — only if applicable>

## Related Documentation
<Links to other docs in this folder>
```

Omit sections that don't apply (e.g., skip Configuration for a data-flow doc that has no config).

## File Path and Storage Rules

Each category maps to a directory, and the file name follows a predictable pattern so docs are discoverable without consulting the index:

| Category | Directory | File name pattern | Example |
|----------|-----------|-------------------|---------|
| `module` | `docs/modules/` | `{crate-name}.md` (matches directory name under `crates/`) | `docs/modules/collaboration-server.md` |
| `architecture` | `docs/architecture/` | `{concept-slug}.md` (kebab-case concept name) | `docs/architecture/runner-pattern.md` |
| `data-flow` | `docs/data-flows/` | `{flow-slug}.md` (kebab-case flow name) | `docs/data-flows/envelope-pipeline.md` |
| `operation` | `docs/operations/` | `{task-slug}.md` (kebab-case task name) | `docs/operations/adding-a-new-worker.md` |

A few conventions that keep things tidy:
- All docs live under `docs/` — this makes searching fast and avoids docs getting lost in crate directories
- File names use kebab-case (lowercase, hyphens) — module docs convert the crate directory name's underscores to hyphens (e.g., `crates/analytics_worker/` → `docs/modules/analytics-worker.md`)
- One doc per file — if two topics are related but distinct, link between them rather than combining
- The `category` frontmatter field matches the subdirectory — this is how `/doc-search` maps topics to files

## Steps

### 1. Determine Category

Based on what's being documented:
- **module**: Documents a single crate's internals, key types, design decisions
- **architecture**: Cross-cutting concept that spans multiple crates (e.g., runner pattern, error handling strategy)
- **data-flow**: End-to-end path through the system (e.g., envelope ingestion pipeline, CDC event flow)
- **operation**: Procedural "how to" guide (e.g., adding a new worker, setting up local dev)

### 2. Derive File Path

Use the storage rules table above. For module docs, look up the crate's directory name under `crates/` and convert underscores to hyphens.

### 3. Research and Write

Read the relevant source files to understand the component. Fill in the template sections with accurate, specific content. Focus on the "why" and design decisions — the code already shows the "what".

For `related-files`, list the most important source files (not every file, just the ones someone would need to read to understand the component).

### 4. Update INDEX.md

Add a row to the appropriate table in `docs/INDEX.md`:

```markdown
| [Title](category/filename.md) | One-line description |
```

Replace the "*(none yet)*" placeholder row if it's the first doc in that category.
