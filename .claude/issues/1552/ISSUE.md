**Severity**: LOW (Skyrim — unreachable) / MEDIUM if re-scoped to FO76 152–154 · **Dimension**: BSLightingShader / BSEffectShader Dispatch
**Location**: `crates/nif/src/blocks/shader.rs:1330` (the read), dispatched from `:982`, band selected at `:809-818`
**Source**: AUDIT_SKYRIM_2026-06-14 (SK-D2-01)

## Description
`BSLightingShaderProperty::parse` routes on raw header BSVER: `>= 155` → `parse_fo76_plus`; `>= 130` → `parse_fo4` (band **130..=154**); else → `parse_skyrim`. Inside `parse_shader_type_data_fo4`, `env_map_scale` is read **unconditionally** for `shader_type == 1`, while nif.xml (L6619) gates it `#NI_BS_LTE_FO4#` = `BSVER <= 139`. The two SSR bools one line below ARE correctly gated to 130–139 — only the `env_map_scale` read lacks the matching upper bound, so for BSVER 140–154 an EnvironmentMap (type-1) BSLSP over-reads 4 bytes.

## Evidence
`shader.rs:1330` reads `env_map_scale` with no `bsver <= 139` guard; the SSR bools immediately below are gated `(FALLOUT4..FO4_DLC_UPPER)` (confirmed live at `:1332-1337`). Routing band confirmed at `shader.rs:813`.

## Impact
**Skyrim SE (BSVER 100) and LE (83) both route through `parse_skyrim`, not `parse_fo4` — so Skyrim content is completely unaffected.** BSVER 140–151 is a dead band (no shipping game). BSVER 152–154 is an FO76-era edge (retail FO76 BSLSP is exactly 155, correctly routed elsewhere; 152–154 is an early/dev-build edge). Hence LOW for the Skyrim scope; MEDIUM only if an FO76 audit re-scopes the 152–154 reach.

## Related
Cross-game; surfaced here because the band is adjacent to the Skyrim path. Distinct from #1330 (BSShaderNoLightingProperty over-read on FO3/FNV bsver≤26).

## Suggested Fix
Add `if bsver <= 139` to the `env_map_scale` read to mirror the SSR bool gate immediately below it.

## Completeness Checks
- [ ] **SIBLING**: Confirm no other type arm in `parse_shader_type_data_fo4` over-reads past the FO4 upper band
- [ ] **TESTS**: A regression test pins the BSVER 140–154 type-1 stride
