

================ #1576 [OPEN] bug, import-pipeline, low, legacy-compat, game:starfield, esm-plugin ================
# SF-D4-03: Model-less STAT/BNDS/ACTI/ARMO Starfield forms drop because geometry lives in a BFCB component block

**Severity**: LOW · **Dimension**: SF ESM Resolve-Rate
**Location**: `crates/plugin/src/esm/cell/support.rs:38-160` (only top-level `MODL` is read)
**Source**: `docs/audits/AUDIT_STARFIELD_2026-06-14.md` (SF-D4-03)

## Description
`build_static_object_from_subs` extracts the model only from a top-level `MODL` subrecord (`support.rs:41`). Some Starfield STAT/BNDS/ACTI/ARMO records put the model reference inside a `BFCB`-wrapped component, so they return `None` and their REFRs drop.

## Evidence
`STAT 00000021 subs: EDID OBND ODTY OPDS BFCB BFCE FLLD PRPS DNAM` (no MODL); `BNDS 000001F9 subs: EDID OBND ODTY DNAM(28) MNAM(4)`. Counts: STAT 44/2, BNDS 60/2, ACTI 33/11, ARMO 3/1 — ~140 REFRs (~0.5% of cell).

## Impact
Small. The two unresolved STAT forms are very low FormIDs (0x21/0x43 — likely default/template/marker statics); BNDS is bendable-spline (needs a generator). Tail content; no structural architecture lost.

## Related
SF-D4-01 (shares the `BFCB` component-block walker need).

## Suggested Fix
When SF-D4-01's `BFCB` component walker lands, reuse it to recover a model reference for STAT/ACTI/ARMO. BNDS needs a dedicated spline-mesh generator — track separately.

## Completeness Checks
- [ ] **SIBLING**: Reuses the exact `BFCB`/`BFCE` walker from SF-D4-01 (not a second copy); STAT/ACTI/ARMO all route through it
- [ ] **TESTS**: A test pins a model-less-`MODL` STAT form recovering its model ref from the `BFCB` block



================ #2097 [OPEN] bug, import-pipeline, low, legacy-compat, dependencies, game:starfield ================
# LZ4-01: LZ4 decompress relies on undocumented-safe dependency behavior the crate itself disclaims as "may panic"

**Severity**: LOW
**Dimension**: BA2 v2/v3 LZ4 Block Decompression
**Location**: `crates/bsa/src/ba2.rs:692-696` (comment), `:717-724` (call site)
**Source**: `docs/audits/AUDIT_STARFIELD_2026-07-16.md` (LZ4-01)

## Description
A comment asserts the LZ4 branch "inherently size-checks" and hard-errors on a size mismatch (see the comment on the neighboring Zlib branch referencing this as "#812 / FO4-D2-NEW-02"), but pinned `lz4_flex 0.11.6`'s own docs state the `decompress` function "may panic" if `min_uncompressed_size` undershoots the true decompressed size — a stronger guarantee than the dependency's public contract promises. Empirical fuzzing (constructed LZ4 payloads, undersized from 1 byte to 0) found zero panics on the currently pinned version — not an active bug, but an unpinned assumption that could silently regress on a future `lz4_flex` upgrade.

## Evidence
`crates/bsa/src/ba2.rs` — the `Ba2Compression::Lz4Block` arm calls `lz4_flex::block::decompress(packed, unpacked_size)` directly, propagating any error via `map_err`, but has no `catch_unwind` guard against the "may panic" documented behavior.

## Impact
None today. A future dependency bump that tightens/loosens the internal bounds-check discipline (within its still-compatible public contract) could crash the engine on a malformed/adversarial v3 BA2 chunk record, with no code change on this side to explain why.

## Suggested Fix
Wrap the call in `catch_unwind` and convert a caught panic into the existing `Err` path, or pin the safety claim to `lz4_flex 0.11.6` with a version-gated regression test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the Zlib branch's own size-mismatch handling in the same function; confirm no other direct `lz4_flex`/`zlib` calls elsewhere in the archive readers share this gap)
- [ ] **TESTS**: A regression test pins this specific fix (constructed undersized-LZ4 payload test, or a version-pin assertion on `lz4_flex`)



================ #2330 [OPEN] documentation, nif-parser, low, legacy-compat, game:skyrim, nifal ================
# SKY-D7-03: Canonical PBR roughness is written at a second spawn-time site outside translate_material

**Severity**: LOW
**Location**: `byroredux/src/material_translate.rs:300-338` (`resolve_normal_alpha_spec_roughness`), called from both spawn paths (`byroredux/src/scene/nif_loader.rs:924`, `byroredux/src/cell_loader/spawn.rs:1350`)

## Description

Both spawn paths call `resolve_normal_alpha_spec_roughness` after texture
handles are attached, re-deriving `roughness` from `glossiness`/
`specular_strength` plus resolved normal/gloss textures. This is the
dominant path for Skyrim specifically (no dedicated gloss map; spec mask
lives in the normal-map alpha), so most Skyrim architecture's shipped
roughness comes from this second write, not `translate_material`'s literal.
**Not a defect** — the helper is idempotent and NaN-guarded — this is a
documentation-precision finding only.

## Evidence

```rust
// byroredux/src/material_translate.rs:300
pub(crate) fn resolve_normal_alpha_spec_roughness(world: &mut World, entity: EntityId) { ... }
```
Called from:
```
byroredux/src/scene/nif_loader.rs:924
byroredux/src/cell_loader/spawn.rs:1350
```
confirmed present at HEAD (1ae86f62); both spawn paths call it after texture
resolution.

## Impact

None functionally. The cost is purely that `material_translate.rs`'s own
"the single site" doc claim and `nifal.md` describe a one-shot boundary
where the real implementation is a documented two-phase one.

## Suggested Fix

Amend the module doc and `nifal.md`'s Materials row to describe the
boundary as two-phase (spawn-time literal + post-texture-resolution
roughness re-derivation).

## Completeness Checks
- [ ] **SIBLING**: Check whether other canonical fields (not just roughness) get a similar second write at either spawn site
- [ ] **CANONICAL-BOUNDARY**: Document the two-phase boundary explicitly in `material_translate.rs` and `nifal.md` so future per-game logic additions know both write sites exist. See `/audit-nifal`.



================ #2335 [OPEN] bug, import-pipeline, low, legacy-compat, game:fnv, game:fo3 ================
# FO3-D6-NEW-01: parse_real_facegen.rs docstring claims FNV+FO3 coverage but the test only ever exercises FNV by default

**Severity**: LOW
**Location**: `crates/facegen/tests/parse_real_facegen.rs:1-41`
**Status**: NEW

### Description
The module doc claims FNV/FO3 coverage; the actual data-dir resolution + BSA filename are FNV-only, with no `BYROREDUX_FO3_DATA` fallback (unlike the sibling NIF/`.spt` real-data tests, which do have dedicated FO3 arms).

Confirmed against current code: `parse_real_facegen.rs:1-3` doc says "vanilla FNV / FO3 content"; `data_dir()` reads only `BYROREDUX_FNV_DATA` (or the hardcoded `FNV_DEFAULT_DATA` Steam path) and `FNV_MESH_BSA` is hardcoded to `"Fallout - Meshes.bsa"` under the FNV data dir — no FO3-specific env var or path exists anywhere in the file.

### Evidence
Manually pointed the FNV env var at the FO3 install — all 3 tests pass; FO3's `headhuman.{egm,egt,tri}` are byte-for-byte identical to the hardcoded FNV baselines (asset-reuse coincidence, not something the test structurally guarantees).

### Impact
No functional bug — FaceGen parsing genuinely works on real FO3 data. Pure test-coverage/CI-signal gap: nothing would catch a future FO3-only face asset (ghoul/super-mutant/robot) diverging from the FNV-shared assets.

### Suggested Fix
Parametrize the existing 3 tests over `[("FNV", …), ("FO3", …)]` mirroring the `Game` enum pattern in `parse_real_nifs.rs`/`parse_real_spt.rs`.

### Related
analogous to already-closed #1452/#2090 (same class, doc-overclaim)

## Completeness Checks
- [ ] **SIBLING**: Mirror the `Game`-enum parametrization pattern already used in `parse_real_nifs.rs`/`parse_real_spt.rs`
- [ ] **TESTS**: Parametrized test suite exercises both FNV and FO3 data dirs explicitly, not by coincidental byte-identity

