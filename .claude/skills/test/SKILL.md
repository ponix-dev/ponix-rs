---
description: Run tests for the ponix-rs workspace. Use when the user says "run tests", "test", "run unit tests", "run integration tests", "watch tests", "test my changes", or any variation of executing the test suite. Accepts an optional argument to specify the test variant (unit, integration, all, watch, e2e).
user-invocable: true
---

# Test

Run the project's test suite. All commands use the mise tasks defined in `.mise.toml`.

## Steps

### 1. Determine Test Variant

If the user provided an argument or context clues, map it to a variant. Otherwise, ask which suite to run:

| Variant | Command | Requirements |
|---------|---------|--------------|
| **unit** (default) | `mise run test:unit` | None — fast, no Docker |
| **integration** | `mise run test:integration` | Docker running (uses testcontainers) |
| **all** | `mise run test:all` | Docker running |
| **watch** | `mise run test:watch` | None — continuous unit test feedback |
| **watch:integration** | `mise run test:integration:watch` | Docker running |
| **e2e** | `mise run test:e2e` | Running service — run `tilt ci` first to start the environment |

**Argument mapping examples:**
- `/test` → ask user or default to unit
- `/test unit` → unit tests
- `/test integration` → integration tests
- `/test all` → all tests
- `/test watch` → watch mode for unit tests
- `/test e2e` → end-to-end tests

### 2. Run Tests

Execute the selected command. For watch variants, inform the user that tests will re-run on file changes and they can stop with Ctrl+C.

### 3. Report Results

- On success: report the number of tests passed and total duration
- On failure: show the failing test names and output, then offer to investigate and fix the failures
