---
description: "Fetch a GitHub issue, investigate, plan, implement, test, and commit the fix"
argument-hint: "<issue-number>"
---

# Fix Issue Pipeline

## Phase 1: Fetch
```bash
gh issue view $ARGUMENTS --repo matiaszanolli/ByroRedux --json title,body,labels,state
```
Save details to `.claude/issues/$ARGUMENTS/ISSUE.md`.

## Phase 2: Classify Domain
Based on issue content, classify:
- **renderer** → Vulkan, pipeline, GPU memory, sync, shaders
- **ecs** → Storage, queries, world, systems, resources, components
- **platform** → Windowing, input, OS abstractions
- **nif** → NIF parsing, legacy format loading
- **cxx** → C++ interop
- **binary** → Game loop, scene setup, demo code

## Phase 3: Investigate
Read the relevant source files. Trace the code path.
If the issue spans multiple domains, consult specialist agents.
Save findings to `.claude/issues/$ARGUMENTS/INVESTIGATION.md`.

## Phase 4: Scope Check
If fix touches >5 files, pause and confirm with the user before proceeding.

## Phase 5: Implement
Follow project conventions from CLAUDE.md.
- Rust: cargo check + cargo test after changes
- Shaders: recompile SPIR-V if GLSL changed
- No new dependencies without user approval

## Phase 6: Verify
```bash
cargo test -p byroredux-core
cargo check
```
All 68+ tests must pass.

## Phase 7: Commit & Close
Commit with conventional message referencing the issue:
```
Fix #<number>: <description>
```
Close the issue: `gh issue close $ARGUMENTS --comment "Fixed in <commit-hash>"`
