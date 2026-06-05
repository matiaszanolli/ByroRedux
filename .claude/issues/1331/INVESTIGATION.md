# #1331 — NiAVObject flag width uses variant helper instead of per-file BSVER>26

## Root cause (confirmed)
`crates/nif/src/blocks/base.rs:78` branched on `stream.variant().avobject_flags_u32()`
(a *variant*-level predicate) where nif.xml gates `NiAVObject.Flags` strictly on the
*per-file* `#BSVER# #GT# 26` (uint) vs `#LTE# 26` (ushort). A transitional v20.2.0.7
export with `uv=11 / bsver≤26` (NifSkope / newer-tool Oblivion + non-Bethesda Gamebryo
content) detects as the `Fallout3` variant (`version.rs` `NifVariant::detect`, uv=11 &
uv2<34), whose helper returns `true` → reads u32 where 2 bytes are on disk, slipping the
stream +2 from `flags` onward (block_size realignment masks the slip — same failure mode
as #342).

The properties-list branch directly below (base.rs:103) already used the correct
`stream.bsver() <= …::FO3_FNV` per-file pattern (#160) — the flags branch was the last
variant-level predicate shadowing a strict per-file BSVER gate.

## Sibling (same anti-pattern)
`crates/nif/src/blocks/shader.rs:166` — `BSShaderNoLightingProperty` falloff fields
(4×f32) used the same `variant().avobject_flags_u32()` gate. nif.xml line 6236 gates
them on `#BSVER# #GT# 26` (verified against `/mnt/data/src/reference/nifxml/nif.xml`).
Fixed in the same change.

## Fix
Both sites now branch on `stream.bsver() > crate::version::bsver::FLAGS_U32_THRESHOLD`
(the `=26` constant that already existed for exactly this gate, `version.rs:282`).

## Verification
- `NifVariant::detect(V20_2_0_7, uv=11, uv2=11)` → `Fallout3` (confirmed) → old code
  read u32; new code reads u16. The u16 regression test is red before the fix, green
  after.
- New tests (all green):
  - `base.rs::niavobject_version_gate_tests::flags_read_as_u16_when_bsver_le_26`
  - `base.rs::niavobject_version_gate_tests::flags_read_as_u32_when_bsver_gt_26`
  - `shader_tests.rs::no_lighting_falloff_absent_when_bsver_le_26`
  - `shader_tests.rs::no_lighting_falloff_present_when_bsver_gt_26`
- Standard Oblivion (v20.0.0.x) is unaffected: caught by the Oblivion variant branch
  before the uv match, bsver≤26 → u16 either way.
