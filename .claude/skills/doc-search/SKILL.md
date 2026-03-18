---
description: Search and navigate project documentation in the docs/ folder. Use when the user says "find docs", "doc-search", "is there documentation for", "where is the doc about", "search docs", or any variation of looking up project documentation. Also use when the user asks how a component or crate works — check docs first before diving into source code, since there may already be a writeup that answers their question.
user-invocable: true
---

# Doc Search

Find and surface relevant documentation from `docs/`. This is a read-only skill — it searches but doesn't create or modify docs.

Checking docs first saves time because a well-maintained doc gives you architecture context, key types, and design rationale in one place — rather than piecing it together from scattered source files.

## Search Strategies

Try these in order, stopping as soon as you find a match:

### 1. By Crate

If the user is asking about a specific crate, check directly with the Read tool:

```
docs/modules/{crate-name}.md
```

Where `{crate-name}` is the crate directory name with underscores converted to hyphens (e.g., `analytics_worker` → `analytics-worker`).

### 2. By Topic

Use the Grep tool to search `title` and `description` frontmatter fields across all docs:

- Pattern: the user's topic keywords
- Path: `docs/`
- Glob: `**/*.md`

### 3. By Source File

If the user is asking about a specific source file, use the Grep tool to search `related-files` entries:

- Pattern: the file path (or a distinctive part of it)
- Path: `docs/`
- Glob: `**/*.md`

### 4. By Category

Use the Glob tool to list all docs in the relevant category:

- `docs/architecture/*.md`
- `docs/modules/*.md`
- `docs/data-flows/*.md`
- `docs/operations/*.md`

### 5. Full Index

Read `docs/INDEX.md` for the complete listing with descriptions.

## When No Doc Exists

If no documentation is found for the user's query, let them know and suggest using `/doc-format` to create one. Mention the category that would be most appropriate.
