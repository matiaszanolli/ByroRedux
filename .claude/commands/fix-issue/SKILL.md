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
Classify the issue — the matched crate becomes the `-p` test target in Phase 6:
- **renderer** → `byroredux-renderer` — Vulkan, pipeline, GPU memory, sync, shaders, RT acceleration
- **ecs** → `byroredux-core` — Storage, queries, world, systems, resources, components
- **nif** → `byroredux-nif` — NIF parsing, block types, version handling, import pipeline
- **animation** → `byroredux-core` (+ `byroredux-nif` for KF import) — interpolation, blending, animation system
- **bsa** → `byroredux-bsa` — BSA/BA2 archive reading, extraction, decompression
- **esm** → `byroredux-plugin` — ESM record parsing, cell loading, XCLL lighting
- **platform** → `byroredux-platform` — Windowing, input, OS abstractions
- **cxx** → `byroredux-cxx-bridge` — C++ interop
- **binary** → `byroredux` — Game loop, scene setup, demo code

## Phase 3: Investigate
Read only the source files on the code path — don't pre-read a whole crate, and
don't re-read files already in context. Trace the path inline.

**Specialist agents are a last resort, not a default.** Each one is a fresh
context window — only delegate when the issue genuinely spans 2+ domains *and*
you can't trace it yourself. When you do, ask the agent for a conclusion
(file:line + cause), not file dumps.

**INVESTIGATION.md is optional.** Skip it for single-site fixes — the commit body
covers those. Write it only when the investigation uncovered non-obvious findings
worth preserving (cross-file interactions, a wrong-looking-but-correct invariant).

## Phase 4: Scope Check
If fix touches >5 files, pause and confirm with the user before proceeding.

## Phase 5: Implement
Follow project conventions from CLAUDE.md.
- Shaders: recompile SPIR-V if GLSL changed
- No new dependencies without user approval
- **Inner loop = `cargo check -p <crate>`** (the Phase 2 crate), not the full
  workspace. It's the fastest signal while iterating. Do *not* run `cargo test`
  on every edit — save it for Phase 6.

## Phase 6: Verify
Scope first, widen only at the end. Keep output quiet so test logs don't flood
context — `-q` prints dots for passes and full detail only for failures.

```bash
cargo test -q -p <crate>          # the Phase 2 crate — the only suite that can regress for a scoped fix
cargo test -q                     # full workspace — ONCE, final gate (skip the redundant `cargo check`: test already built everything)
```
All tests must pass with zero warnings. Run the full suite a single time at the
end, not per fix iteration. For a fix confined to one crate with no cross-crate
surface, the scoped run is sufficient evidence — note that you scoped it.

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
