#!/usr/bin/env bash
# Shared helpers for the shell-based e2e test scripts (test-e2e.sh,
# test-e2e-skills.sh). Source this file; do not run it directly.

# Resolve the repo root from the sourcing script and set common paths.
E2E_REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[1]}")/.." && pwd)"
cd "$E2E_REPO_ROOT"

BINARY="${BINARY:-target/release/imrule}"
TMP="${TMP:-/tmp/imrule-e2e}"

# assert "description" command...
# PASS when the command succeeds; prints FAIL and exits the script otherwise.
assert() {
    local description="$1"
    shift
    if "$@"; then
        echo "[PASS] $description"
    else
        echo "[FAIL] $description"
        exit 1
    fi
}

# assert_not "description" command...
# PASS when the command fails — e.g. `test -f` for a file that must not exist.
assert_not() {
    local description="$1"
    shift
    if "$@"; then
        echo "[FAIL] $description"
        exit 1
    else
        echo "[PASS] $description"
    fi
}

# reset_dir <name> — recreate $TMP/<name> as an empty directory.
reset_dir() {
    rm -rf "$TMP/$1" && mkdir -p "$TMP/$1"
}

# fresh_fixture <name> — reset $TMP/<name> to a fresh copy of $TEST_DIR.
fresh_fixture() {
    rm -rf "$TMP/$1" && cp -r "$TEST_DIR" "$TMP/$1"
}

# scaffold_rules <name> — reset $TMP/<name> with a minimal .imrule/AGENTS.md.
scaffold_rules() {
    reset_dir "$1"
    mkdir -p "$TMP/$1/.imrule"
    printf "# Rules\n" > "$TMP/$1/.imrule/AGENTS.md"
}
