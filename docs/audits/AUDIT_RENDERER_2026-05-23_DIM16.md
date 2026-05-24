# Renderer Audit — 2026-05-23, Dim 16 focus

**Focus**: `--focus 16` (Tangent-Space & Normal Maps — M-NORMALS).
**Depth**: deep.
**Trigger**: focused follow-up sweep after the 2026-05-16 audit closed 27 of its 29 findings via #1081–#1119; only #1104 (REN-D16-002, Path-2 UV-mirror handedness) carried forward in the tangent-space area. This audit re-verifies that carry-over and walks the import paths end-to-end with the post-#1118 split layout to surface any drift since #1086/#1204/#1147 Phase 2b closed.
**Prior base**: `AUDIT_RENDERER_2026-05-19.md` (24 findings, 23 LOW + 1 MEDIUM; no Dim 16 entries).
**Open Dim 16 issues at audit start**: #1104 (Path-2 UV-mirror handedness), #972 (TXST HasModelSpaceNormals flow).

## Executive Summary

| Severity | Count | Status |
|----------|-------|--------|
| CRITICAL | 0 | — |
| HIGH     | 0 | — |
| MEDIUM   | 2 | 1 NEW, 1 existing (#972) |
| LOW      | 2 | 1 NEW (audit-skill doc drift), 1 existing (#1104) |
| **Total** | **4** | 2 NEW, 2 carry-over |

**Pipeline areas affected**: NIF import (BSGeometry path), REFR texture overlay (cell loader), shader (Path 2 fallback), audit-skill maintenance.

**Headline**: The tangent-space pipeline is healthy. Path 1 (authored Bethesda + FO4+ inline + Skyrim+ inline tangents) is sound; the #786 / #795 / #796 fix-chain is still in place and covered by tangent-convention tests. Two MEDIUMs are wiring follow-ups that were missed in prior closeout batches: BSGeometry's no-tangent path never got routed through `synthesize_tangents_yup` after that function landed for the BSTriShape SSE-recon case (#1204), and the TXST `HasModelSpaceNormals` flag (parsed since #814) still drops on the floor at `merge_from_texture_set` even though the shader has been ready to consume it since #1147 Phase 2b.

## Findings (grouped by severity, CRITICAL first)

### MEDIUM

#### REN-D16-NEW-02: BSGeometry no-tangent fallback returns `Vec::new()` instead of routing through `synthesize_tangents_yup`

- **Severity**: MEDIUM
- **Dimension**: Tangent-Space & Normal Maps
- **Location**: [crates/nif/src/import/mesh/bs_geometry.rs:113-120](../../crates/nif/src/import/mesh/bs_geometry.rs#L113-L120)
- **Status**: NEW
- **Description**: BSGeometry's tangent extraction has two paths today:
  1. `mesh_data.tangents_raw` non-empty → UDEC3-unpack via the #1086 fix.
  2. `tangents_raw` empty → return `Vec::new()`, with a stale comment "A future improvement could call synthesize_tangents here, but it requires a Y-up variant since BSGeometry data is already in engine space."

  That Y-up variant ([`synthesize_tangents_yup` at tangent.rs:376-516](../../crates/nif/src/import/mesh/tangent.rs#L376-L516)) now ships and is consumed by the BSTriShape SSE-reconstructed branch ([bs_tri_shape.rs:178-189](../../crates/nif/src/import/mesh/bs_tri_shape.rs#L178-L189)) under #1204 — but the BSGeometry slot was missed in that follow-up. Every Starfield mesh without authored UDEC3 tangents currently falls through to Path 2 in `perturbNormal` and inherits the #1104 UV-mirror sign bug as a consequence.
- **Evidence**:
  ```rust
  let tangents: Vec<[f32; 4]> = if !mesh_data.tangents_raw.is_empty() {
      mesh_data.tangents_raw.iter().map(|&raw| { … }).collect()
  } else {
      // No authored tangents — the renderer falls back to screen-space
      // derivative TBN (Path 2). A future improvement could call
      // synthesize_tangents here, but it requires a Y-up variant since
      // BSGeometry data is already in engine space …
      Vec::new()
  };
  ```
- **Impact**: Lower-quality normal maps on Starfield meshes that omit UDEC3 tangents (vanilla `Saturn.nif` ships authored tangents but mod content + some LOD chains do not). Inverted normals on mirrored UV shells in such content (chains to REN-D16-NEW-01 / #1104). Closing this turns those meshes from Path-2 fallback into Path-1.
- **Related**: #1086 (closed — original UDEC3 decode), #1204 (closed — sibling BSTriShape SSE-recon fix), #1104 (open Path-2 sign bug — partially mitigated by closing this finding).
- **Suggested Fix**: Mirror the BSTriShape Path-D branch in [bs_tri_shape.rs:178-189](../../crates/nif/src/import/mesh/bs_tri_shape.rs#L178-L189):
  ```rust
  } else if !normals.is_empty() && !uvs.is_empty() && !positions.is_empty() {
      let triangles_u16: Vec<[u16; 3]> = indices
          .chunks_exact(3)
          .filter_map(|c| {
              if c[0] <= u16::MAX as u32 && c[1] <= u16::MAX as u32 && c[2] <= u16::MAX as u32 {
                  Some([c[0] as u16, c[1] as u16, c[2] as u16])
              } else { None }
          })
          .collect();
      synthesize_tangents_yup(&positions, &normals, &uvs, &triangles_u16)
  } else {
      Vec::new()
  };
  ```
  Strip the stale "future improvement" comment in the same hunk. A `bs_geometry_tangent_tests.rs` test asserting `synthesize_tangents_yup` runs when `tangents_raw` is empty would lock the wiring.

---

#### REN-D16-NEW-03: TXST `HasModelSpaceNormals` flag still not propagated into `mesh.model_space_normals` — orphaned wiring

- **Severity**: MEDIUM
- **Dimension**: Tangent-Space & Normal Maps
- **Location**: [byroredux/src/cell_loader/refr.rs:93-103](../../byroredux/src/cell_loader/refr.rs#L93-L103) (parser source: [crates/plugin/src/esm/cell/mod.rs:623-632](../../crates/plugin/src/esm/cell/mod.rs#L623-L632))
- **Status**: Existing: #972 (still OPEN; re-verified live in this audit)
- **Description**: TXST DNAM bit 2 (FO4+) is `HasModelSpaceNormals`. The parser correctly captures it into `TextureSet.flags: u16` and there's a documented intent that "The renderer's normal-map decode path branches on `HasModelSpaceNormals` once it consumes this field." The shader-side branch IS now in place — [triangle.frag:993-1000](../../crates/renderer/shaders/triangle.frag#L993-L1000) consumes `MAT_FLAG_BGSM_MODEL_SPACE_NORMALS` to skip the TBN multiply when the source normal map is model-space. But the BGSM source (`mesh.model_space_normals`) is the *only* path that populates that flag today; the TXST source is dropped on the floor at `merge_from_texture_set`:
  ```rust
  fn merge_from_texture_set(&mut self, ts: &esm::cell::TextureSet, pool: &mut StringPool) {
      Self::fill(&mut self.diffuse, ts.diffuse.as_deref(), pool);
      Self::fill(&mut self.normal,  ts.normal.as_deref(),  pool);
      …
      Self::fill(&mut self.material_path, ts.material_path.as_deref(), pool);
      //                                            ↑ ts.flags is never read
  }
  ```
- **Impact**: FO4 meshes whose model-space-normal hint comes from the *TXST* (not BGSM) — e.g., REFRs with XATO override carrying a model-space-normal-flagged TXST, or non-BGSM static-mesh content — silently render with a tangent-space TBN multiply on a model-space normal map. The double-rotation produces darkening / inversion patterns on the affected surfaces. The wiring asymmetry is the bug: shader is ready, BGSM source feeds it, TXST source drops on the floor.
- **Suggested Fix**: Plumb `ts.flags & 0x04` into `RefrTextureOverlay` (add a `model_space_normals: Option<bool>` field, fill in `merge_from_texture_set` and `build_refr_texture_overlay`), then OR it into `mesh.model_space_normals` at the overlay-apply site. The pack site at [cell_loader.rs:189-190](../../byroredux/src/cell_loader.rs#L189-L190) already converts `mesh.model_space_normals` → `MAT_FLAG_BGSM_MODEL_SPACE_NORMALS` so no shader-side work is needed. A regression test should verify a TXST with bit 2 set on its DNAM flows into the final `GpuMaterial.materialFlags`.

---

### LOW

#### REN-D16-NEW-01: Path-2 screen-space derivative bitangent ignores UV-mirror sign — inverted normals on mirrored shells

- **Severity**: LOW (carry-over)
- **Dimension**: Tangent-Space & Normal Maps
- **Location**: [crates/renderer/shaders/triangle.frag:761-766](../../crates/renderer/shaders/triangle.frag#L761-L766)
- **Status**: Existing: #1104 (still OPEN; re-verified live)
- **Description**: Path 2 (no authored tangent) computes a sign-aware `B = normalize(dPdy * dUVdx.x - dPdx * dUVdy.x)` at line 762, then immediately overwrites it with `B = cross(N, T)` at line 766 — discarding the UV-mirror sign. The 2026-05-15 and 2026-05-16 audits both flagged this; no code delta since.
- **Evidence**:
  ```glsl
  vec3 T = normalize(dPdx * dUVdy.y - dPdy * dUVdx.y);
  vec3 B = normalize(dPdy * dUVdx.x - dPdx * dUVdy.x);  // ← sign captured here
  T = normalize(T - dot(T, N) * N);
  B = cross(N, T);                                       // ← sign discarded here
  ```
- **Impact**: Subtle lighting inversion on mirrored UV shells (faces, symmetric props). Most Bethesda content uses authored tangents (Path 1) so the screen-space fallback is rare. Confined to BSGeometry meshes lacking `tangents_raw` (Starfield — see REN-D16-NEW-02 chain) and any future synthetic / particle content that arrives without authored or synthesized tangents.
- **Suggested Fix**:
  ```glsl
  float screenSign = sign(dUVdx.x * dUVdy.y - dUVdx.y * dUVdy.x);
  B = screenSign * cross(N, T);
  ```
  Matches the Mikkelsen convention. Single-line change in `triangle.frag`; recompile shader.

---

#### REN-D16-NEW-04: Audit skill prompt references stale test name `triangle_frag_dbg_bits_match`

- **Severity**: LOW
- **Dimension**: Tangent-Space & Normal Maps (audit-skill maintenance)
- **Location**: `.claude/commands/audit-renderer.md` Dimension 16 checklist (this skill file)
- **Status**: NEW
- **Description**: The Dim 16 prompt names the lockstep test `crates/renderer/src/shader_constants.rs::tests::triangle_frag_dbg_bits_match`, but the live test is `triangle_frag_dbg_bits_not_redeclared` (post-#1162 / TD4-206 rename). The validation gate at `.claude/commands/_audit-validate.sh` only checks backticked *paths*, not symbol names, so this drift slipped through. Sibling pattern of #1229 (4 stale `crates/nif/src/blocks/tri_shape.rs` refs in audit skill files post-#1118 split).
- **Evidence**: `grep "fn triangle_frag_dbg_bits" crates/renderer/src/shader_constants.rs` returns the single hit `fn triangle_frag_dbg_bits_not_redeclared`. The Rust source-of-truth catalog at `shader_constants_data.rs:118-200` isn't covered by a positive "match" test; the relevant lockstep checks are `triangle_frag_dbg_bits_not_redeclared` (negative — no `const uint` redeclaration) + `generated_header_contains_all_defines` (positive — each `#define` emitted with the right value). The audit prompt's wording suggests a hypothetical positive-match test that doesn't exist.
- **Impact**: An auditor checking the claim runs `cargo test triangle_frag_dbg_bits_match`, gets zero results, and either (a) believes the lockstep is missing → files a false-positive finding, or (b) digs through the file to find the real test name. Either path wastes auditor time. Same failure mode as the #1229 stale-path findings.
- **Suggested Fix**: Update the audit-skill text to name both real tests — `triangle_frag_dbg_bits_not_redeclared` + `generated_header_contains_all_defines` — and describe their division of labour (one negative, one positive). Consider extending `_audit-validate.sh` to validate referenced test/symbol names (LOW on its own; bundle with #1229 follow-up if that lands a structural fix).

---

## Did-not-find (negative coverage)

- **#786 fix in place** — `extract_tangents_from_extra_data` still reads Bethesda's bitangent half into `Vertex.tangent.xyz` and uses the tangent half to derive `bitangent_sign`. No regression. [tangent.rs:72-82](../../crates/nif/src/import/mesh/tangent.rs#L72-L82).
- **BSTriShape inline tangent reassembly (#795 / #796)** — correct: all three bitangent slots captured + reassembled in [blocks/tri_shape/bs_tri_shape.rs:447-545](../../crates/nif/src/blocks/tri_shape/bs_tri_shape.rs#L447-L545). Sign derivation on raw Z-up values is rotation-invariant.
- **Z-up → Y-up coord conversion** — applied consistently across Path A ([tangent.rs:91-92](../../crates/nif/src/import/mesh/tangent.rs#L91-L92)), Path B (`bs_tangents_zup_to_yup`), Path C (per-vertex finalize at [tangent.rs:293-294](../../crates/nif/src/import/mesh/tangent.rs#L293-L294)). No path converts N but not T or vice versa.
- **BGSM-side `model_space_normals` end-to-end** — confirmed wired from `BgsmFile.model_space_normals` → `pack_bgsm_material_flags` → `MAT_FLAG_BGSM_MODEL_SPACE_NORMALS` → shader branch ([triangle.frag:993-1000](../../crates/renderer/shaders/triangle.frag#L993-L1000)).
- **DBG_* bit catalog** — all 10 bits present in [shader_constants_data.rs:118-200](../../crates/renderer/src/shader_constants_data.rs#L118-L200), lockstep test in place ([shader_constants.rs:189-211](../../crates/renderer/src/shader_constants.rs#L189-L211)), no orphans.
- **Tangent-convention tests** — `synthesize_tangents_stores_dpdu_not_dpdv`, `synthesize_tangents_flips_bitangent_sign_on_mirrored_uvs`, plus Y-up siblings cover the canonical invariants. All passing (686 nif tests pass).
- **"Chrome posterized walls" red herring** — verified that the running advice from `feedback_chrome_means_missing_textures.md` is honored across the codebase: chrome artifacts are diagnosed as missing-texture + valid-normal-map products, not as tangent-space bugs. No finding here.

## Prioritized Fix Order

1. **REN-D16-NEW-02** (MEDIUM, NEW) — one-screen change at `bs_geometry.rs:113-119`, mirror the BSTriShape-side fix, strip the stale TODO. Direct quality win on Starfield content; closes the chain that makes #1104 visible on Starfield meshes.
2. **REN-D16-NEW-03** (MEDIUM, #972) — wire `TextureSet.flags & 0x04` into `RefrTextureOverlay` → `mesh.model_space_normals`. Shader half already in place; only the data-flow gap.
3. **REN-D16-NEW-01** (LOW, #1104) — one-line shader fix. Lowest blast radius; defer behind the two MEDIUMs.
4. **REN-D16-NEW-04** (LOW, NEW) — text fix in audit skill. Trivial.

## Methodology notes

- All 4 import paths walked: Bethesda authored (Path A), FO4+ inline (Path B), CalcTangentSpace synthesis Z-up (Path C), Y-up sibling (Path D). Plus shader-side Path 1 (authored) and Path 2 (screen-space derivative fallback).
- Cross-checked claim "#786 fix in place" against [tangent.rs:60-82](../../crates/nif/src/import/mesh/tangent.rs#L60-L82) — comment block at lines 60-82 documents the swap convention and the regression's history; code reads the bitangent half at offset `num_verts * 12` as expected.
- Cross-checked #1086 BSGeometry UDEC3 decode + #1204 BSTriShape SSE-recon Y-up synthesis. Both in place; gap is the BSGeometry no-tangent fallback that didn't get the #1204 sibling treatment.
- Verified #972 still open by inspecting `merge_from_texture_set` at [refr.rs:93-103](../../byroredux/src/cell_loader/refr.rs#L93-L103) — `ts.flags` not read; the `RefrTextureOverlay` struct has no equivalent field.
- Did not run `tex.missing` against a live scene (audit-skill rule honored — but no chrome-artifact finding proposed here anyway; the "chrome means missing textures" feedback memo is correctly internalized).
- 686 nif tests + workspace test suite green at audit start (re-verified after read-only exploration).
