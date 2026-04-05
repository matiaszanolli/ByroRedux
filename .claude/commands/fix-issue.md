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
- **renderer** → Vulkan, pipeline, GPU memory, sync, shaders, RT acceleration
- **ecs** → Storage, queries, world, systems, resources, components
- **nif** → NIF parsing, block types, version handling, import pipeline
- **animation** → Keyframe parsing, interpolation, blending, animation system
- **bsa** → BSA/BA2 archive reading, extraction, decompression
- **esm** → ESM record parsing, cell loading, XCLL lighting
- **platform** → Windowing, input, OS abstractions
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
cargo test
cargo check
```
All 312+ tests must pass. Zero warnings.

## Phase 7: Completeness Checks
Before committing, verify:
- [ ] **UNSAFE**: Any new unsafe blocks have safety comments
- [ ] **SIBLING**: Same bug pattern checked in related files
- [ ] **DROP**: Vulkan object lifecycle correct (destroy-before-parent)
- [ ] **TESTS**: Regression test added if applicable

## Phase 8: Commit & Close
Commit with conventional message referencing the issue:
```
Fix #<number>: <description>
```
Close the issue:
```bash
gh issue close $ARGUMENTS --repo matiaszanolli/ByroRedux --comment "Fixed in <commit-hash>"
```
