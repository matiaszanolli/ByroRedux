## Finding NIF-NEW-02 — NIF Audit 2026-06-13

- **Severity**: HIGH
- **Dimension**: Stream Position (field-read/stride) + Version Handling (comparator surface)
- **Game Affected**: Oblivion only (v10.2.0.0, sizeless format).
- **Location**: `crates/nif/src/blocks/particle.rs::parse_particles_data` (entry, line 1089) and the shared `parse_geometry_data_base` / `parse_psys_geometry_data_base` in `crates/nif/src/blocks/tri_shape/` (called from `particle.rs:1103-1105`); surfaces as truncation in `crates/nif/src/lib.rs::parse_nif` Err-branch. Comparator surface: `crates/nif/src/version.rs` (missing `V10_1_0_108/110/111`).
- **Status**: NEW — validated CONFIRMED at HEAD `8d191d7d`.

## Description

On Oblivion particle NIFs, `NiPSysData` over/under-consumes, leaving the stream misaligned; the following `NiPSys*Emitter` reads a string-length off the misaligned offset, reinterprets ASCII as a u32 count, trips the 268 MB allocation cap, and returns Err. The sizeless v10.2.0.0 format has no `block_size` table → no realignment → the rest of the scene truncates.

## Evidence (validated)

- `parse_particles_data` confirmed at `particle.rs:1089`; the version-gated NiParticleInfo path has tests (`parse_particles_data_uses_40_byte_particle_info_on_pre_10_4_0_1`, `..._skips_particle_info_on_oblivion`, `..._does_not_skip_particle_info_on_bs202`). The shared geometry base lives in `super::tri_shape::parse_geometry_data_base` / `parse_psys_geometry_data_base`.
- `meshes\effects\metalsparks.nif` (v10.2.0.0): block 30 `NiPSysData` consumed 4900 B, block 32 `NiPSysBoxEmitter` then requested a **1,752,457,573-byte** allocation = `0x68746165` = `"eath"` (tail of an `...AgeDeath` name) — DISCARDING 40 blocks. A static walk from the believed offset yields a clean empty-name emitter, confirming the drift entered upstream at block 30/31.

## Impact

~27 of 56 truncated Oblivion scenes (23 on `NiPSysBoxEmitter`, +3 other emitter types, +1 `NiPSysPositionModifier`). Concentrated in `effects/`, `magiceffects/`, dungeon FX. Drops the particle subtree + everything after it. No corruption/OOM — the alloc cap holds.

## Suggested Fix

Byte-audit `parse_particles_data` + the shared geometry-data base against nif.xml for the **v10.2.0.0 / non-BS202** path specifically (existing gates were tuned for Oblivion 20.0.0.4 and FNV+ 20.2.0.7; the 10.x sub-versions are under-tested). `NiParticleInfo` stride (40 B ≤10.4.0.1) is correct per nif.xml:2263 — the drift is upstream in the geometry-data base or a has-flag array. Add the missing version constants (shares `V10_1_0_108` with NIF-NEW-01) and a v10.2 NiPSysData round-trip fixture.

## Completeness Checks
- [ ] **UNSAFE**: N/A expected
- [ ] **SIBLING**: Check every `NiPSys*Emitter` subtype (Box/Sphere/Cylinder/Mesh/Position) reads correctly once the upstream NiPSysData stride is fixed — the emitter is the victim, not the originator
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **CANONICAL-BOUNDARY**: N/A (parse-layer)
- [ ] **TESTS**: Add a v10.2.0.0 NiPSysData round-trip fixture; regression check = `metalsparks.nif` parses with 0 dropped blocks

---
Source: `docs/audits/AUDIT_NIF_2026-06-13.md` · Filed by `/audit-publish` · Absorbs NIF-D3-NEW-08 + v10.2 half of NIF-D2-NEW-02
