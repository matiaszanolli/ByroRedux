---
name: source-command-trace-flow-skill
description: Trace a named or custom ByroRedux engine data flow, such as rendering, frame ticks, ECS queries, resize, or NIF import, and explain each boundary with source references.
---

# Trace Engine Flow

Read [the canonical trace-flow command](../../../.claude/commands/trace-flow/SKILL.md) and use the supplied flow name or description as `$ARGUMENTS`. Treat the built-in paths as starting points and verify each step against the current checkout.

Use the graph tools described in `AGENTS.md` to find the entry point, trace calls and data flow, and inspect exact symbols. Follow the canonical output format for both built-in and custom flows. Cite the source location and the data passed or transformed at each boundary.
