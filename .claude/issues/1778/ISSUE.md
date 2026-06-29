**Severity**: MEDIUM · **Dimension**: ESM Record Coverage · **Status**: NEW (shared FO3/FNV — surfaced by the FO3 audit)
**Location**: `crates/plugin/src/esm/records/misc/water.rs:135-209` (`decode_data`)
**Source**: `docs/audits/AUDIT_FO3_2026-06-28.md` (D3-01)

## Description

`decode_data` documents FO3/FNV `WATR.DATA` as a "60-byte prefix" with `shallow_color`@36, `deep_color`@40, `reflection_color`@44. **This does not match the on-disk data.** A record-scoped raw walk of the top-level WATR GRUP in both masters shows the DATA sub-record is *never* 60 bytes — it is either:

- **2 bytes** (Damage `u16` only): 42 FO3 / 70 FNV records — colors fall OOB and benignly default; and
- **186 bytes** (full visual params): 11 FO3 / 8 FNV records — these carry the real colors.

On the 186-byte records there is an extra `f32` (a second fog distance) at offset 36, so the real color block sits at **40 / 44 / 48**, not 36 / 40 / 44. The current code therefore reads `shallow@36` = the fog float's bytes (a garbage near-pure-blue value), and `deep`/`reflection` each shifted one slot toward shallow; the true reflection color (offset 48) is never read.

## Evidence

Hex dump of `PPurityWater01Murky` (FO3, 186-byte DATA, offset 32 onward):

```
 +32: 00 00 00 00  00 00 da 42  24 2f 24 00  0d 0d 0b 00
 +48: 29 30 2e 00  ...
```

- offset 32-35 = `00 00 00 00` → fog_far = 0.0 (read correctly as f32)
- offset 36-39 = `00 00 da 42` = **109.0** — a *float* (extra fog field), NOT a color
- offset 40-43 = `24 2f 24` = `[36, 47, 36]` — the **real shallow_color**
- offset 44-47 = `0d 0d 0b` = `[13, 13, 11]` — the **real deep_color**
- offset 48-51 = `29 30 2e` = `[41, 48, 46]` — the **real reflection_color**

Second sample `DupontFontWaterType` confirms the identical 4-byte shift. The existing test's "53/53 non-default colors" assertion passes because the misalignment always yields *a* non-zero color — which is why this slipped past prior FO3/FNV audits.

## Impact

Every FO3/FNV water body authored with a 186-byte DATA (Project Purity water, Dupont fountain, named river/pond types) renders with a wrong shallow tint and one-slot-shifted deep/reflection colors. Visible-but-cosmetic: the fog/wind/wave/reflectivity/fresnel prefix (offsets 0-35) reads correctly — only the color block past offset 35 is misaligned. Never crashes (always yields *a* non-zero color), which is why it was undetected.

## Suggested Fix

1. **Source the authoritative FO3/FNV 186-byte WATR.DATA field map from an external reference** (xEdit `wbDefinitionsFNV` `WATR` / GECK wiki) — it is NOT in the local reference repos, so **do not guess** the full layout (per the no-guessing policy).
2. The minimal, evidence-backed correction: move the three color reads from 36/40/44 to **40/44/48** and account for the missing `f32` at offset 36, gated on `data.len() >= 52`. Keep the OOB-defaults fallback so the 2-byte records are unaffected.
3. Correct the `decode_data` doc comment (lines 135-158): the "60-byte prefix identical across Oblivion/FO3/FNV" claim is false — FO3/FNV ship 2-byte or 186-byte DATA, never 60.
4. Add a regression fixture from the `PPurityWater01Murky` bytes above.

## Completeness Checks
- [ ] **SIBLING**: Check the Skyrim+ `decode_dnam_skyrim` sibling (`water.rs:216+`) for the same offset assumption; confirm the Oblivion DATA path (no fog distances) still defaults correctly.
- [ ] **TESTS**: A regression test pins the corrected 40/44/48 color offsets against the `PPurityWater01Murky` fixture; the existing "53/53 non-default colors" test is tightened so a one-slot shift fails it.
