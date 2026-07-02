# RT-2: TES-family character rig never grounds -> infinite freefall (Oblivion, Skyrim)

**Issue**: #1832
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-07-02.md`
**Labels**: medium, performance, legacy-compat, bug

**Severity**: MEDIUM
**Dimension**: runtime / physics (character controller)
**Game / Cell**: oblivion / ICMarketDistrictTheGildedCarafe **and** skyrim_se / WhiterunDragonsreach
**Location**: `byroredux/src/systems/character.rs` (M28.5 character/physics grounding; symptom logged as `M28.5 frame N: body Y a→b … grounded=false`); ground-probe/KCC result consumed from `crates/physics/src/world.rs` (`grounded` field on the move-shape result, ~line 411/635) and written into `crates/physics/src/components.rs::CharacterController.is_grounded` (~line 107)

**Description**: A clean cross-game split surfaced in the freefall telemetry from the 2026-07-02 runtime audit: the character rig **grounds in every Fallout cell** (`grounded=true` — FNV, FO3, FO4) but **never grounds in either TES cell** (`grounded=false` — Oblivion and Skyrim), with body Y descending unbounded at v=-2000 (Skyrim: Y −6542.6 → −6609.3 in one log window). This is infinite fall, not settling. It is benign in Oblivion (156 rapier bodies → `systems_ms=0.14`) but catastrophic in Skyrim (1575 bodies → `systems_ms=31.97`, the RT-1 / #1698 perf collapse). Note body count alone is not the cause: FO4 has **2081** rapier bodies yet `grounded=true` and `systems_ms=0.67`.

**Evidence**: per-game frame-240 telemetry — FNV `grounded=true`/1342 bodies; FO3 `grounded=true`/845; FO4 `grounded=true`/2081; Oblivion `grounded=false`/156; Skyrim `grounded=false`/1575.

**Impact**: The player/character rig falls through the world in TES-family cells. Invisible on small cells (Oblivion — cheap enough to be a silent correctness bug); on Skyrim it is the direct cause of the RT-1 (#1698) ~10x perf collapse (321→30 fps), because a continuously-falling body sweeps a 1575-body scene every physics substep.

**Related**: #1698 (perf half of this same root cause — fixing grounding here should also close it).

**Suggested Fix**: Investigate why the ground probe fails for TES cells — compare the collision-mesh registration in the Oblivion/Skyrim cell-load path against the Fallout path; verify the ground raycast axis after the Z-up→Y-up conversion (`crates/nif/src/import/coord.rs`). Likely candidates: floor/collision-mesh not registered with the Rapier solver for TES cell loads, or a ground-probe axis mismatch specific to the TES import route.

## Completeness Checks
- [ ] **SIBLING**: Same grounding path checked across all TES-family cell-load sites (Oblivion interior/exterior, Skyrim interior/exterior)
- [ ] **TESTS**: A regression test pins `is_grounded=true` for a TES cell with valid floor collision within N frames of spawn
