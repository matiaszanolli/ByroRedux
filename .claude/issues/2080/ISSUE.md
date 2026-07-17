# FNV-D4-02: parse_npc's FaceGen-recipe FormID fields (HNAM/ENAM/PNAM-eyebrow/FMRI) still unremapped after #1996

- **Severity**: HIGH
- **Labels**: high, import-pipeline, bug
- **Location**: `crates/plugin/src/esm/records/actor.rs:691-712,754-763`; consumer `byroredux/src/npc_spawn.rs:1083-1165`
- **Status note**: follow-on gap in the closed #1996 fix (same function, same commit, incomplete coverage).

## Description
#1996 added the `remap` parameter to `parse_npc` and applied it to RNAM/CNAM/VTCK/SCRI/SNAM/CNTO/PKID/DOFT/INAM/TPLT/PRPS/PRKR — but the same function's FaceGen-recipe arms (HNAM/ENAM/FNV-PNAM-eyebrow/FMRI/FO4-PNAM-head_parts) still read raw `u32_or_default()` with no `remap_fid()` wrapper, despite `remap` being in scope for the whole function.

## Evidence
In `actor.rs`, fields like `race_form_id`/`class_form_id`/`voice_form_id`/`script_form_id`/faction/perk/AV entries all call `remap_fid(raw, remap)` (lines ~567-795), but the `HNAM` (691), `ENAM` (697), `PNAM`-eyebrow (704), and `FMRI` (711) arms read `SubReader::new(&sub.data).u32_or_default()` directly with no `remap_fid` wrapper.

## Impact
On a multi-plugin load, an NPC defined in a non-base plugin whose hair/eyes/eyebrow reference points at content defined in that same plugin resolves the wrong or no `index.hair`/`index.eyes`/`index.head_parts` entry — silently bald/browless, or (on FormID collision across plugins) wrong hair/eye texture attached. No crash, no log.

## Suggested Fix
Wrap HNAM/ENAM/PNAM(both arms)/FMRI reads in the existing local `remap_fid(raw, remap)` helper, exactly like the PRPS/PRKR arms in the same function.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked across all FormID-bearing sub-record arms in `parse_npc` (not just the classic fields #1996 already fixed)
- [ ] **SIBLING-PARSERS**: Confirm no other per-game (FO3/Oblivion/FO4) NPC_ parsing variant shares this same unremapped-FaceGen-field gap
- [ ] **TESTS**: A regression test pins this specific fix (multi-plugin NPC with HNAM/ENAM/PNAM/FMRI pointing at same-plugin content)
