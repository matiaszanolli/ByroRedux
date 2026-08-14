# REN-D2-02: shader-pipeline.md GpuLight rows document a removed shadow_policy; params.z is the live cull mask

- **Issue**: [#2917](https://github.com/matiaszanolli/ByroRedux/issues/2917)
- **Finding ID**: `REN-D2-02`
- **Labels**: `medium,renderer,documentation`
- **Source report**: [`docs/audits/AUDIT_RENDERER_2026-08-14.md`](../../../docs/audits/AUDIT_RENDERER_2026-08-14.md)
- **Run**: `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2917 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Ray Queries
- **Location**: `docs/engine/shader-pipeline.md` (`### GpuLight — 64 bytes, SSBO (Set 1,
  Binding 0)` table, rows at offsets 56 and 60–63, plus the `type` row); live contract:
  `GpuLight::params` in `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs`,
  `decodeVisibilityMask` / `visibilityOpaqueMask` / `visibilityMaskNeedsTrace` in
  `crates/renderer/shaders/include/shadow_common.glsl`
- **Status**: NEW (re-drift after #2252's fix — the doc side is now wrong in a *different* way)
- **Description**: The audit-designated authoritative GPU-layout doc describes offset 56 as
  `shadow_policy` "`SHADOW_POLICY_*` encoded as f32 — see `decodeShadowPolicy` in
  `shadow_common.glsl`", and offsets 60–63 as *(reserved)*. Neither is true. `5798e467`
  (2026-08-09, "Refactor visibility layers and adaptive ray budget management") replaced the
  shadow-policy encoding with the `VisibilityMask` bitfield: `params.z` is now the explicit
  visibility-layer mask that `decodeVisibilityMask` turns into the **`cullMask` argument of
  every `rayQueryInitializeEXT` on the direct-shadow path**, and `params.w` carries the
  `AttenuationModel` discriminant that `pointSpotAtten` branches on. No symbol named
  `decodeShadowPolicy` or `SHADOW_POLICY_*` exists anywhere in `crates/renderer/src` or
  `crates/renderer/shaders` — the only surviving `SHADOW_POLICY_NONE` mentions are two prose
  comments. The doc's `type` row ("0 = point, 1 = spot, 2 = directional") also omits type 3
  (ambient fill), which `giLightSample` explicitly rejects with `if (lightType > 2.5) return
  false;` and which `bindings.glsl` does document.
- **Evidence**:
  - `gpu_types.rs`: "`x` = legacy attenuation exponent; `y` = finite luminous-source radius;
    `z` = explicit `VisibilityMask` bits encoded as an exact f32 integer; `w` =
    `AttenuationModel` discriminant encoded as f32."
  - `shadow_common.glsl`: `uint decodeVisibilityMask(float encodedMask) { return
    uint(max(floor(encodedMask + 0.5), 0.0)) & VISIBILITY_MASK_FULL; }` — and
    `traceShadowTransmittance` / `traceShadowBinary` take that value as the `cullMask`.
  - `grep -rn "decodeShadowPolicy\|SHADOW_POLICY" crates/renderer/shaders/ crates/renderer/src/`
    returns only comment text (`triangle.frag` and `restir.rs`) plus the doc line itself.
  - `git log -S` confirms the ordering: `d2333818` (2026-08-02) fixed #2252 by writing the
    shadow-policy rows; `5798e467` (2026-08-09) changed the code without touching the doc.
- **Impact**: The doc is the stated reference for anyone touching a ray-query cull mask. A
  reader following it would look for a non-existent decoder, treat `params.w` as free padding
  (it is the live attenuation-model selector — writing there changes every point/spot light's
  falloff curve), and miss that `params.z` is a *layer bitfield* whose bits must line up with
  `shadow_mask_for_instance`'s TLAS-side buckets in `acceleration/tlas.rs`. Not a runtime
  defect; a wrong entry in the contract that the severity guidance explicitly says not to
  treat as a typo.
- **Related**: #2252 (the previous fix of these same rows), `5798e467`, #2781 (OPEN — the
  sibling drift on the binding-11 row of the same doc).
- **Suggested Fix**: Rewrite the offset-56/60 rows as `visibility_mask` (`VISIBILITY_LAYER_*`
  bits, consumed as the ray-query `cullMask` via `decodeVisibilityMask`) and
  `attenuation_model` (`ATTENUATION_MODEL_*`), and add type 3 (ambient fill, never a GI/shadow
  candidate) to the `type` row.

---

## Completeness Checks
- [ ] **SIBLING**: The same doc table / anchor class is swept, not just the one row cited
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_RENDERER_2026-08-14.md`](docs/audits/AUDIT_RENDERER_2026-08-14.md) — `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`. Verified CONFIRMED against current code at publish time.*
