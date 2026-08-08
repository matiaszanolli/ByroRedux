# TD1-005: crates/scripting/src/scene.rs newly crossed 2000 LOC via diffuse quest/scene-lifecycle growth

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2408
**Finding ID**: TD1-005 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 1 — File / Function / Module Complexity
**Location**: `crates/scripting/src/scene.rs:1-1335` (production), `:1336-2375` (tests)
**Status**: NEW

## Description
`crates/scripting/src/scene.rs` newly crossed 2000 LOC via diffuse quest/scene-lifecycle growth over 5 commits in 8 days (SCEN playback, PACK actions, save/load registration, #2295 cross-ref, quest-alias/lifecycle + observability work) — not a monolithic dump. None of its functions exceed complexity 25, but the file now combines 4 responsibilities that arrived as separate features: scene registry/playback, quest-alias injection, package-action execution, and actor-binding resolution.

## Related
Same inline-test-bulk pattern as `save_io.rs`, plus #2295.

## Suggested Fix
(1) Extract `mod tests` to per-topic sibling files. (2) Split production code along the commit-history-revealed boundaries: `scene_playback.rs`, `quest_alias.rs`, `package_action.rs`.

## Completeness Checks
- [ ] **TESTS**: All extracted tests still compile and pass unchanged after any split
- [ ] **SIBLING**: Confirm no other file in `crates/scripting/src/` has absorbed similar multi-responsibility growth this session
