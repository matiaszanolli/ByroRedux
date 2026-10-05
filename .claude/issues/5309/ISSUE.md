# #5309: TD2-2026-10-05-01: The interior and exterior CELL sub-record walkers duplicate 20 arms and their accumulator locals, and the duplication already produced a one-sided fix (#1220)

Labels: medium,tech-debt,bug,esm-plugin
Filed from: docs/audits/AUDIT_TECH_DEBT_2026-10-05.md

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-05.md` (TD2-2026-10-05-01) · audit HEAD `a2c24b16e`

- **Severity**: MEDIUM. Promotion rule: duplicated logic with a divergent bug-fix history.
- **Dimension**: 2 — Logic Duplication
- **Location**:
  - `crates/plugin/src/esm/cell/walkers.rs:215-560` (`parse_cell_group_inner`: locals at 219-260, sub-record
    arms at about 296-560)
  - `crates/plugin/src/esm/cell/wrld.rs:378-560` (`parse_wrld_children_inner`: locals at 378-420, arms at about
    456-560)
- **Status**: NEW. No open or closed issue proposes the consolidation. LEGACY_COMPAT 2026-05-19 and #1220 fixed
  only the symptom.
- **Effort**: medium
- **Description**:
  - Both walkers declare the same accumulator set (`display_name`, `water_height`, `water_height_is_explicit`,
    `image_space_form`, `water_type_form`, `acoustic_space_form`, `music_type_form`, `music_type_enum`,
    `climate_override`, `location_form`, `encounter_zone_form`, `regions`, `lighting_template_form`, the
    ownership triple, `regional_color_override`, the XCRI/XPRI precombine fields, …).
  - They then decode the same 20 sub-records: XCLW, XCIM, XCWT, XCAS, XCMO, XCMT/XCCM, XLCN, XEZN, XCLR, LTMP,
    XOWN/XRNK/XGLB, RCLR, XCRI, XPRI, FULL, …. A set difference over the two functions' `b"XXXX"` literals
    leaves only `DATA`/`XCLL` (interior) and `XCLC` (exterior).
  - The exterior copy's comments say "see the interior walker above". That walker is in a different file.
- **Evidence**:
  - **Divergent-fix history.** LEGACY_COMPAT 2026-05-19 found the exterior walker "hardcodes empty XCRI/XPRI on
    a wrong premise" after the interior one gained them. #1220 (CLOSED 2026-05-21) copied the arms across.
  - **The arms are still spelled two ways today.**
    - Interior `b"XRNK" => SubReader::new(&sub.data).i32().ok()` (`walkers.rs:418`).
    - Exterior `b"XRNK" if sub.data.len() >= 4 => Some(i32::from_le_bytes([...]))` (`wrld.rs:491-498`).
    - XOWN/XGLB carry a length guard only on the exterior side.
  - The cross-file cites have already rotted. `wrld.rs:429` and `:511` and `tests/wrld.rs:354` say the
    interior XCRI decode is "at `walkers.rs:158-190`/`158-204`". It is now at `walkers.rs:344`.
- **Impact**:
  - Every new CELL sub-record (Starfield, FO76 or modded) needs two edits.
  - #1220 shows that the second edit gets forgotten, and silent under-coverage on exteriors is the result. For
    FO4 precombines that is the headline exterior performance feature.
- **Related**: #1220, #1188 (CLOSED); LC-D3-02 (LEGACY_COMPAT today: Starfield XCLL/LGTM twin decoders, the same
  class one layer down); ESM-2026-09-21-D1-02 (skip arms duplicated across the same walkers); TD3-01 (stale
  line cites).
- **Suggested Fix**:
  - Add `struct CellSubrecordFields { … }` with `fn absorb(&mut self, reader: &EsmReader, sub: &SubRecord) -> bool`
    to `cell/helpers.rs`, where `read_form_id` and `gated_water_height` already live.
  - Each walker keeps only its own arms and falls through to `fields.absorb(...)`.
  - A single `tests/` case that feeds the same sub-record list through both walkers pins equivalence.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
