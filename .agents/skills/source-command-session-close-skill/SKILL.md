---
name: source-command-session-close-skill
description: Close out a ByroRedux work session by checking project statistics and issue traceability and proposing coordinated HISTORY.md, ROADMAP.md, and README.md edits. Use when the user requests a repository session closeout.
---

# Session Close

Read [the canonical session-close command](../../../.claude/commands/session-close/SKILL.md) and use any `--since <commit>` argument as `$ARGUMENTS`. Follow its session-boundary checks, ground-truth collection, and unified diff review. Use the pinned toolchain in `AGENTS.md` and resolve `/audit-incremental` to `.claude/commands/audit-incremental/SKILL.md` when that workflow is relevant.

Apply the documentation diff after the user accepts it. Use the user's authorization for commits, GitHub comments or closures, pushes, and release tags.
