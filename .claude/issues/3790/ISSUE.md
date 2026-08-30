# #3790: FNV-2026-08-30-D8-02: --game fnv omits Update.bsa, so 21 base-game FalloutNV.esm MODL paths (incl. the NCR guard towers) resolve to no mesh

**Labels**: bug, import-pipeline, low, legacy-compat, game:fnv
**Filed**: 2026-08-30 · HEAD `64f64480`

---

**Source**: `docs/audits/AUDIT_FNV_2026-08-30.md` — FNV-2026-08-30-D8-02 (LOW)
**Dimension**: 8 — launch profiles / asset sourcing
**Location**: `assets/debug_profiles.toml:47` — `default_bsas = ["Fallout - Meshes.bsa"]`

## Description

FNV's `Update.bsa` (86 entries, 55 of them NIFs) is the base game's own patch archive and loads at **higher priority** than `Fallout - Meshes.bsa` in the retail engine. The `[profiles.fnv]` profile does not list it, and `Update.bsa` is not a `<stem>N.bsa` sibling of anything, so the auto-load rule in `byroredux/src/asset_provider/archive.rs` does not pick it up either.

Verified at HEAD: `default_bsas = ["Fallout - Meshes.bsa"]` is unchanged.

## Evidence

Resolved the first `MODL` of every `STAT` / `DOOR` / `CONT` / `ACTI` / `FURN` / `MSTT` / `WEAP` / `ARMO` / `MISC` / `TERM` / `LIGH` / `TREE` / `GRAS` / `SCOL` record in `FalloutNV.esm` — 11,994 paths — against the archive index:

| Outcome | Count |
|---|---:|
| in `Fallout - Meshes.bsa` | 11,953 |
| **only in `Update.bsa`** | **21** |
| only in a DLC archive | 1 |
| in no FNV BSA (loose / pre-order pack) | 19 |

The 21 include the NCR guard towers (`meshes\architecture\ncr\nvguardtower01a.nif`, `…01b`, `…01c`) — visible exterior architecture, not debug props.

## Impact

Small and bounded: 21 of 11,994 base records (0.18%), degrading to a missing mesh rather than a failure. Filed LOW for completeness of the profile, and because `Update.bsa` also carries **6 `wastelandnv` object-LOD block quads** that the #3321 LOD ring would otherwise resolve.

## Suggested Fix

`default_bsas = ["Fallout - Meshes.bsa", "Update.bsa"]`, ordered so the patch archive wins the priority resolution the way retail does.

## Related

- #3321 — object-LOD block ring (6 of its quads live in the unlisted archive)
- FNV-2026-08-30-D8-01 — the sibling omission in the same profile (`default_sounds_bsas`)

## Completeness Checks
- [ ] **SIBLING**: Check `[profiles.fo3]` and `[profiles.oblivion]` for equivalent unlisted base-game patch archives
- [ ] **TESTS**: A regression test pins the archive priority order once `Update.bsa` is listed (the patch archive must win)
