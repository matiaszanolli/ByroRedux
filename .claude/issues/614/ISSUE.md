# SK-D5-03: BSBoneLODExtraData has no parser entry — every Skyrim SE skeleton.nif (52 files) truncates to NiUnknown

## Finding: SK-D5-03

- **Severity**: MEDIUM
- **Source**: `docs/audits/AUDIT_SKYRIM_2026-04-24.md`
- **Game Affected**: Skyrim SE (every actor skeleton — vampire, draugr, dragon, hare, all DLC02 races, ~52 files)
- **Location**: `crates/nif/src/blocks/extra_data.rs` (dispatch missing); `nif_stats` log: `/tmp/audit/skyrim/meshes0.log`

## Description

`BSBoneLODExtraData` has no parser entry. Every Skyrim SE `skeleton.nif` carries this block and falls through to `NiUnknown`. The block-size table snaps the stream forward so geometry survives, but the data — bone-LOD distance thresholds for skeleton mesh swapping — is lost.

This is one of the two drivers of the parse-rate regression from 100 % to ~99.7 % on `Skyrim - Meshes0/1.bsa` (60 files now flagged truncated/recovered, where the 2026-04-22 baseline showed 0).

## Evidence

```
# /tmp/audit/skyrim/meshes0.log
─── Unparsed types (no dispatch entry) ──
   unknown  type
        52  BSBoneLODExtraData
```

## Suggested Fix

Add a parser per nif.xml `BSBoneLODExtraData`:

```
struct BSBoneLODExtraData : NiExtraData {
    uint num_bone_lods;
    struct {
        uint distance;
        Ref<NiNode> bone;
    } bone_lods[num_bone_lods];
}
```

Wire dispatch in `crates/nif/src/blocks/extra_data.rs` alongside the other `*ExtraData` kinds. Surface `bone_lods` on `ImportedNode` if the runtime needs it, otherwise just consume to round out the parse rate.

## Related

- #164 (closed): `NiStringsExtraData` / `NiIntegersExtraData` unhandled — same pattern.
- SK-D5-04 (companion): broader stream-drift bundle restoring 100% parse rate.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Audit other Skyrim+ `*ExtraData` block names that fall to `NiUnknown` in the histogram (cross-check against the meshes0.log unknown-types list).
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add an integration test — open a Skyrim SE skeleton.nif (e.g. `actors\character\character assets\skeleton.nif`), expect 0 NiUnknown blocks.

_Filed from audit `docs/audits/AUDIT_SKYRIM_2026-04-24.md`._
