## Finding NIF-NEW-01 — NIF Audit 2026-06-13

- **Severity**: HIGH
- **Dimension**: Block Parsing + Version Handling (coupled)
- **Game Affected**: Oblivion (the v10.1.0.x sub-corpus). Vanilla hit: `meshes\characters\_1stperson\skeleton.nif` (file-version 10.1.0.106). Any NIF in `[10.1.0.104, 10.1.0.108]` carrying NiInterpController-derived controllers.
- **Location**: field read — `crates/nif/src/blocks/controller/mod.rs:227-241` (`NiSingleInterpController::parse`) and every NiInterpController descendant (`NiMultiTargetTransformController` in `controller/sequence.rs`, `NiGeomMorpherController` in `controller/morph.rs`); version surface — `crates/nif/src/version.rs` (no `V10_1_0_108` constant, no 104–108 range predicate).
- **Status**: NEW — validated CONFIRMED at HEAD `8d191d7d`.

## Description

nif.xml:3615 defines `NiInterpController."Manager Controlled" : bool` `since="10.1.0.104" until="10.1.0.108"`. The parser collapses NiInterpController into `NiSingleInterpController` and reads only the `interpolator_ref` (gated `>= V10_1_0_104`); the preceding 1-byte bool is never read. `version.rs` jumps `V10_1_0_104 (0x0A010068) → V10_1_0_106 (0x0A01006A) → V10_1_0_113 (0x0A010071)` with **no `0x0A01006C` (108)** constant, so even a corrected field-read has no comparator to express the `until=10.1.0.108` upper bound.

## Evidence (validated)

- `version.rs:88,91,93` — constants present are `V10_1_0_104`, `V10_1_0_106`, `V10_1_0_113`; `0x0A01006C` (108) is absent.
- `grep -rni 'manager.controlled' crates/nif/src/blocks/controller/` → no read site anywhere.
- Byte-trace of block 5 `NiTransformController` in `_1stperson\skeleton.nif` (v10.1.0.106): base 26 B ends at target@1362=2, then **Manager Controlled bool@1366=0 [UNREAD]**, interpolator@1367. Parser ends at 1370; correct end 1371. The next block's `name_len@1375=14` ("Bip01 NonAccum") confirms the exact 1-byte shortfall.
- A per-file sweep showed `_1stperson\skeleton.nif` is the ONLY Oblivion mesh with `recovered>0` (135) — the "NiNode=70 / NiStringExtraData=65 / NiMaterialProperty=1 / NiTexturingProperty=1" recoveries are all this one file's controller-drift cascade tail (not independent parser bugs).

## Impact

1-byte/controller drift accumulates across the controller-dense skeleton until a downstream block reads a garbage length and recovers; 135 blocks become opaque `NiUnknown` — the first-person view-model skeleton loses every animation-controller linkage. Confined to the 104–108 band (1 vanilla file today; widens for modded / early-Gamebryo content).

## Suggested Fix

1. Add `pub const V10_1_0_108: Self = Self(0x0A01006C);` to `version.rs`.
2. Add a range predicate, e.g. `has_interp_controller_manager_controlled(self) -> bool { self >= V10_1_0_104 && self <= V10_1_0_108 }`.
3. Read the 1-byte bool through that helper in a shared `NiInterpController` base layer used by all descendants. **The version-gate plumbing must land first.**

This is the version-constant prerequisite shared with NIF-NEW-02 / NIF-NEW-03 (all three need the missing `V10_1_0_108`/`110`/`111` constants) — land the constants once, then each parser byte-audit independently.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant (N/A expected)
- [ ] **SIBLING**: Apply the bool read to ALL NiInterpController descendants (NiSingleInterpController, NiMultiTargetTransformController, NiGeomMorpherController), not just the skeleton's controller type
- [ ] **DROP**: N/A (no Vulkan objects)
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **CANONICAL-BOUNDARY**: N/A (parse-layer only; does not touch translate_material)
- [ ] **TESTS**: Add a v10.1.0.106 controller-chain round-trip fixture; regression check = `_1stperson\skeleton.nif` recovers to 0 dropped blocks

---
Source: `docs/audits/AUDIT_NIF_2026-06-13.md` · Filed by `/audit-publish` · Absorbs agent IDs NIF-D1-NEW-01 + NIF-D2-NEW-01 (same fix, two layers)
