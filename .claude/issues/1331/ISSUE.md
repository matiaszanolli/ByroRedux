**Severity:** MEDIUM · **Dimension:** Version Handling · **Game Affected:** Oblivion (and non-Bethesda Gamebryo) content authored/exported at v20.2.0.7 / user_version=11 / BSVER ≤ 26 — any file that detects as the `Fallout3` variant with in-file bsver ≤ 26.

From audit `docs/audits/AUDIT_NIF_2026-05-29.md` (finding NIF-2026-05-29-03).

## Description
nif.xml defines `NiAVObject.Flags` as `uint` for `#BSVER# #GT# 26` (line 3442) and `ushort` for `#LTE# 26` (line 3483) — a strict per-file gate. The code branches on `stream.variant().avobject_flags_u32()`, which returns `true` for the **entire** `Fallout3` variant regardless of the file's actual BSVER. An Oblivion file exported at v20.2.0.7 / uv=11 / bsver=11 detects as `Fallout3` (`version.rs:362-365`), so the helper says u32 where nif.xml wants u16.

## Location
`crates/nif/src/blocks/base.rs:75-82` (`NiAVObjectData::parse`)

## Evidence
The code's own comment (base.rs:75-77) prescribes *"Use actual BSVER from header, not the variant's hardcoded value … (e.g., Oblivion files with uv=11, bsver=11)"* — then line 78 uses `stream.variant().avobject_flags_u32()`, contradicting itself. `avobject_flags_u32()` (`version.rs:480-491`) is variant-level. Standard Oblivion (v20.0.0.4/5) is caught by the unconditional Oblivion branch *before* the uv match, so the bug is confined to the v20.2.0.7 transitional-export edge (NifSkope/newer-tool exports). `bsver::FLAGS_U32_THRESHOLD = 26` (version.rs:224) exists for exactly this gate but is unused here.

## Impact
Reads 4 bytes where 2 are on disk, slipping the stream +2 from `flags` onward; block_size realignment masks the slip (parse rate unaffected, contents wrong — same failure mode as #342). Subtly wrong NiAVObject flags (cull/visibility bits) on affected files.

## Suggested Fix
`let flags = if stream.bsver() > crate::version::bsver::FLAGS_U32_THRESHOLD { stream.read_u32_le()? } else { stream.read_u16_le()? as u32 };`. Add regression tests `(V20_2_0_7, uv=11, bsver=11)` → u16 and `(…, bsver=34)` → u32. This is the last variant-level predicate shadowing a strict per-file BSVER gate.

## Related
#1277 Task 5 (helper canonicalization), #342 (analogous masked-slip), #437 (GameVariant). **Same anti-pattern, the other surviving site:** the BSShaderNoLightingProperty falloff finding (NIF-2026-05-29-02) — fix both together.

## Completeness Checks
- [ ] **SIBLING**: Fix the twin `avobject_flags_u32()`-as-BSVER-gate misuse at `shader.rs:166` (companion finding) in the same change
- [ ] **VERSION-BAND**: Confirm `NifVariant::detect` band for `(V20_2_0_7, uv=11, bsver≤26)` and that the fix doesn't regress standard v20.0.0.x Oblivion (u16) or FO3/FNV bsver=34 (u32)
- [ ] **TESTS**: Regression tests for both the u16 (bsver=11) and u32 (bsver=34) branches added
