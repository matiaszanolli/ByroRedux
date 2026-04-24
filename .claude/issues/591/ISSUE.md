# #591: FO4-DIM6-06: NPC_ face-morph data unextracted — named FO4 NPCs render with base HDPT morphs

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/591
**Labels**: bug, medium, legacy-compat, 

---

**From**: `docs/audits/AUDIT_FO4_2026-04-23.md` (Dim 6)
**Severity**: MEDIUM
**Location**: `crates/plugin/src/esm/records/actor.rs:parse_npc`. `rg -n 'FMRI|FMRS|MSM0|MSM1|NAM9|face_morph' crates/plugin/src/esm/` returns zero hits.

## Description

FO4 NPC_ records carry face-morph data across 7+ sub-records:

- **FMRI** (face-morph range index): i32 index into a central face-morph enum per HDPT.
- **FMRS** (face-morph range setting): f32 0..1 slider value per FMRI.
- **MSDK** / **MSDV** (morph sliders, key/value pairs).
- **NAM9** (nose/cheek/jaw morph scalars — 19×f32 in vanilla format).
- **QNAM** / **NAMA** / **FTSM** (face texture set refs).
- **BCLF** (body color override).

Without these, every named FO4 NPC renders with the base HDPT morph targets = the vanilla "greybox" mannequin face. Same symptom as FO4 modding when FaceGen cache is missing.

## Impact

Companions (Nick, Piper, Cait, Preston, etc.) and all named NPCs look wrong. Generic settlers use the BGSM/HDPT default and may look OK.

**Note**: HDPT itself is stubbed already (`misc.rs:parse_hdpt`) but only captures EDID — see the second-order block below.

## Suggested Fix

Extend `parse_npc` with FMRI/FMRS/NAM9 arrays:

```rust
pub struct NpcFaceMorphs {
    pub sliders: Vec<(i32, f32)>,    // FMRI + FMRS pairs
    pub nose_cheek_jaw: [f32; 19],   // NAM9
    pub morph_kv: Vec<(String, f32)>, // MSDK + MSDV
}
```

Add `face_morphs: Option<NpcFaceMorphs>` to `NpcRecord`. Actual morph-target application is downstream of the skinning pipeline (closed #178) + HDPT mesh linking — the parse step unblocks that work.

**Second-order**: extend `parse_hdpt` to capture face texture set refs + morph-target mesh filenames (needed to apply face_morphs at render time).

## Completeness Checks

- [ ] **UNSAFE**: n/a
- [ ] **SIBLING**: Same parse walk applied to RACE record (race-level defaults for FMRI/FMRS)
- [ ] **DROP**: n/a
- [ ] **LOCK_ORDER**: n/a
- [ ] **FFI**: n/a
- [ ] **TESTS**: Corpus regression — assert Piper / Nick / Cait NPC records each yield non-empty `face_morphs.sliders`.

## Related

- Closed #178 (GPU skinning infra).
- HDPT expansion pending — currently only EDID captured.
