# #5472: TD3-2026-10-08-03: The window's split wave (#5092 / #5093 / #5311) left docs and comments citing deleted files and stale line ranges

**Labels**: low,tech-debt,documentation,doc-rot
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5472

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-08.md` — `TD3-2026-10-08-03` (HEAD `00f580e09`)

**Publish note**: Same class as OPEN #5313 but disjoint sites (from the #5092/#5093/#5311 splits that landed after #5313 was filed) — filed separately, not a duplicate.

- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**:
  - `docs/engine/physics.md:70`: the module tree still lists `world.rs` as "PhysicsWorld resource + KCC move_character /
    cast_ray_down helpers". It is now `world/{mod,queries,recovery}.rs`, and `move_character` is at `world/queries.rs:594`.
  - `crates/physics/src/config.rs:6,19,145` cite `world.rs::PhysicsWorld::move_character` and "`world.rs:285`". The last
    is inside an `assert_eq!` message.
  - `docs/engine/npc-spawn-ai-packages.md:141`: "`crates/plugin/src/esm/records/actor/mod.rs:190-191`, from `PKID`". The
    field and its parse arm moved to `actor/npc.rs:224` / `:637`.
  - `docs/engine/per-game-translation-survey.md:212,241`: link the NPC_ attribute and RACE DATA decoders to
    `records/actor/mod.rs`, which no longer contains them (they are in `npc.rs` / `race.rs` / `class.rs`).
  - `docs/engine/per-game-translation-survey.md:243`: cites `records/actor/mod.rs:1225`. The arm is at `actor/race.rs:415`.
  - `docs/engine/exterior-readiness-plan.md:303`: cites `streaming.rs:719-729`.
  - About 9 code comments still name `streaming.rs` as the exterior loader, which is now `streaming/mod.rs` and
    `streaming/pre_parse.rs`: `crates/plugin/src/esm/cell/mod.rs:427`, `byroredux/src/cell_loader/lod_bands.rs:95`,
    `byroredux/src/cell_loader/references/synth_child.rs:569,573`, `byroredux/src/cell_loader/nif_import_registry.rs:43`,
    `byroredux/src/scene/nif_loader.rs:323`, `byroredux/src/cell_loader/object_lod.rs:1102`,
    `crates/core/src/math/coord.rs:38`.
- **Status**: NEW. This is the same class as OPEN #5313, which holds the previous wave's instances. These are new
  instances from splits that landed after it was filed.
- **Effort**: trivial
- **Description**:
  - `_audit-validate.sh` resolves backticked paths only in skills and `docs/engine`.
  - The physics.md tree row is not backticked, and line-range cites into a file that still exists (`actor/mod.rs`) resolve.
  - So none of the above is gated.
- **Related**: #5313 (OPEN), TD4-2026-10-08-01.
- **Suggested Fix**: re-point each site. For the `streaming.rs` comments, say "the exterior streaming module
  (`streaming/`)".

## Completeness Checks
- [ ] **SIBLING**: Remaining `streaming.rs` / `world.rs` / `actor/mod.rs:<line>` cites swept repo-wide (`grep -rn 'streaming\.rs\|physics/src/world\.rs\|actor/mod\.rs:'`)
