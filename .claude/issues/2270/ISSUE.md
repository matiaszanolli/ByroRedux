# NEW-CONC-2: scripting's "snapshot before iterate" lock discipline is undocumented as a house rule

**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-08-03.md` (finding `NEW-CONC-2`, "documentation-only companion")
**Severity**: LOW
**Dimension**: ECS Lock Ordering & Deadlock
**Location**: `crates/scripting/src/{scene,package,dialogue}.rs` (pattern), no single doc location
**Labels applied**: `low`, `sync`, `documentation`

## Description

`scene_playback_system`, `scene_package_system`, and `scene_dialogue_system` all independently re-derive the same "snapshot resources/components to owned values before the per-entity loop, so no guard survives into a called helper" discipline that `physics_sync_system` and the M42 AI-package systems (`follow.rs`/`escort.rs`/etc., per the `#2134` fix) already established. It works today in every site checked, but it's tribal knowledge repeated by convention rather than a documented pattern a new contributor can be pointed at — exactly the gap that let `NEW-CONC-1` (#2269) slip through in the one place (`apply_effect`) where a nested acquisition genuinely happens instead of a snapshot-first collect.

## Evidence

No module-level or crate-level doc comment states the rule; each system's local comments (e.g. `scene.rs:772-775`, `package.rs` header) explain their own local reasoning without cross-referencing a shared convention.

## Impact

None today — purely a discoverability/consistency gap that raises the odds of a future system reintroducing a nested-lock pattern without recognizing the established alternative.

## Related

#2269 (`NEW-CONC-1`, the one site that didn't follow this convention).

## Suggested Fix

A short paragraph in `crates/core/src/ecs/world.rs`'s module docs or `CLAUDE.md`'s Critical Patterns section: "systems that call into shared per-effect/per-command helpers should snapshot resources to owned values before the loop, not hold guards across the call."
