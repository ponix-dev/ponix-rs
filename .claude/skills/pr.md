---
description: Use when the user asks to create a PR, open a pull request, submit changes for review, or says "make a PR". Runs format, clippy, and test checks before creating the pull request.
user-invocable: true
---

# Create Pull Request

Create a pull request after running all pre-flight quality checks. Fix any issues found before proceeding.

## Steps

### 1. Pre-flight Checks

Run the following checks sequentially. If any check fails, fix the issues and re-run that check before moving on.

**Format check:**
```bash
mise run fmt:check
```
If formatting errors are found, fix them by running `cargo fmt --all`, then re-run the check.

**Clippy check:**
```bash
mise run clippy:check
```
If clippy warnings or errors are found, fix the code issues, then re-run the check.

**Unit tests:**
```bash
mise run test:unit
```
If tests fail, fix the failing tests or underlying code, then re-run.

### 2. Gather Context

- Run `git status` to see all changed files (never use `-uall` flag)
- Run `git diff` to see staged and unstaged changes
- Run `git log` to understand recent commit style
- Run `git diff main...HEAD` (or the appropriate base branch) to see the full diff for the PR

### 3. Create the Pull Request

Use the PR template format from `.github/PULL_REQUEST_TEMPLATE.md`. Fill in all sections based on the changes.

The PR title should follow conventional commit format and be under 70 characters:
- `feat: add device validation endpoint`
- `fix: resolve timeout in CDC worker`
- `refactor: extract shared gRPC middleware`
- `chore: update dependencies`

Create the PR using the `gh` CLI:

```bash
gh pr create --title "<conventional commit style title>" --body "$(cat <<'EOF'
## Summary

<1-3 sentence description of what this PR does and why>

## Changes

<bulleted list of key changes>

## Type of Change

- [x] `<type>`: <description>

## Test Plan

- [x] Unit tests pass (`mise run test:unit`)
- [x] Format check passes (`mise run fmt:check`)
- [x] Clippy check passes (`mise run clippy:check`)

## Additional Context

<any extra context or notes>
EOF
)"
```

Make sure to push the branch with `git push -u origin <branch-name>` before creating the PR.

### 4. Report

Return the PR URL to the user when done.
