# TD2-2026-10-15-01: Both NIF import walkers re-spell the shape cull gate eight times; every fix to it had to be hand-applied per arm and one arm family (particles) was missed

Labels: medium,nif-parser,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-01`).

- **Severity**: MEDIUM
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/nif/src/import/walk/mod.rs` hierarchical walker `:525-547` (NiTriShape), `:578-600` (BsTriShape), `:631-642` (NiLodTriShape), `:671-681` (BSGeometry); flat walker `:925-947`, `:969-991`, `:1009-1020`, `:1044-1054`; node-level copies at `:355-361` / `:801-806` (switch) and the NiNode arms
- **Status**: NEW
- **Age**: gate introduced `a050c5d38` (2026-04-17); mask changed `b06692d0e` (#332, 2026-04-20); live-NiVisController clause `452cccd0b` (#3640, 2026-09-02); particle arms brought into line `9281eebd7` (#4561, 2026-09-23)
- **Effort**: small
- **Description**: every geometry arm opens with the same 6 lines: `if shape.av.flags & 0x01 != 0 && !has_live_visibility_controller(scene, shape.av.net.controller_ref) { return; }` then `if is_editor_marker(shape.av.net.name.as_deref()) { return; }`, and each carries its own copy of the 12-line APP_CULLED / #332 / #3640 comment. Eight shape copies plus four node copies. The particle path already has the right shape (`particle_system_is_culled`, `:236`) but the shape arms never adopted a sibling.
- **Evidence**: the divergence is in the git history, not hypothetical. #332 changed the cull mask from `0x21` to `0x01` and had to touch each arm; #3640 added the `has_live_visibility_controller` clause to all eight; #4561's own commit message says "Every node and geometry arm in both import walkers gates on APP_CULLED ... but neither NiParticleSystem arm read the block's own flags or name" - an emitter authored hidden spawned from frame zero for ~5 months because the particle arms were outside the copy set.
- **Impact**: the next cull rule (a new APP_CULLED-like flag, a Starfield `BSGeometry` visibility bit) is a 12-site edit and a miss silently draws hidden content on one walker only (hierarchical = cell placement, flat = collision/legacy path).
- **Related**: #332, #3640, #4561, #5485 (revisit guard already lives once at `:282`).
- **Suggested Fix**: add `fn av_is_culled(scene: &NifScene, av: &NiAVObject) -> bool` beside `particle_system_is_culled` (flags bit + `has_live_visibility_controller` + `is_editor_marker`), have `particle_system_is_culled` call it first, and replace the 12 inline gates with `if av_is_culled(scene, &shape.av) { return; }`. Keep the 12-line rationale once, on the helper.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
