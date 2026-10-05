# #5346: LC-D3-02 (2026-10-05): The Starfield 108-byte lighting tail has two hand-copied decoders (XCLL and LGTM) and two hand-copied unit lifts

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5346
- **Labels**: low,legacy-compat,esm-plugin,tech-debt,bug,game:starfield
- **Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-10-05.md` (LC-D3-02)

_From `docs/audits/AUDIT_LEGACY_COMPAT_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW
- **Dimension**: 3 — Cross-game translation pattern (Pattern C shape: one per-game wire structure decoded at two sites)
- **Location**:
  - Decoders: `crates/plugin/src/esm/cell/walkers.rs:496-557` (the SF XCLL arm) and `crates/plugin/src/esm/records/misc/world.rs:1320-1364` (the SF LGTM arm, `#5002`).
  - Lifts: `crates/plugin/src/esm/records/spatial_units.rs:28-39` (`lighting()`, CellLighting) and `:138-152` (the `lighting_templates` loop).
- **Status**: NEW. `#5171` (open) covers only the *missing test* for the LGTM lift, and `AUDIT_ESM_2026-10-05` raises only the byte-28 label question. Neither covers the duplication.
- **Description**:
  - `8347fde67` fixed `#5002` by copying the SF XCLL offset table into `parse_lgtm`. Its commit message says "decode the SF tail with the same offset table as the SF XCLL arm".
  - The result is two independent 20-field decoders of bytes 28..108 (gravity scale, fog clip/power, far colour, fog max, light fades, the height-fog model, interior type) that build the same `StarfieldLighting`.
  - It also copied the four-line height-lift block into the `lighting_templates` loop, with the comment "lifts exactly like the XCLL one in `lighting()` above", rather than sharing `lighting()`.
  - The copies agree today. The XCLL arm uses `unwrap_or` / `f32_or_default` and the LGTM arm uses `.ok()`, which is equivalent under the `>= 108` guard.
- **Evidence**: The same offset comments appear at both sites:
  ```rust
  // walkers.rs:497-516                         // world.rs:1323-1341
  let gravity_scale = r.f32_or_default(); // 28  let gravity_scale = tail.f32_or_default(); // 28
  ...                                             ...
  let interior_type = r.u8_or_default(); // 104  let interior_type = tail.u8_or_default(); // 104
  ```
- **Impact**:
  - The risk is drift. Today's ESM report already disputes the byte-28 / 56-59 labels between xEdit's SF1 LGTM and XCLL definitions.
  - Any future correction (a label, a new SF DLC length, a lift change) must be made twice, or Starfield interiors (XCLL) and lighting templates (LGTM) will diverge. This is the "fix kept at only one site" failure that `#1044` collapsed for coordinates.
  - The lift twin is unpinned (`#5171`), so a drift there would be silent.
- **Related**: `#5002` (closed), `#5171` (open, the LGTM-lift test gap), `#1293` / `#1579` (the SF XCLL arm), ESM-2026-10-05 (the byte-28 label note).
- **Suggested Fix**:
  - Extract `decode_starfield_lighting_tail(&[u8]) -> (base fields, StarfieldLighting)` in `esm::cell` and call it from both arms.
  - Make the template lift reuse `lighting()`, either by giving `LgtmRecord` an embedded `CellLighting`-shaped core or by sharing a helper that lifts `StarfieldLighting` and the five base distances.
  - Pin both with the `#5171` test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
