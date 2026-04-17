# FO4-D4-C3: TXST MNAM (BGSM material path) silently dropped — 37% of FO4 texture sets lost

**Issue**: #406 — https://github.com/matiaszanolli/ByroRedux/issues/406
**Labels**: bug, renderer, critical, legacy-compat

---

## Finding

`crates/plugin/src/esm/cell.rs:997-1008` `parse_txst_group` extracts only the `TX00` subrecord. FO4's TXST record has a mutually-exclusive path via `MNAM` (BGSM material reference) that replaces `TX00`.

## Evidence

Vanilla `Fallout4.esm` TXST subrecord frequency across 382 records:

```
DNAM 382  EDID 382  OBND 382
TX00 240    ← parsed
MNAM 140    ← NOT parsed (BGSM material path — 37% of all TXST)
TX01 235    ← NOT parsed (normal map)
DODT 207    ← decal geometry data, ignored
TX07 160  TX02-05 …
```

140 of 382 TXST records (37%) use `MNAM`→BGSM path only and have no `TX00`. All are silently skipped by the `txst_textures.insert()` call.

## Impact

- **LTEX → TXST → landscape texture resolution** at [cell.rs:243-250](crates/plugin/src/esm/cell.rs#L243-L250) works for ~63% of texture sets. The rest yield empty paths, landing on fallback checkerboard for any LAND terrain patch whose LTEX points at a BGSM-only TXST.
- **Interior decals** using BGSM-only TXST records (`Decals\\*.BGSM`) miss their diffuse texture entirely.

## Fix

Extend `parse_txst_group` to also extract `MNAM` and store both fields per TXST form ID:

```rust
pub struct TxstEntry {
    pub texture_path: Option<String>,   // TX00
    pub material_path: Option<String>,  // MNAM → BGSM
    pub normal_path: Option<String>,    // TX01 (optional)
}
```

Resolve `material_path` via the BGSM parser at the texture-provider layer. The BGSM parser itself is tracked as a separate issue (Dim 6 Stage C).

## Dependencies

- Separate issue: **BGSM parser missing** (Dim 6 Stage C). Until BGSM parser lands, MNAM extraction alone doesn't unblock rendering — but the data must be captured for when it does.
- Complements #357 (Skyrim TXST 8 slots) — different record layout; FO4 TXST additionally carries the MNAM path.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Skyrim TXST handling at #357 — is the MNAM path Skyrim-relevant? Verify before the PR.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Synthetic TXST with MNAM only (no TX00) round-trips with `material_path.is_some()`; live test confirms 140/140 vanilla MNAM-only TXST records surface their material_path.

## Source

Audit: `docs/audits/AUDIT_FO4_2026-04-17.md`, Dim 4 C3.
