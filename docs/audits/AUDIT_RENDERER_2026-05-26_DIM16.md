# Renderer Audit — 2026-05-26 (Dimension 16 only: Tangent-Space & Normal Maps)

**Scope**: single-dimension run via `/audit-renderer 16` — focused re-verification of the M-NORMALS path against the 2026-05-23 baseline. The other 19 renderer dimensions were not run today; see the 2026-05-24 sweeps for current state of the broader pipeline.

## Executive Summary

**All 4 findings from the 2026-05-23 Dim 16 sweep are FIXED-VERIFIED** on `main` as of today. Significant team progress in 3 days. Tangent-space remains healthy across all three import paths (Bethesda authored, FO4 BSTriShape inline, synthesized) and both shader fallbacks (perturbNormal Path-1 / Path-2). The only NEW finding is a LOW drift in the anisotropic-GGX paths added by #1250 — those sites omit the Gram-Schmidt orthogonalization that `perturbNormal` Path-1 performs. Latent today (every legacy NIF authors `mat.anisotropic = 0`); will manifest only when authored anisotropy lands (BGSM v22+ / synthetic hair-card content).

| Severity | NEW | CARRYOVER | FIXED-VERIFIED | INFO |
|----------|-----|-----------|----------------|------|
| Critical | 0   | 0         | 0              | 0    |
| High     | 0   | 0         | 0              | 0    |
| Medium   | 0   | 0         | 2              | 0    |
| Low      | 1   | 0         | 2              | 0    |
| **Total**| **1** | **0**   | **4**          | **0**|

## Prior-Finding Verification (delta-from-2026-05-23)

| Prior ID | Severity | Status | Fix commit | Evidence |
|----------|----------|--------|------------|----------|
| REN-D16-NEW-01 (#1104) | LOW | **FIXED-VERIFIED** | `38ba5506` | [triangle.frag:954-970](crates/renderer/shaders/triangle.frag#L954-L970) computes `screenSign = sign(dUVdx.x * dUVdy.y - dUVdx.y * dUVdy.x)` then `B = screenSign * cross(N, T)`. Path-2 carries the UV-mirror determinant sign through to bitangent (Mikkelsen convention). Mirror correction at `triangle.frag:838-847` (RT-side TBN). |
| REN-D16-NEW-02 (#1232) | MEDIUM | **FIXED-VERIFIED** | `293db681` | [bs_geometry.rs:104-126](crates/nif/src/import/mesh/bs_geometry.rs#L104-L126) now branches: UDEC3 unpack when `tangents_raw` non-empty, else `synthesize_tangents_yup` when positions/normals/uvs populated, else `Vec::new()`. Pinned by [bs_geometry_tangent_tests.rs:67-120](crates/nif/src/import/mesh/bs_geometry_tangent_tests.rs#L67-L120). |
| REN-D16-NEW-03 (#972)  | MEDIUM | **FIXED-VERIFIED** | `6b5983d4` | [refr.rs:86](byroredux/src/cell_loader/refr.rs#L86) adds `model_space_normals: bool` to `RefrTextureOverlay`. 3 population sites (`merge_from_texture_set:103-132`, XATO-only path, `apply_slot_swap:170-177`). Consumed at [spawn.rs:852-859](byroredux/src/cell_loader/spawn.rs#L852-L859). 4 regression tests at `cell_loader/refr_texture_overlay_tests.rs:333-471`. |
| REN-D16-NEW-04 | LOW | **FIXED-VERIFIED** | doc-only | [.claude/commands/audit-renderer.md:297](.claude/commands/audit-renderer.md#L297) names both lockstep tests correctly. |

## Checklist Status (today's sweep)

| # | Item | Status | Evidence |
|---|------|--------|----------|
| 1 | Bethesda authored tangents (Oblivion/FO3/FNV NiBinaryExtraData) | **PASS** | [tangent.rs:60-117](crates/nif/src/import/mesh/tangent.rs#L60-L117) reads bitangent half at offset `num_verts * 12`, #786 swap in place |
| 2 | FO4+ BSTriShape inline tangents (VF_TANGENTS \| VF_NORMALS) | **PASS** | [tri_shape/bs_tri_shape.rs:459-545](crates/nif/src/blocks/tri_shape/bs_tri_shape.rs#L459-L545), #795/#796 b63ab0c untouched |
| 3 | Synthesized fallback (Z-up + Y-up siblings) | **PASS** | `synthesize_tangents` + `synthesize_tangents_yup` both consumed (`bs_tri_shape.rs:178/193`, `bs_geometry.rs:123`) |
| 4 | Bitangent sign convention (`tangent.w`) | **PASS** | All 3 import paths pack `bitangent_sign` into `Vertex.tangent.w`; shader reconstructs at `perturbNormal:942` |
| 5 | Z-up → Y-up applied to tangent xyz in lockstep with N | **PASS** | No path converts N without T or vice versa |
| 6 | `perturbNormal` default-on, `DBG_BYPASS_NORMAL_MAP = 0x10` | **PASS** | [triangle.frag:910-974](crates/renderer/shaders/triangle.frag#L910-L974), `shader_constants_data.rs:166` |
| 7 | Permanent diagnostic bit catalog (10 bits) | **PASS** | [shader_constants_data.rs:143-224](crates/renderer/src/shader_constants_data.rs#L143-L224); lockstep tests `triangle_frag_dbg_bits_not_redeclared` + `generated_header_contains_all_defines` |
| 8 | "Chrome posterized walls" red herring (`feedback_chrome_means_missing_textures.md`) | **PASS** | No tangent-space finding proposed from chrome fragments alone |

## Findings

### REN-D16-2026-05-26-01 — Anisotropic-GGX TBN omits Gram-Schmidt — inconsistent tangent frame vs. `perturbNormal`

- **Severity**: LOW
- **Status**: NEW
- **Location**: [triangle.frag:2500-2501](crates/renderer/shaders/triangle.frag#L2500-L2501) (fallback-directional specular) + [triangle.frag:2740-2741](crates/renderer/shaders/triangle.frag#L2740-L2741) (per-light specular)
- **Related issue**: none (not yet filed)

**Evidence**

```glsl
// perturbNormal Path-1 (lines 933-944) — Gram-Schmidt before B:
vec3 T = normalize(vertexTangent.xyz);
T = normalize(T - dot(T, N) * N);          // ← orthogonalize
vec3 B = vertexTangent.w * cross(N, T);

// Anisotropic-GGX specular sites (lines 2497-2501, 2737-2741) — no Gram-Schmidt:
if (mat.anisotropic > 0.0
    && dot(fragTangent.xyz, fragTangent.xyz) > 1e-4)
{
    vec3 T = normalize(fragTangent.xyz);
    vec3 B = normalize(cross(N, T)) * fragTangent.w;  // ← T may not be ⟂ N
    float HdotX = dot(H, T);
    float HdotY = dot(H, B);
    ...
}
```

**Impact**

When the per-vertex authored T is not exactly perpendicular to the interpolated per-fragment N (normal across smoothing-group seams — the very condition Gram-Schmidt was added to handle in `perturbNormal`), the anisotropic specular projection uses a tilted T while the normal-map sample uses an orthogonalized T. The anisotropic GGX lobe orientation is slightly off-axis relative to the bump-mapped normal.

Visually: subtle directional shift on hair / brushed-metal / hair-card surfaces near smoothing-group seams.

**Currently latent**: `mat.anisotropic` is zero on every legacy NIF — #1250 added the path in preparation for hair/brushed-metal but no authored anisotropy flows in yet. Will manifest only when authored anisotropy lands (BGSM v22+ or synthetic hair-card paths).

**Fix sketch**

Mirror `perturbNormal`'s Gram-Schmidt at both sites. Single-line addition:

```glsl
vec3 T = normalize(fragTangent.xyz);
T = normalize(T - dot(T, N) * N);          // ← add this
vec3 B = normalize(cross(N, T)) * fragTangent.w;
```

After Gram-Schmidt T is unit-length and ⟂ N, so the inner `normalize` on B becomes redundant but harmless.

**Architectural recommendation**: extract a `mat3 buildTBN(vec3 N, vec4 vertexTangent)` helper so the **4 call sites** (perturbNormal Path-1 + perturbNormal Path-2 + anisotropic specular fallback-directional + anisotropic specular per-light + RT-side TBN at `:2501`) share one definition. Would have prevented both #1104 and this finding — see cross-cutting note below.

## Cross-cutting notes

- **TBN reconstruction duplication is a maintenance hazard**. Four sites in `triangle.frag` reconstruct a TBN from `(N, vertexTangent)`:
  - `perturbNormal` Path-1 (line 933-944) — has Gram-Schmidt ✓
  - `perturbNormal` Path-2 (line 954-970) — sign-aware after #1104 fix ✓
  - Anisotropic specular fallback-directional (line 2497-2501) — **missing Gram-Schmidt** ✗ (REN-D16-2026-05-26-01)
  - Anisotropic specular per-light (line 2737-2741) — **missing Gram-Schmidt** ✗ (REN-D16-2026-05-26-01)

  #1104 was Path-2 drifting from Path-1's UV-mirror convention. REN-D16-2026-05-26-01 is the same class of bug — Path-1 has Gram-Schmidt, the anisotropic paths don't. A shared `buildTBN` helper would consolidate them.

- **Shader-side flag rename drift** (informational): Stage 3 shader refactor `ae364e29` renamed `MAT_FLAG_BGSM_MODEL_SPACE_NORMALS` → `MAT_FLAG_MODEL_SPACE_NORMALS` (dropped the `BGSM_` prefix). The audit-renderer skill prompt + this report's checklist item 7 still reference the old name. Not a code bug; a documentation-drift trigger for the next sweep. The `DBG_*` bit catalog is unaffected (catalog uses `DBG_` prefix, not `BGSM_`).

- **BS-tri-shape inline tangent decode** at `blocks/tri_shape/bs_tri_shape.rs:459-545` (#795 / #796) is untouched since the prior audit; no regression. `VF_NORMALS = 0x008` and `VF_TANGENTS = 0x010` still pinned at lines 214-215.

## Methodology

1. Read the 2026-05-23 Dim 16 audit baseline (`docs/audits/AUDIT_RENDERER_2026-05-23_DIM16.md`).
2. `git log --since=2026-05-23 -- crates/nif/src/import/mesh/ crates/renderer/shaders/triangle.frag crates/plugin/src/esm/cell/mod.rs byroredux/src/cell_loader/refr.rs` to identify fix commits.
3. Per-finding code re-read at the cited file:line locations; matched against the prior audit's "suggested fix" sketches to confirm semantic equivalence (not just structural change).
4. Full Dim 16 checklist walk for new drift; cross-referenced shader sites that consume `vertexTangent` to find the 4 TBN-reconstruction copies.
5. Dedup baseline at `/tmp/audit/renderer/issues.json` — no existing issue covers REN-D16-2026-05-26-01.

---

Suggest: `/audit-publish docs/audits/AUDIT_RENDERER_2026-05-26_DIM16.md`
