# TD8-001: Orphaned synchronous NPC-spawn compatibility wrappers with zero call sites

Severity: low
Source audit: docs/audits/AUDIT_TECH_DEBT_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2266

**Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
**Location**: `byroredux/src/npc_spawn.rs:716-746` (`spawn_npc_entity`), `:815-846` (`spawn_prebaked_npc_entity`)
**Status**: NEW

**Description**: Commit `9bf4c493` ("Implement resumable NPC assembly and enhance streaming architecture", 2026-07-27) introduced the resumable `NpcSpawnJob`/`NpcSpawnProgress` job API and, in the same commit, tagged the two older synchronous entry points `spawn_npc_entity` and `spawn_prebaked_npc_entity` with `#[allow(dead_code)]`. Their doc comments call them "compatibility entry points" that "drive the same `NpcSpawnJob` ... with an unlimited budget," implying a caller exists that wants synchronous, non-yielding NPC spawning. No such caller exists: `byroredux/src/cell_loader/references/mod.rs` (the only real spawn site, interior + exterior streaming) constructs `NpcSpawnJob::runtime(...)`/`NpcSpawnJob::prebaked(...)` directly and drives `job.advance(...)` itself.

**Evidence**: `grep -RIn "spawn_npc_entity(\|spawn_prebaked_npc_entity(" --include="*.rs" .` → matches only the two `fn` definitions themselves; every other hit in the repo is a doc-comment reference, none a call.

**Impact**: Two ~30-line `pub fn`s (`#[allow(clippy::too_many_arguments)]`, 12 parameters each) exist purely as unreachable API surface, including an untested `unreachable!()` branch inside ("unlimited budget cannot yield"). ByroRedux has no external consumers yet, so "compatibility entry point" framing doesn't apply — nothing to be compatible with.

**Suggested Fix**: Delete both functions and their now-orphaned doc-comment cross-references in `save_io.rs:1126/1143`, `ai_package.rs:69`, `pack.rs:1811`, `animation.rs:1354`, `systems.rs:433` (which just say "mirrors X" and don't need the dead symbol to exist). If a genuine future need for a synchronous unlimited-budget spawn resurfaces, re-derive it trivially from `NpcSpawnJob::runtime(...).advance(..., &mut FrameTimeBudget::unlimited())` at the call site.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix, if applicable
