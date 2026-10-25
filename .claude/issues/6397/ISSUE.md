# TD8-2026-10-25-04: three pub items with zero callers

Labels: low, scripting, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6397
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Effort**: trivial · **Status**: NEW
- `crates/scripting/src/fragment/populate.rs:502` `populate_scene_fragments_from_pex_detailed` — only mention outside its body is the re-export `crates/scripting/src/lib.rs:163`. Production uses `populate_owned_scene_fragments_from_pex_detailed_with_providers` (`byroredux/src/asset_provider/script.rs:635`). #6189 deleted this one's non-`_detailed` sibling (`populate_scene_fragments_from_pex`) but left the `_detailed` wrapper behind (added f5127c1cc, 2026-09-10). Delete fn + `lib.rs:163` name. (The quest twin `populate_quest_fragments_from_pex_detailed` is still called by `populate_quest_fragments_from_pex`.)
- `crates/nif/src/blocks/tri_shape/ni_tri_shape.rs:41` `NiTriShape::name_str` under the comment "Backward-compatible field access" — no caller anywhere (`.name_str()` hits nothing); `HasObjectNET::name` (:67) already provides it. Delete the `impl` block.
- `crates/physics/src/world/queries.rs:572` `PhysicsWorld::bodies_overlapping_body` — all three production sites call `bodies_overlapping_body_where` (`systems/script_queries.rs:152`, `systems/trap_hits.rs:246,352`). Added 6dc948ac4 (2026-10-14). Only mentions are docs (`docs/engine/scripting.md:1860`, `AUDIT_PHYSICS_2026-10-16.md`) — update that doc line if deleting.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD8-2026-10-25-04`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
