#!/usr/bin/env bash
# End-to-end tests for `imrule skills` against the release binary, including a
# remote GitHub source. Run through `make test-e2e-skills`.
set -euo pipefail

source "$(dirname "$0")/e2e-lib.sh"

SKILLS_FIXTURE="${SKILLS_FIXTURE:-test-e2e-skills/fixture-local-source}"

echo ""; echo "━━━ E2E Skills: imrule skills add (local source) ━━━"
rm -rf "$TMP/skills-xdg"
scaffold_rules skills-local
printf "# ImRule Configuration File\n" > "$TMP/skills-local/.imrule/imrule.toml"
XDG_CONFIG_HOME="$TMP/skills-xdg" "$BINARY" skills add "$SKILLS_FIXTURE" --project-root "$TMP/skills-local"
assert "sample-skill installed to .imrule/skills" test -d "$TMP/skills-local/.imrule/skills/sample-skill"
assert "sample-skill has SKILL.md" test -f "$TMP/skills-local/.imrule/skills/sample-skill/SKILL.md"
assert "another-skill installed to .imrule/skills" test -d "$TMP/skills-local/.imrule/skills/another-skill"

echo ""; echo "━━━ E2E Skills: skill propagated to agent dirs ━━━"
assert "sample-skill propagated to .claude/skills" test -d "$TMP/skills-local/.claude/skills/sample-skill"
assert "sample-skill propagated to .codex/skills" test -d "$TMP/skills-local/.codex/skills/sample-skill"

echo ""; echo "━━━ E2E Skills: imrule skills add --list ━━━"
reset_dir skills-list
XDG_CONFIG_HOME="$TMP/skills-xdg" "$BINARY" skills add "$SKILLS_FIXTURE" --list --project-root "$TMP/skills-list" > "$TMP/skills-list/out.txt"
assert "--list shows sample-skill" grep -q "sample-skill" "$TMP/skills-list/out.txt"
assert_not "--list did not install" test -d "$TMP/skills-list/.imrule/skills"

echo ""; echo "━━━ E2E Skills: imrule skills add --skill (selective) ━━━"
scaffold_rules skills-filter
XDG_CONFIG_HOME="$TMP/skills-xdg" "$BINARY" skills add "$SKILLS_FIXTURE" --skill sample-skill --project-root "$TMP/skills-filter"
assert "sample-skill installed" test -d "$TMP/skills-filter/.imrule/skills/sample-skill"
assert_not "another-skill NOT installed (--skill filter)" test -d "$TMP/skills-filter/.imrule/skills/another-skill"

echo ""; echo "━━━ E2E Skills: imrule skills list ━━━"
XDG_CONFIG_HOME="$TMP/skills-xdg" "$BINARY" skills list --project-root "$TMP/skills-local" > "$TMP/skills-local/list.txt"
assert "skills list shows installed skill" grep -q "sample-skill" "$TMP/skills-local/list.txt"

echo ""; echo "━━━ E2E Skills: imrule skills add (remote GitHub source) ━━━"
scaffold_rules skills-remote
XDG_CONFIG_HOME="$TMP/skills-xdg" "$BINARY" skills add vercel-labs/agent-skills --project-root "$TMP/skills-remote" > "$TMP/skills-remote/log.txt" 2>&1
assert "remote skills installed" grep -q "Installed" "$TMP/skills-remote/log.txt"
assert ".imrule/skills directory created from remote" test -d "$TMP/skills-remote/.imrule/skills"

echo ""; echo "━━━ E2E Skills: all skills tests passed ━━━"
