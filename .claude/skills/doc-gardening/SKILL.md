---
description: Analyze code changes on the current branch and update project documentation to stay in sync. Use when the user says "doc-gardening", "update docs", "sync docs", "are docs up to date", "check documentation", or any variation of ensuring documentation reflects recent code changes. Also triggered automatically as part of preflight checks.
user-invocable: true
---

# Doc Gardening

Analyze code changes on the current branch vs main and update documentation to stay in sync. This keeps docs accurate without requiring manual effort after every code change — stale docs are worse than no docs because they actively mislead.

## Steps

### 1. Identify Changed Source Files

```bash
git diff main...HEAD --name-only
```

Filter to `crates/**/*.rs` files — these are the ones that might affect documentation.

If there are no Rust source file changes, skip to step 6 (just reconcile the index).

### 2. Map Changes to Existing Docs

For each changed file, find docs that reference it:

- **By crate name**: If files in `crates/foo_bar/` changed, check `docs/modules/foo-bar.md`
- **By `related-files` frontmatter**: Use the Grep tool to search across `docs/**/*.md` for the changed file paths

Build a list of docs that need review.

### 3. Analyze What Changed

For each affected doc, examine the diffs in its related source files. Look for:

- New public types, traits, or functions
- Changed function signatures or trait definitions
- New or removed modules (`mod` declarations)
- Configuration changes (new env vars, config fields)
- Architectural changes (new dependencies, changed data flow)

### 4. Update Existing Docs

For each doc whose related files changed:

- Update content to reflect the changes found in step 3
- Update the `last-updated` field to today's date
- Add any new `related-files` entries if the changes introduced new important files
- Follow the template defined by `/doc-format`

### 5. Flag Undocumented Crates

Check if any changed files belong to crates that don't have a module doc yet:

```
crates/{crate_name}/ has changes but docs/modules/{crate-name}.md doesn't exist
```

Report these as candidates for new docs in the summary. Don't auto-generate full docs here — creating a thorough module doc requires focused attention, so flag it for the user to decide whether to run `/doc-format` for each one.

### 6. Reconcile INDEX.md

Ensure `docs/INDEX.md` accurately reflects all docs on disk:

- Use the Glob tool to list all `*.md` files in `docs/` subdirectories (excluding INDEX.md itself)
- Compare against the entries in INDEX.md
- Add missing entries, remove stale entries
- Read each doc's frontmatter for the title and description to populate the table

### 7. Report

Present a summary:

```
Doc Gardening:
- Updated: X docs (list names)
- Unchanged: Z docs
- Undocumented crates with changes: (list crate names — run /doc-format to create)
- Warnings: (any issues found, e.g., docs referencing deleted files)
```
