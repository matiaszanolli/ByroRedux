# #5422: ESM-2026-10-08-D3-01: PKIN `VNAM` is xEdit's integer *Version*, but it is documented as a form ID and routed through `remap_fid` (inverse shape 4)

**Labels**: low,esm-plugin,bug,game:fo4
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5422

**Source**: `docs/audits/AUDIT_ESM_2026-10-08.md` — `ESM-2026-10-08-D3-01` (HEAD `00f580e09`)

- **Severity**: LOW. No consumer reads it, and the vanilla values are 0/1.
- **Dimension**: FormID Remap, Load Order & ESL Space
- **Record / Sub-record**: `PKIN` / `VNAM`
- **Location**: `crates/plugin/src/esm/records/pkin.rs:26`, `:68-71` (`vnam_form_id`) and `:113-116`.
- **Status**: NEW. The pre-existing decode was surfaced by #5231's PKIN rework.
- **Description**: xEdit FO4 `PKIN` is `wbFormIDCk(CNAM, 'Cell', [CELL])` plus `wbInteger(VNAM, 'Version', itU32)`. The module doc still says "VNAM — optional u32 form ID (workshop / preview marker). Semantics not documented", and the arm calls `remap_fid`. That is the skill's inverse shape (4): a non-FormID `u32` fed to the remap.
- **Evidence**: `VNAM` census: `Fallout4.esm` {0: 741}; DLCRobot {0: 19, 1: 1}; DLCCoast / DLCworkshop03 / DLCNukaWorld all 0. The DLCRobot 1 passes through the master arm unchanged only because FO4 sits at global slot 0.
- **Impact**: A version with a non-zero high byte, or a load order where the plugin's slot-0 master is not at global 0, would be rewritten or warned through the out-of-range arm.
- **Related**: #5075/#5076 (same shape, closed), #5231.
- **Suggested Fix**: Store `version: u32` raw, cite `FO4.pas` PKIN, and drop the `remap_fid` call.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other non-FormID u32 sub-records fed to `remap_fid` (inverse shape 4), e.g. #5075/#5076 class)
- [ ] **TESTS**: A regression test pins this specific fix
