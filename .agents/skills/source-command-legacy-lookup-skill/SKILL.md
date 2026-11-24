---
name: source-command-legacy-lookup-skill
description: Look up a Gamebryo 2.3 class, function, or pattern in the local legacy source and map it to ByroRedux. Use when the user asks for an original-engine source lookup.
---

# Legacy Gamebryo Lookup

Read [the canonical legacy-lookup command](../../../.claude/commands/legacy-lookup/SKILL.md) and use the requested symbol or pattern as `$ARGUMENTS`. Follow its header-first lookup and report the class hierarchy, relevant methods, serialization macros, and ByroRedux equivalents.

Use `docs/legacy/key-files.md` to locate the legacy tree if the command's path is unavailable. For the ByroRedux cross-reference, follow the graph-first code discovery instructions in `AGENTS.md`. If only repository documentation is available for the original engine, identify which claims it supports and which still need source verification.
