---
name: source-command-fix-issue-skill
description: Run the ByroRedux fix-issue workflow for a GitHub issue number, from reading the issue and tracing the bug through implementing and verifying the fix. Use when the user asks to fix a ByroRedux issue by number.
---

# Fix Issue

Read [the canonical fix-issue command](../../../.claude/commands/fix-issue/SKILL.md) and follow its phases. Treat the issue number supplied by the user as `$ARGUMENTS`. Check for the curated local issue record before fetching the issue from GitHub.

Use `AGENTS.md` for Codex project conventions, including graph-first code discovery and crate-specific test requirements. Resolve `/audit-publish` to `.claude/commands/audit-publish/SKILL.md` when that workflow is relevant. The user's authorization determines whether to commit the fix or close the GitHub issue.
