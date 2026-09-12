# #4241 — FO4-D6-01: mswp.rs module doc describes XMSP consumption as future work; it already shipped

**Severity**: LOW
**Dimension**: 6 — ESM Architecture Records (SCOL/MOVS/PKIN/TXST)
**Location**: `crates/plugin/src/esm/records/mswp.rs:30-38`
**Status**: NEW

**Description**: The module doc phrases `XMSP` material-swap parsing and consumption as pending ("the cell loader will consult once … parsed"). Both the parse (`esm/cell/walkers.rs:1015-1030`) and the consumer (`byroredux/src/cell_loader/refr.rs:401-438`) already exist and are regression-tested.

**Evidence**: Confirmed in current code — `walkers.rs:1024` has a live `b"XMSP" => {` parse arm, and `refr.rs:401-438` resolves `placed.material_swap_ref` against `index.material_swaps`, populating `ov.material_swaps`/`ov.material_swaps_filter`.

**Impact**: None on behavior — stale doc only, risk of a future contributor re-implementing shipped work.

**Suggested Fix**: Update the doc's "Downstream use" paragraph to state XMSP resolution is implemented (cite `refr.rs`'s consumer instead of describing it as future work).

## Completeness Checks
- [x] **TESTS**: N/A (documentation-only fix)

**Disposition**: Rewrote the "Downstream use" paragraph to describe the shipped behavior (#971 / FO4-D4-NEW-08), citing `refr.rs`'s actual resolution flow: eager `material_path` substitution against the FNAM filter, plus the preserved `swaps` list for the spawn path's per-shape later-wins application.

---

# #4266 — OB-D7-01: docs/feature-matrix.md's Cell Loading table is stale — still shows Oblivion exterior bench as pending, a month after it landed

**Severity**: MEDIUM
**Dimension**: Dimension 7 — Exterior Blocker Chain & Game-Specific Quirks
**Location**: `docs/feature-matrix.md:20,23,25-28`
**Status**: NEW

## Description
`docs/feature-matrix.md`'s Cell Loading table reads "bench pending" / "device check pending" for Oblivion's exterior grid and confirmed-bench rows, false since commit `f90e4eec` (2026-08-12, Fix #2368), which recorded a real on-device measurement but never touched `feature-matrix.md`.

## Evidence
`docs/feature-matrix.md:20` and `:23` still showed Oblivion as pending; the real 2026-08-12 measurement: Tamriel `(0,0)` radius 1, 6,043 entities / 2,355 draws, image-health + environment-value gates both clean.

## Impact
Discoverability gap: a reader consulting `feature-matrix.md` concludes Oblivion's exterior-cell rendering is still unverified, when it has been closed for a month.

## Suggested Fix
Change line 20's Oblivion cell to "✓", line 23's to the real entity/draw figures + citation, and rewrite lines 25-28's note. FO3 remains genuinely pending and should stay as-is.

## Completeness Checks
- [x] **SIBLING**: Same pattern checked in related files
- [x] **TESTS**: N/A (documentation-only fix)

**Disposition**: Updated the Exterior grid row to "✓" for Oblivion, the Confirmed bench row to "6 043 ent · 2 355 draws (#2368)", and rewrote the explanatory note to cite the actual closed measurement instead of "only an on-device exterior render bench is pending." FO3's "device check pending" cell left untouched as instructed.

---

# #4280 — SF-2026-09-11-D6-02: BSShaderCRC32 hash derivation is fully reproducible (32/32) — retract the stale "do not repeat this search" instruction

**Severity**: LOW
**Dimension**: Dimension 6 — NIF Shader Blocks, BSVER 155+
**Location**: `crates/nif/src/shader_flags.rs:515-524 (comment); docs/audits/AUDIT_STARFIELD_2026-08-30.md:288-293`
**Status**: NEW

## Description
Two standing comments record the `BSShaderCRC32` hash derivation as opaque/unrepeatable. This audit established the derivation empirically (32/32 match): reflected CRC-32, polynomial `0xEDB88320`, init `0`, no final XOR, over the flag name in ASCII uppercase exactly as nif.xml spells it.

## Evidence
`crates/nif/src/shader_flags.rs::bs_shader_crc32` names all 32 of 32 nif.xml `BSShaderCRC32` entries — complete spec coverage.

## Impact
No functional impact. Documentation-accuracy risk: the stale "do not repeat this search" instruction would misdirect a future contributor.

## Suggested Fix
Update both comment sites to record the now-solved derivation instead of the stale "unrepeatable" claim.

## Completeness Checks
- [x] **SIBLING**: Same pattern checked in related files
- [x] **TESTS**: N/A — documentation-only fix.

**Disposition**: Independently re-derived and verified all 32/32 entries against nif.xml (`sed -n '6520,6553p' nif.xml` + a standalone reflected-CRC32 computation) before writing the fix, confirming the exact parameterization the issue describes — critically, nif.xml spells the flag names in a mix of underscored and non-underscored forms (`VERTEXCOLORS` with no underscore, not this module's `Vertex_Colors` doc-comment spelling), which is why a prior naive uppercase/underscored search missed it. Updated `shader_flags.rs`'s `#712` test comment to record the solved derivation. Left `docs/audits/AUDIT_STARFIELD_2026-08-30.md` untouched: that "negative result" paragraph documents a *different* search (matching 10 *observed* CRC hashes against 1,368 unrelated nif.xml bitflag/option names + 459 CDB strings as an independent cross-check vocabulary, not the `BSShaderCRC32` enum's own 32 option names), and per this project's convention dated audit reports are point-in-time session snapshots, not living docs to retroactively rewrite.

---

# #4281 — SF-2026-09-11-D6-03: parse_fo76_plus keeps a third inline copy of the CRC-array head that #3845's consolidation does not cover, while a neighboring comment claims full coverage

**Severity**: LOW
**Dimension**: Dimension 6 — NIF Shader Blocks, BSVER 155+
**Location**: `crates/nif/src/blocks/shader.rs (parse_fo76_plus, CRC-array head)`
**Status**: NEW

## Description
`parse_fo76_plus` keeps its own inline copy of the CRC-array head, which #3845's consolidation does not actually cover — leaving a third, independent copy of logic that should be shared. A neighboring comment claims full coverage by #3845. The copy itself is behaviorally correct; the comment describing it as consolidated is stale.

## Evidence
Verified by reading `parse_fo76_plus` alongside #3845's actual consolidated call sites (`parse_fo4`, `BSEffectShaderProperty::parse`, both via `parse_skyrim_shader_base`) and confirming `parse_fo76_plus` is not among them.

## Impact
No behavioral defect today. Tech-debt / doc-rot risk: a future change to the shared CRC-array-head logic would need to remember this third, uncovered copy exists.

## Suggested Fix
Fold `parse_fo76_plus`'s inline copy into #3845's shared helper, and correct the stale comment in the meantime if the consolidation is deferred.

## Completeness Checks
- [x] **SIBLING**: Same pattern checked in related files
- [x] **TESTS**: N/A (comment-only fix; no behavior changed).

**Disposition**: Took the comment-correction path, deferring the actual consolidation. `parse_fo76_plus`'s CRC-array head genuinely differs in shape from `parse_skyrim_shader_base` (a `shader_type` field sits before the counts here, and its `num_sf2` read is unconditional rather than version-gated), and `BSLightingShaderProperty::parse`'s own doc comment states the three per-variant parsers are individually "bit-for-bit equivalent to the corresponding slice of the pre-#1279 monolithic parse" — folding one into a shared helper without an equivalent verification pass (the `parse_real_nifs --ignored` 100%-recoverable contract this file's doc calls out) is a larger, riskier change than this batch's scope. Corrected the stale "no per-BSVER jumps into shared code paths" claim in the doc table (false since #3845 — `parse_fo4` does jump into the shared helper) and added an explicit pointer comment at the actual inline copy site (`parse_fo76_plus`) recording that it is a third, independent copy not covered by #3845, with the specific shape differences that keep it from being a drop-in fold.
