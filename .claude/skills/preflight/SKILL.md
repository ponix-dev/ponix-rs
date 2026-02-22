---
description: Run pre-flight quality checks before pushing or creating a PR. Use when the user says "run checks", "preflight", "pre-flight", "ready to push", "check my code", "lint", "format check", or any variation of verifying code quality before submission.
user-invocable: true
---

# Preflight

Run formatting, linting, and unit test checks sequentially. Fix any failures before moving to the next check.

## Steps

### 1. Format Check

```bash
mise run fmt:check
```

If formatting errors are found, fix them:

```bash
cargo fmt --all
```

Then re-run `mise run fmt:check` to confirm.

### 2. Clippy Check

```bash
mise run clippy:check
```

If clippy warnings or errors are found, fix the code issues, then re-run the check.

### 3. Unit Tests

```bash
mise run test:unit
```

If tests fail, fix the failing tests or underlying code, then re-run.

### 4. Report

Present a summary to the user:

```
Pre-flight checks:
- Format:  PASS/FAIL
- Clippy:  PASS/FAIL
- Tests:   PASS/FAIL
```

If all checks pass, let the user know they're clear to push or create a PR.
