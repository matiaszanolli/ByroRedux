# Skyrim SE Compatibility Audit — 2026-05-03

**Auditor**: Claude Opus 4.7 (1M context)
**Baseline commit**: `2f8b484` (`docs: session 28 closeout — RenderLayer depth-bias ladder + lighting curves`)
**Reference report**: `docs/audits/AUDIT_SKYRIM_2026-04-24.md` (9 days ago)
**Scope**: Delta audit. The 04-24 baseline shipped 3 HIGH / 6 MEDIUM / 11 LOW; **15 of those are now closed** (cluster fix landed via #559 / #561 / #566 / #611 / #612 / #613 / #614 / #615 / #616 / #620 / #622 / #624 / #786 / etc.). This pass focuses on what changed since 04-24, what new code paths the M-NORMALS / M41 / R1 work introduce, and what's still load-bearing from the open backlog.
**Open-issue baseline**: 48 open issues at audit start (`/tmp/audit/issues.json`). 4 with the `SK-` prefix (#570 / #571 / #625 / #693) plus 1 cross-cut (#786 R-N2 — applies to Skyrim too).
**Methodology**: Direct main-context delta audit. Verified each prior-audit finding against current code state before classifying it closed; verified each new finding against current code before including it.

---

## Executive Summary

**1 HIGH · 1 MEDIUM · 0 LOW** new findings. The Skyrim path is in **the strongest state since the audit series began** — every visible Skyrim regression from 04-24 is closed, the parse rate holds at 100 % (18 862 / 18 862), and `WhiterunBanneredMare` benches at 253.3 FPS / 1932 entities @ 3.95 ms (`6a6950a` 2026-04-24, +53 % entity count vs the prior 1258-entity baseline at the same FPS).

The **single load-bearing new issue is `SK-D1-03` (HIGH)**: BSTriShape inline per-vertex tangents are still discarded (`stream.skip(4)` at `tri_shape.rs:581`) and `extract_bs_tri_shape` ships `tangents: Vec::new()` at `mesh.rs:802`. The 04-30 / 05-02 M-NORMALS work (#783) only wired authored-tangent decode for the **NiTriShape pre-Skyrim path** (Oblivion / FO3 / FNV via `NiBinaryExtraData("Tangent space …")`). Skyrim's 18,862 BSTriShape meshes get nothing — neither authored decode nor `synthesize_tangents` fallback — so when R-N2 / #786 (perturbNormal default-off workaround) reactivates, every Skyrim surface falls through Path 2 (screen-space derivative TBN) and reproduces the chrome regression on mesh boundaries. The MEDIUM `SK-D1-04` is the SSE skin-partition global-buffer sibling at `mesh.rs:1267-1269` — must be fixed in lockstep.

| Sev | Count | NEW IDs |
|--|--:|--|
| CRITICAL | 0 | — |
| HIGH | 1 | SK-D1-03 (BSTriShape inline tangent decode missing) |
| MEDIUM | 1 | SK-D1-04 (SSE skin-partition tangent decode missing — sibling of SK-D1-03) |
| LOW | 0 | — |

### What's confirmed closed since 2026-04-24

| Prior ID | Closed by | Notes |
|---|---|---|
| `SK-D5-02` (root selector) | #611 | `is_ni_node_subclass` predicate at `lib.rs:566-573` widens match to BSTreeNode, NiSwitchNode, NiLODNode, BsRangeNode, etc. Tree LODs and switch-state scenes now find their real root. |
| `SK-D5-03` (BSBoneLODExtraData) | #614 | Parser at `extra_data.rs:176`, dispatch at `blocks/mod.rs:444`, regression test at `dispatch_tests.rs:1015`. 52 actor skeleton.nif files now decode cleanly. |
| `SK-D5-04` (parser drift on 7 types) | #615 | `BSLODTriShape`, `NiStringsExtraData`, `BSLagBoneController`, `BSWaterShaderProperty`, `BSSkyShaderProperty`, `bhkBreakableConstraint`, `BSProceduralLightningController` — drift cleared, parse-rate restored to 100 %. |
| `SK-D3-04` (FO76 SkinTint kind=4) | #612 | Variant remapping landed; FO76 SkinTint multiply now reaches `triangle.frag`. |
| `SK-D1-01` (bone indices `[u8;4]`) | #613 | Multi-partition bone-index aliasing eliminated via partition-local → global remap during `NiSkinPartition` decode. |
| `SK-D2-03` (per-file embed-name flag bit 31) | #616 | `0x80000000` now extracted alongside `0x40000000` and XOR'd against archive-level `embed_file_names`. |
| `SK-D2-06` / `SK-D2-02 / -04 / -05 / -07` | #622 | BSA v105 hardening bundle (LZ4 frame size assert, `genhash_*` allocation, name-length validation, dead-field `cfg(debug_assertions)` gate). |
| `SK-D4-01` (BSEffectShaderProperty falloff) | #620 | View-angle falloff cone (start_angle/stop_angle/start_opacity/stop_opacity/soft_falloff_depth) reaches GPU via `MaterialBuffer.falloff_*`. |
| #559 (skinned-actor SSE skin-partition) | (closed) | `try_reconstruct_sse_geometry` at `mesh.rs:1129` decodes the global vertex buffer; NPC bodies / creatures / dragons now spawn. |
| #561 (multi-master CLI) | (closed) | `--master <esm>` repeatable; DLC interiors load. |
| #566 (LGTM lighting-template fallback) | (closed) | Cells without XCLL fall through to LGTM defaults. |
| #563 (BSShaderTextureSet slot routing) | #563 → 40802fe | Shader-type-aware slot routing — FaceTint/EnvMask correctly bind. |
| #624 (CELL-meta hardening) | 48b5033 | Thread-local `is_localized_plugin` gate, FULL consumer, IMGS dispatch — three-part bundle. |

### What's still open from 04-24

| Issue | Status | Notes |
|---|---|---|
| #570 `SK-D3-03` | OPEN | `MaterialInfo::material_kind` is `u8` — silently masks `shader_type >= 256`. Today no parser produces shader_type ≥ 256 (Skyrim caps at 16, FO76 at 18). Defensive open-on-overflow. |
| #571 `SK-D1-02` | OPEN | `BSDynamicTriShape` with `data_size == 0` produces renderable shape with zero triangles — silent import failure. Current FaceGen heads are NOT exercising this (M41 head spawn uses BsTriShape today, not BSDynamicTriShape). Latent. |
| #625 `SK-D4-LOW` | OPEN | `BsValueNode.value/value_flags` + `BsOrderedNode.alpha_sort_bound` discarded by `as_ni_node` walker. Latent — no consumer wires these today. |
| #693 `O3-N-05` | OPEN | CELL parser drops `XCMT` (pre-Skyrim music) + `XCCM` (Skyrim per-cell climate override). Today `parse_clmt` runs once per worldspace; per-cell overrides are silent. Visible in cells with authored XCCM (Solitude Avenues at sunset, etc.). |

### Cross-cutting from the 2026-05-03 renderer audit (apply to Skyrim too)

| Issue | Severity | Skyrim Impact |
|---|---|---|
| #785 `R-N1` | CRITICAL | UI overlay reads `materials[0].textureIndex` — not Skyrim-specific but blocks Scaleform menu rendering on Skyrim cells. Same one-line revert. |
| #786 `R-N2` | HIGH | `perturbNormal` disabled by default. **Even when re-enabled, Skyrim content has no authored tangents** — `SK-D1-03` below is the Skyrim-side gap that must close in tandem. |
| #787 `R-N3` | MEDIUM | Cotangent transform under non-uniform scale. Latent — surfaces only when #786 + `SK-D1-03` both close. |
| #788 `R-N4` | LOW | Vertex stage tangent compute waste. |

---

## RT / Rendering Assessment

**Sweetroll demo**: 18,862 individual mesh parses at 100 % clean per ROADMAP §R6a-stale (2026-04-24 bench at `6a6950a`). Not re-benched today since the 36 commits between `6a6950a` and HEAD touched no Skyrim-specific paths (R1 MaterialTable / RenderLayer / M-NORMALS land orthogonally). Expectation: same FPS within compositor-jitter band.

**WhiterunBanneredMare (interior cell)**: 253.3 FPS / 1932 entities @ 3.95 ms / RTX 4070 Ti / 1280×720 (same bench as above). Cell loading via `--master Skyrim.esm --master Update.esm --cell WhiterunBanneredMare`.

**Open architectural gap**: `SK-D1-03` below — affects shading quality only, not perf or geometry. Macro lighting (RT shadows / GI / direct lights) and cell streaming all unaffected.

---

## Findings

### Dimension 1 — BSTriShape Vertex Format

#### SK-D1-03 — BSTriShape inline tangent bytes discarded; Skyrim content gets no per-vertex tangents (HIGH, NEW)

- **Severity**: HIGH
- **Dimension**: BSTriShape Vertex Format / Shader Correctness (cross-cut)
- **Locations**:
  - `crates/nif/src/blocks/tri_shape.rs:579-583` — inline parser discards 4 tangent bytes via `stream.skip(4)` when `VF_TANGENTS` is set
  - `crates/nif/src/import/mesh.rs:792-803` — `extract_bs_tri_shape` ships `tangents: Vec::new()`; comment at line 796-801 explicitly defers ("BSTriShape and SSE-packed paths leave it empty until the inline-vertex-stream tangent decode lands as a follow-up")
  - `crates/nif/src/import/mesh.rs:447-461, 472-487` — NiTriShape and NiTriStripsData paths *do* call `extract_tangents_from_extra_data` then `synthesize_tangents` as a fallback; BSTriShape path has no equivalent
- **Status**: NEW. Tracking comment in code says "follow-up" but no GH issue exists (`gh issue list --search "BSTriShape inline tangent"` returns empty). The parent #783 is closed but the BSTriShape branch was punted explicitly.
- **Description**: M-NORMALS (#783, commit `91e9011`) wired per-vertex authored tangents end-to-end for Oblivion / FO3 / FNV NIFs that ship tangent-space data via `NiBinaryExtraData("Tangent space (binormal & tangent vectors)")`. The implementation reads the extra-data blob, runs Z-up → Y-up axis conversion + Bethesda's `tan_u`/`tan_v` swap, derives a bitangent sign, and writes `[Tx, Ty, Tz, sign]` per vertex into `ImportedMesh.tangents`. The followup commit `82a4563` adds `synthesize_tangents` (nifly-port `CalcTangentSpace`) as a fallback when the extra-data blob is absent.

  **Skyrim+ BSTriShape stores tangents inline in the packed vertex buffer**, not as a separate `NiBinaryExtraData` blob. The flag bit `VF_TANGENTS = 0x010` in `BSVertexDesc.VertexAttribute` declares "this vertex stream has 4 tangent bytes (3 × normbyte tangent + 1 × bitangent_z)". The parser at `tri_shape.rs:580-582` reads-and-discards those bytes. The importer at `extract_bs_tri_shape` then assembles `ImportedMesh` with empty tangents. No fallback to `synthesize_tangents` either.

  **Effect today**: zero observable, because R-N2 / #786 disabled `perturbNormal` by default on 2026-05-03 (commit `77aa2de`). Per-fragment normal-map perturbation is gated off until the chrome regression is RenderDoc-traced.

  **Effect when #786 closes**: every Skyrim fragment with `normalMapIdx != 0` falls through to Path 2 of `perturbNormal` (screen-space derivative TBN at `triangle.frag:603-619`) — exactly the path that produced the 2026-05-01 chrome-walls regression on FNV plaster. The R-N2 / #786 fix path needs **both** the perturbNormal sign-correction work *and* a Skyrim-side authored-tangent source.
- **Evidence**:
  ```rust
  // tri_shape.rs:579-583 — inline parser discards
  // Tangent (ByteVector3 + bitangent Z)
  if vertex_attrs & VF_TANGENTS != 0 && vertex_attrs & VF_NORMALS != 0 {
      stream.skip(4)?; // 3 bytes tangent + 1 byte bitangent Z
  }
  ```
  ```rust
  // mesh.rs:792-803 — importer ships empty tangents
  Some(ImportedMesh {
      positions,
      colors,
      normals,
      // #783 / M-NORMALS — placeholder; the NiTriShape path
      // overwrites this with the decoded `geom.tangents` below
      // (see Edit). BSTriShape and SSE-packed paths leave it empty
      // until the inline-vertex-stream tangent decode lands as a
      // follow-up; the renderer falls back to screen-space
      // derivative TBN for now on those.
      tangents: Vec::new(),
      ...
  ```
- **Impact**: When R-N2 / #786 reactivates per-fragment normal-map perturbation, every Skyrim BSTriShape surface (≈100 % of Skyrim+ content) will use Path 2 screen-space derivative TBN. Boundary discontinuities at every mesh seam reproduce the chrome regression. The vanilla `Skyrim - Meshes0/1.bsa` ships 18,862 BSTriShape meshes — **none currently feed authored tangents to the GPU**.
- **Trigger Conditions**: `VF_TANGENTS` bit set on the BSTriShape vertex descriptor (universal on Skyrim+ content with normal maps) AND R-N2 / #786 reactivated.
- **Suggested Fix**: Two-step, both at `crates/nif/src/import/mesh.rs`:
  1. **Inline decode** — change `tri_shape.rs:579-583` to *read* the 4 tangent bytes (3 × normbyte → Y-up direction, 1 byte bitangent_z → already on the normal field as bitangent_y at line 571). Store on `BsTriShape` as `tangents: Vec<[f32; 4]>` (xyz + bitangent sign reconstructed from the two bitangent bytes).
  2. **Importer wire** — replace the placeholder at `mesh.rs:802` with `tangents: shape.tangents.clone()` (or the SSE-decoded equivalent for the skin-partition path — see SK-D1-04 below). Add the `synthesize_tangents` fallback at `extract_bs_tri_shape` mirroring lines 454-461 of the NiTriShape path.

  Decode shape at `tri_shape.rs:579-583`:
  ```rust
  if vertex_attrs & VF_TANGENTS != 0 && vertex_attrs & VF_NORMALS != 0 {
      let tx = byte_to_normal(stream.read_u8()?);
      let ty = byte_to_normal(stream.read_u8()?);
      let tz = byte_to_normal(stream.read_u8()?);
      let _bitangent_z = stream.read_u8()?;  // pair with bitangent_y (line 571)
      // Compose bitangent sign from (bitangent_y, bitangent_z) — see nif.xml
      // BSVertexDesc decoder reference; combine with the byte read at line 571.
      tangents_in.push((tx, ty, tz));  // axis-conversion happens at importer
  }
  ```
- **Related**:
  - `#783` (M-NORMALS, parent — closed for NiTriShape only)
  - `#786` (R-N2, perturbNormal disabled by default) — this issue's reactivation blocker
  - `#787` (R-N3, cotangent transform) — co-blocker once #786 + this both close
  - `feedback_speculative_vulkan_fixes.md` — applies; verify the Bethesda `tan_u/tan_v` swap convention against authoritative reference (nif.xml + nifly) before shipping the decode

#### SK-D1-04 — SSE skin-partition global-buffer decoder also discards tangent bytes (MEDIUM, NEW — sibling of SK-D1-03)

- **Severity**: MEDIUM
- **Dimension**: BSTriShape Vertex Format / Shader Correctness
- **Location**: `crates/nif/src/import/mesh.rs:1266-1269` (`decode_sse_packed_buffer`)
- **Status**: NEW. Sibling site to SK-D1-03 — same bug, different code path.
- **Description**: The `decode_sse_packed_buffer` function reconstructs SSE skinned-mesh geometry from `NiSkinPartition.global_vertex_data` (the path that fixed #559's invisible NPCs). It decodes positions / UVs / normals / colors / skin payload — but at lines 1266-1269 explicitly skips tangent bytes:
  ```rust
  // Tangent: 3 × normbyte + bitangent_z. Discarded per #351.
  if vertex_attrs & VF_TANGENTS != 0 && vertex_attrs & VF_NORMALS != 0 {
      off += 4;
  }
  ```
  The `#351` reference is now stale — that issue closed before M-NORMALS made tangent decode load-bearing. Without this fix, every Skyrim NPC body, creature, and dragon (the content that goes through the SSE skin-partition reconstruction path) gets empty tangents even when SK-D1-03 lands.
- **Impact**: Co-blocks Skyrim NPC content from M-NORMALS once #786 reactivates. Without this sibling fix, fixing SK-D1-03 alone would leave NPCs with chrome highlights (Path 2 fallback) while clutter / weapons / armor (non-skin-partition path) gets correct authored tangents.
- **Trigger Conditions**: Same as SK-D1-03 (R-N2 reactivated) + the mesh has `VF_SKINNED + data_size == 0` (every Skyrim NPC body / creature).
- **Suggested Fix**: Sibling fix to SK-D1-03 — decode the 4 tangent bytes in `decode_sse_packed_buffer`, attach the decoded tangents to `DecodedPackedBuffer.tangents`, route through `try_reconstruct_sse_geometry` → `extract_bs_tri_shape` → `ImportedMesh.tangents`. Mirror the axis convention used by SK-D1-03's fix.
- **Related**: SK-D1-03 (parent — same bug, different code path); `#559` (closed — the parent `try_reconstruct_sse_geometry` work)

---

## Shader Variant Coverage Matrix (BSLightingShaderType)

Re-validated against current `crates/renderer/shaders/triangle.frag`. Status unchanged from 04-24 except for the FO76 SkinTint cell (now ✓ via #612):

| Type | Variant | Parse | Import | Render | Notes |
|----:|----|:--:|:--:|:--:|---|
| 0 | Default | ✓ | ✓ | ✓ | Baseline lit. |
| 1 | EnvironmentMap | ✓ | ✓ | partial | No dedicated branch gate; env reflection rides through Fresnel. |
| 2 | Glow | ✓ | ✓ | ✓ | |
| 3 | Parallax | ✓ | ✓ | ✓ | POM. |
| 5 | SkinTint | ✓ | ✓ | ✓ | Skyrim. |
| 6 | HairTint | ✓ | ✓ | ✓ | |
| 7 | ParallaxOcc | ✓ | ✓ | ✓ | POM. |
| 11 | MultiLayerParallax | ✓ | ✓ | ✗ stub | Tracked at #SK-D3-05 (open from 04-24, no commit). |
| 14 | SparkleSnow | ✓ | ✓ | ✓ | |
| 16 | EyeEnvmap | ✓ | ✓ | ✗ stub | Tracked at #SK-D3-05 (open from 04-24, no commit). |

FO76 SkinTint (kind=4): now ✓ via #612 (variant remap to kind=5 at importer).

---

## Forward Blocker Chain — what's needed for full Skyrim authoring fidelity

The 2026-04-24 forward chain was *"actor bodies invisible / tree LODs empty / FO76 SkinTint silently miscoloured / DLC interiors empty"*. Every link is closed.

The new forward chain (2026-05-03) is:

1. **#785 (CRITICAL)** — UI overlay one-line revert. Unblocks Scaleform menus on every cell, Skyrim included.
2. **#786 (HIGH) → SK-D1-03 + SK-D1-04 (HIGH/MEDIUM, this audit)** — reactivate perturbNormal *and* feed Skyrim authored tangents in the same change. Without the Skyrim-side decode, reactivation reproduces the chrome regression on every BSTriShape surface.
3. **#787 (MEDIUM)** — cotangent transform under non-uniform scale. Bundle with #786.
4. **#570 / #571 / #625 / #693** — pre-existing low-priority backlog. None block visible Skyrim content.

Items 1–3 take Skyrim content from "macro lighting only" (current) to "macro + bump-mapped surface detail." Items in #4 are quality-of-life / future-proofing.

---

## Verified Working — No New Gaps

- **NIF parse rate**: 100 % (18,862 / 18,862) on Meshes0+1.bsa.
- **SSE skin-partition reconstruction** (#559): `try_reconstruct_sse_geometry` decodes global vertex buffer + bone payload; NPC bodies / creatures / dragons spawn correctly. Validated by M41 NPC spawn pipeline (`d5a9d03`).
- **`is_ni_node_subclass` root selector** (#611): scenes rooted at BSTreeNode / NiSwitchNode / NiLODNode etc. correctly resolve their real root.
- **BSBoneLODExtraData** (#614): all 52 actor skeleton.nif files decode cleanly.
- **Stream-alignment drift on 7 parsers** (#615): cleared.
- **BSEffectShaderProperty falloff cone** (#620): start_angle/stop_angle/start_opacity/stop_opacity/soft_falloff_depth reach GPU via `MaterialBuffer.falloff_*`.
- **Multi-master ESM** (#561) + **LGTM fallback** (#566): DLC interiors load with correct lighting templates.
- **CELL-meta hardening** (#624): thread-local `is_localized_plugin` guarded; FULL consumed; IMGS records dispatched.
- **WhiterunBanneredMare interior**: 253.3 FPS / 1932 entities @ 3.95 ms (RTX 4070 Ti / 1280×720, bench `6a6950a` 2026-04-24, no Skyrim-path code changes since).

---

*Generated by `/audit-skyrim` on 2026-05-03. To file findings as GitHub issues: `/audit-publish docs/audits/AUDIT_SKYRIM_2026-05-03.md`.*
