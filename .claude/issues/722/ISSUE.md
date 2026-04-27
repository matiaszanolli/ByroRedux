# NIF-D5-07: FO4+ BSClothExtraData length-field bug — 1,523 blocks demoted across FO4/FO76/SF

URL: https://github.com/matiaszanolli/ByroRedux/issues/722
Labels: bug, nif-parser, medium

---

## Severity: MEDIUM

## Game Affected
FO4, FO76, Starfield

## Location
- `crates/nif/src/blocks/extra_data.rs:512` (`BsClothExtraData::parse`)

## Description
Implementation reads `length: u32` then `data: u8[length]`, but in FO4+ NIFs the length-prefix format differs (likely a `u64` size or a different leading sentinel). Result: parser reads garbage size, hits EOF or buffer-bounds, errors out, gets demoted via `block_size`-driven recovery.

This is a **parser-error** (block IS dispatched) — fix is in the `parse` body, not the dispatch table.

## Evidence
2026-04-26 corpus sweep:
- `Fallout4 - Meshes.ba2` — 309 demotions
- `SeventySix - Meshes.ba2` — 365 demotions
- `Starfield - Meshes01.ba2` — 298 demotions
- `Starfield - FaceMeshes.ba2` — 551 demotions
- Total: 1,523 demotions

Total ratio of `BSClothExtraData`-bearing NIFs to blocks demoted is near 1:1 — every cloth-having NIF on FO4+ fails.

## Impact
Havok cloth simulation references missing — capes, flags, curtains, hair on FO4+ fall back to rigid geometry.

## Suggested Fix
At `extra_data.rs:512`: gate `length` field width on `bsver` from `NifStream`. nif.xml has the per-version layout — verify against an FO4 cloth NIF (e.g. `meshes/clothes/longjohns/longjohns.nif`) via `examples/trace_block.rs`.

## Related
- Audit: docs/audits/AUDIT_NIF_2026-04-26.md (NIF-D5-07)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check `BsBehaviorGraphExtraData` and other `Bs*ExtraData` for same length-field pattern
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Byte-exact regression with captured FO4+ cloth NIF fixture
- [ ] **CORPUS**: Reproduce zero NiUnknown for `BSClothExtraData` on FO4 + FO76 + SF archives
- [ ] **ALLOCATE_VEC**: Verify `allocate_vec` cap on the data byte array
