# #5489: SF-2026-10-09-D4-01: Starfield pack-in REFRs spawn nothing, because template-CELL instancing is unimplemented and #5231's "0 vanilla PKIN REFRs" premise holds for FO4 only; Cydonia loses 12,218 template placements

**Labels**: bug, esm-plugin, game:starfield, high, legacy-compat

**Source**: `docs/audits/AUDIT_STARFIELD_2026-10-09.md` — finding `SF-2026-10-09-D4-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: HIGH. Visible game content is dropped under normal conditions: every Starfield cell that places a pack-in is affected (3,557 cells), including the reference cell.
- **Dimension**: ESM Resolve Rate + Cell Bring-up (streaming-area: cell load / REFR expansion)
- **Location**:
  - `byroredux/src/cell_loader/refr.rs:586-600` (every CELL-typed CNAM gets `log::warn!` + `continue`);
  - `refr.rs:529-563` (the doc says "vanilla data contains **zero PKIN-based REFRs** and this expander only ever runs on mod-authored placements");
  - `refr.rs:652-669` (`index_resolves_to_cell`, a linear scan over every CELL, justified as "a cold path — vanilla data carries zero PKIN-based REFRs");
  - call site `byroredux/src/cell_loader/references/mod.rs:613`;
  - fallthrough `byroredux/src/cell_loader/references/synth_child.rs:377`.
- **Status**: NEW. #5231 (closed, d63131c57, 2026-10-07) corrected the FO4 data model and turned the CELL-typed CNAM into an explicit skip; its census covered Fallout4.esm and the FO4 DLC only. No open or closed issue covers instancing a pack-in's template cell. The FO4 audits (10-03 → 10-09) list it under FO4 "deeper coverage", which is mod-only there. No Starfield report has flagged it; four of them (06-23, 07-25, 08-16, 08-24) list "PKIN 370" among the *resolved* Cydonia types.
- **Description**:
  - Starfield's PKIN is `wbFormIDCk(CNAM, 'Cell', [CELL])` (xEdit `wbDefinitionsSF1.pas:16001`), the same template-cell model as FO4.
  - Unlike FO4, Starfield does not bake a placed pack-in into ordinary REFRs. The REFR keeps the PKIN as its base, and the game instances the template cell's references under the REFR's transform. The PKIN flags include `Instanced` / `Instanced Static`, `wbDefinitionsSF1.pas:15983-15985`.
  - The expander skips every CNAM (all are CELLs) and returns `None`. The outer REFR then falls to `expand_scol_placements`' single-entry default. Its base is the nominal empty-model static from `parse_pkin_group`, so `synth_child.rs:377` spawns only a logical quest reference.
  - The template cell's REFRs are never read.
- **Evidence** (read-only GRUP walks of `Starfield.esm`; scripts `/tmp/audit/starfield/probe/{pkin_census,xpcs_census,pkin_template_types,cell_count}.py`):
  - **PKIN records**: 11,281. CNAM entries: 11,281, and **11,281 resolve to a CELL**.
  - **Placements**: 97,981 of 3,291,860 REFRs have a PKIN base, spread over 3,557 cells. Their template cells hold **1,211,969 child REFRs** (one level; 0 templates are empty).
  - **`citycydoniamainlevel` (0x002B3DA2)**: 27,823 own REFRs, of which 370 are PKIN-based (138 distinct PKINs). Their template cells hold **12,218 child REFRs**:
    - by base type: STAT 12,081 · PDCL 74 · nested PKIN 40 · MSTT 12 · ACTI 5 · SOUN 3 · MISC 3;
    - largest templates: `Cydonia_Sign_Directory` 363, `ClutterPI_EngineeringGreebA03` 242, `PI_StorageTank_LG01` 235, `ClutterPI_EngineeringGreebA02` 165;
    - most-placed PKIN: `SCOL_StructKit_Truss03`, ×79.
  - **The children are not baked elsewhere.**
    - Only 181 Cydonia REFRs carry `XPCS` ("Source Pack-in", xEdit SF1:7051), from 18 sources.
    - Only 8 of the 138 placed PKINs are ever an `XPCS` source in that cell.
    - Globally, 267,507 REFRs carry `XPCS`, against 1.21 M template children.
- **Impact**:
  - Cydonia renders without about 12k static pieces of its authored dressing (signage, greebles, tanks, trusses), and the same holds for every Starfield cell and exterior that places a pack-in.
  - Pack-in template cells are 11,281 of Starfield's 11,985 interior CELLs.
  - Every load also emits one `warn!` per PKIN REFR (370 on Cydonia) and runs a linear CELL scan per CNAM, over 11,985 interior + 18,732 exterior CELLs. The "cold path" premise that justifies that scan is false for Starfield.
  - Under the baseline doc's own frequency rule, this outranks PDCL (74.9% of the *unresolved* Cydonia REFRs).
- **Related**: #5231 (closed), #589 / #1180 / #2611 (expander history), SF-D4-02 (why the gate missed it), FO4 audits' "PKIN template-CELL instancing" line, ESM 10-08 table row "PKIN CNAM = template CELL".
- **Suggested Fix**:
  - Instance the template cell's `references` under the outer REFR transform, composed exactly like the SCOL arm (`GlobalTransform::compose_trs`). Recurse into nested PKINs under the shared `MAX_PKIN_DEPTH`, and memoise the expanded child list per PKIN.
  - Replace `index_resolves_to_cell`'s scan with a `CELL form_id → cell` map.
  - Correct the "zero PKIN-based REFRs" docs to say "FO4 only".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
