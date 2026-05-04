# AUDIT_RENDERER — 2026-05-03

**Auditor**: Claude Opus 4.7 (1M context)
**Baseline commit**: `2f8b484` (`docs: session 28 closeout — RenderLayer depth-bias ladder + lighting curves`)
**Reference report**: `docs/audits/AUDIT_RENDERER_2026-05-01.md` (15-dim general audit) + `docs/audits/AUDIT_RENDERER_2026-05-01_FOCUS.md` (lighting regression focus)
**Scope**: Delta audit since 2026-05-01. 36 commits / +1280 / -166 LoC across renderer + app since R1 closeout (`22f294a`). 15 dimensions covered (Sync, GPU Memory, Pipeline, Render Pass, Cmd Recording, Shaders, Resource Lifecycle, AS/RT, Ray Queries, Denoiser, TAA, Skinning, Caustics, Material Table R1, Sky/Weather/Exterior).
**Open-issue baseline**: 49 open at 2026-05-01 → 51 open at audit start (`/tmp/audit/renderer/issues.json`).
**Methodology**: Direct main-context delta audit. Anchored on per-file `git log 22f294a..HEAD` to identify newly-added code paths; each delta read against the live tree. Per the 2026-05-01 methodology note, sub-agent dispatches reliably stall on this size of audit; direct read is more efficient and produces a deterministic deliverable.

---

## Executive Summary

**1 CRITICAL · 1 HIGH · 1 MEDIUM · 1 LOW · 0 INFO** — across 4 new findings. One CRITICAL is a **regression of last audit's `R1-N1`** that was closed two days ago and re-introduced by an unrelated commit's stale shader hunk.

The dominant changes since 05-01:

1. **Lighting regression chain closure** — `LIGHT-N1` (#782, interior fog leak, commit `c248a99`), `LIGHT-N2` (#784, fog mix in HDR-linear pre-ACES, commit `18bbeae`), and `M-NORMALS` (#783, per-vertex tangents from NIF, commits `91e9011` + `82a4563`) all landed. Composite fog is now mixed in display space; interior cells preserve their XCLL/LGTM-authored fog; the `Vertex` struct gained a tangent slot (84 → 100 B / 21 → 25 floats) with matching skin-compute stride bump.
2. **`perturbNormal` disabled by default** (commit `77aa2de`) — bump-mapping is OFF on the default render path until the chrome-on-walls regression is properly diagnosed via RenderDoc. New `DBG_FORCE_NORMAL_MAP = 0x20` opts in for testing. The M-NORMALS infrastructure (Vertex layout, decoder, vertex+fragment paths, skin-compute stride) all stays wired — this is a one-bit flip workaround.
3. **`RenderLayer` ECS component + per-layer depth-bias ladder** (commits `088696e` / `c515028` / `0f13ff5` / `ee3cb13`) — replaces the ad-hoc `is_decal || alpha_test_func != 0` heuristic. Architecture (0,0,0) / Clutter (-16,0,-1) / Actor (-32,0,-1.5) / Decal (-64,0,-2) ladder applied via `vkCmdSetDepthBias` per state change. Spawn-time small-STAT-radius escalation reclassifies decorative props (paper piles, folders) from Architecture to Clutter. `INSTANCE_RENDER_LAYER_SHIFT/MASK` packs layer into `GpuInstance.flags` bits 4..5 for debug-viz.
4. **Frostbite smooth light falloff** (commit `78632a6`) — point/spot window curve `1 - (d/r)²` → `(1 - (d/r)⁴)²`. Cull-radius shoulder no longer visible on floors. `c1`-continuous at the cull radius.
5. **`env_map_scale` no longer treated as metalness** (commit `8038ae7`) — `Material::classify_pbr` stops routing dielectrics-with-sheen (cushions, glass, varnished wood) into the metal-reflection branch.

**Most prior-audit open issues (Sync2 BOTTOM_OF_PIPE chain, LIFE-N2, DEN-2/5/9, SH-5, MEM-2-7/8, RT-8, RT-11/12/14, SY-4, PS-6..9, DEN-11/12, SH-13) remain open.** No movement on those since 05-01. The 2026-05-01 report's "Prior-audit backlog" table applies verbatim today; only `R1-N1`'s status changed (closed → regressed).

### What's new since 05-01

- **R1 polish + close-out**: `9c7ea0d` (#776), `a2bb016` (#778), `62a266f` (#777) — the three R1-residual issues from 05-01 closed within hours of audit publication. Then **regressed** within four hours when `c248a99`'s diff inadvertently included a stale `ui.vert` hunk from the Phase-5 state.
- **`#renderlayer`**: new ECS component + ladder; tests pin every threshold.
- **Workaround chain on M-NORMALS**: 8305456 → 91e9011 → 82a4563 → 77aa2de. Net result: authored tangents decode, vertex shader pipes them through, fragment shader gates `perturbNormal` off by default.
- **#695 (vertex-color emissive)** — `MAT_FLAG_VERTEX_COLOR_EMISSIVE` bit landed; `_pad_pbr` repurposed as `material_flags`. Layout-pinning test still passes (272 B). All 4 shaders updated in lockstep (per `feedback_shader_struct_sync.md`).
- **#525 (FNV-ANIM-2)** — every `FloatTarget` arm now routes to a sparse sink. Renderer-adjacent only via animated UV/visibility/alpha sourcing.

### What's confirmed closed (from prior audits)

- `R1-N2` / `#777` — runtime invariant test pinning `texture_index` + `avg_albedo` retentions on `GpuInstance`.
- `R1-N3` / `#778` — stale `inst.<field>` comments removed from `triangle.frag`.
- `LIGHT-N1` / `#782` — `weather_system` gates fog/ambient/directional on `!cell_lit.is_interior`.
- `LIGHT-N2` / `#784` — composite fog mix moved post-ACES (display space).
- `M-NORMALS` / `#783` — authored tangent decode + Vertex slot + Path 1/Path 2 fragment branch (then *disabled by default* — see `R-N2` below).

### Prior-audit backlog (still open)

Verbatim from 2026-05-01 report — no movement on any of these since:

| Prior ID | Site | Status (re-checked 05-03) |
|---|---|---|
| `SY-2` / `RP-N1` (#573) | `helpers.rs:163` | Still open — `BOTTOM_OF_PIPE` term unchanged. |
| `SY-3` (#573) | `composite.rs:408` | Still open. |
| `CMD-3` (#573) | `screenshot.rs:164` | Still open. |
| `LIFE-N2` (#655) | `swapchain.rs:202` | Still open — `&self` destroy. |
| `DEN-9` (#677) | `svgf.rs:792-854` | Still open — recreate_on_resize barrier missing. |
| `SH-5` (#650) | `svgf_temporal.comp` | Still open — disocclusion gating by mesh-id only. |
| `MEM-2-7` (#682) | `acceleration.rs` | Still open — TLAS scratch buffer never shrinks. |
| `MEM-2-8` (#683) | `scene_buffer.rs:589` | Still open — ray_budget BAR waste. |
| `AS-8-6` (#678) | `acceleration.rs` | Still open — `!in_tlas` miscounted as missing BLAS. |
| `DEN-2` (#673) | `ssao.rs` | Still open — every-frame UNDEFINED→GENERAL discards init clear. |
| `DEN-5` (#675) | `svgf` early-out paths | Still open — `histAge=1.0` on first frame. |
| `RT-8` (#671) | `triangle.frag` GI miss | Still open — hardcoded sky color. |
| `SY-4` (#661) | skin compute → BLAS refit | Still open — legacy `ACCELERATION_STRUCTURE_READ_KHR`. |
| `RT-11` / `RT-12` | `triangle.frag:1543, 1581` | Still open — reservoir shadow ray missing `N_view` flip + tMin asymmetry. |
| `RT-14` | `triangle.frag:1635, 1610` | Still open — GI ray tMax / fade-window mismatch. |
| `PS-6` / `PS-7` / `PS-8` / `PS-9` | `pipeline.rs`, `helpers.rs:419` | Still open — static-vs-dynamic depth state drift; cwd-relative pipeline_cache.bin. |
| `DEN-11` / `DEN-12` | `composite.frag:220, 208` | Still open. |
| `SH-13` | `composite.frag:113-150` | Still open — cloud UV mip-LOD oscillation. |

---

## RT Pipeline Assessment

**No new RT findings.** The 36-commit window touched (a) tangent decode + Vertex layout (compute-side skinning stride confirmed in lockstep at `skin_compute.rs:33` and `skin_vertices.comp:36`), (b) light-attenuation curve in the cluster loop (no AS / TLAS / ray-query state touched), (c) fog mix in composite (no AS state touched). `acceleration.rs` is byte-identical to 05-01.

The ray-query backlog from 05-01 (`RT-11`/`RT-12`/`RT-14`/`SY-4`/`AS-8-6`) all remain open with the prior diagnosis still load-bearing.

---

## Rasterization Assessment

**`#renderlayer` ladder is correct.** Per-layer depth bias is selected via `vkCmdSetDepthBias` and only emits on state transitions (`draw.rs:1346` — `if last_render_layer != Some(batch.render_layer)`). The first batch always emits an explicit set (the `Option<RenderLayer>` sentinel forces it) and the initial pre-loop `cmd_set_depth_bias(cmd, 0.0, 0.0, 0.0)` at `draw.rs:1261` covers the Vulkan requirement that dynamic state be set before any draw when the pipeline declares `VK_DYNAMIC_STATE_DEPTH_BIAS`. All world pipelines (`triangle`, `triangle_two_sided`, blend variants) declare the dynamic state and have `depth_bias_enable(true)` at pipeline creation; the UI pipeline declares `depth_bias_enable(false)` and excludes `DEPTH_BIAS` from its dynamic-state list — Vulkan validation would reject a `cmd_set_depth_bias` call against the UI pipeline, and the UI overlay path correctly avoids it.

The 2-bit `RenderLayer` discriminant packs into `GpuInstance.flags` bits 4..5 with no collision against existing bits 0–3 (NON_UNIFORM_SCALE / ALPHA_BLEND / CAUSTIC_SOURCE / TERRAIN_SPLAT) or the terrain tile field at bits 16–31. Collision-free.

**Indirect-draw grouping** (`draw.rs:1296`) correctly keys on `(pipeline_key, render_layer)` so consecutive batches sharing the layer collapse into a single `cmd_draw_indexed_indirect`. Two-sided alpha-blend batches break out of grouping (back/front split — comment at `draw.rs:1459`).

**Smooth light falloff** (`triangle.frag:1701-1740`): `(1 - r⁴)² / (1 + 0.01·d)` is correctly `c¹`-continuous at the cull radius; the prior `1 - r²` curve dropped to ~0.28 at 85% of effective range. Mid-zone energy roughly doubles vs. the prior curve at 0.85 ratio (0.46 vs. 0.28). Algebraically clean. Note: the "Frostbite" label conflates the *window* shape with the published falloff (the paper formula uses `1 / (d² + ε)` not `1 / (1 + 0.01·d)`); the engine keeps Gamebryo's authored 1/d shape and only borrows the window — flagged in `R-N4` below as documentation, not a bug.

**Composite fog mix in display space** (`composite.frag:289-336`): `combined → ACES → tonemapped` then `tonemapped = mix(tonemapped, aces(fog_color * exposure), fogFactor)`. Both `mix()` arguments are post-ACES values in `[0,1]`, so the mix is in display space as documented. The `direct4.a` alpha pass-through preserves the alpha-blend marker bit symmetric across the sky branch and the geometry branch (DEN-11 was about the sky branch alpha; this preserves it correctly).

---

## Findings

### CRITICAL

#### R-N1 — Regression of #776: `ui.vert` reads `materials[0].textureIndex` again

- **Severity**: CRITICAL
- **Dimension**: Material Table (R1)
- **Locations**:
  - `crates/renderer/shaders/ui.vert:65-77` — current state declares `MaterialBuffer` SSBO at `set=1, binding=13` and reads `materials[inst.materialId].textureIndex` again
  - `crates/renderer/src/vulkan/context/draw.rs:984-994` — UI instance still pushed with `..GpuInstance::default()` (zero `material_id`)
  - `crates/renderer/src/vulkan/scene_buffer.rs:172-176` — Rust-side docstring continues to anticipate this exact failure mode
- **Status**: **Regression of #776** (closed by `9c7ea0d` 2026-05-01 18:56:34, regressed by `c248a99` 2026-05-01 21:51:32 — same day, ~3 hours later)
- **Description**: Commit `9c7ea0d` correctly removed the `GpuMaterial` struct + `MaterialBuffer` SSBO declaration from `ui.vert` and switched the read back to `inst.textureIndex`. The next commit, `c248a99` ("Fix #782: gate weather_system fog/ambient/directional writes"), shows a **+24 line / -7 line diff against `ui.vert`** that re-adds the `GpuMaterial` declaration, re-binds the `MaterialBuffer` SSBO, and reverts `fragTexIndex = inst.textureIndex;` back to `fragTexIndex = materials[inst.materialId].textureIndex;`. The diff appears to be a stale hunk picked up alongside the systems.rs / regression-test changes that #782 was actually about — the commit message and body don't mention `ui.vert` at all.
- **Evidence**:
  - `git diff 9c7ea0d c248a99 -- crates/renderer/shaders/ui.vert` shows the +24/-7 hunk that undoes #776 (verified: the index hash is `1f9b263..8a743cc`, the `8a743cc` blob is identical byte-for-byte to the *pre-#776* state)
  - `git show HEAD:crates/renderer/shaders/ui.vert` confirms the current `main` state is the regressed one
  - The `..GpuInstance::default()` push at `draw.rs:987` leaves `material_id = 0`. The `MaterialBuffer` is keyed by per-frame intern order — `materials[0]` is the first scene material interned in `build_render_data`, not a UI-specific entry.
  - `triangle.vert:170` (`fragTexIndex = inst.textureIndex;`) is unaffected; only the UI overlay path is broken.
- **Impact**: UI overlay (Ruffle / Scaleform output) samples an arbitrary scene texture — visually whatever the first interior surface drew. Same severity, same blast-radius, same fix-shape as `R1-N1` from 2026-05-01.
- **Suggested Fix**: One-line revert of the `c248a99` `ui.vert` hunk. Restore the `9c7ea0d` state — drop the `MaterialBuffer` SSBO + `GpuMaterial` struct from `ui.vert`, switch the read back to `inst.textureIndex`. Add a unit-test-equivalent grep guard (similar to `#777`'s build-time grep test for `GpuInstance.texture_index`) so the bytes don't drift again. Worth reviewing whether the `c248a99` regression test — which only covers `weather_system` — should also include a build-time grep on `ui.vert` so a stray hunk is caught at PR review.
- **Related**: `#776` (closed, regressed); `R1-N1` from 2026-05-01 audit (root finding); `feedback_shader_struct_sync.md` (the broader contract this violates).

### HIGH

#### R-N2 — `perturbNormal` disabled by default; default render path forfeits all bump detail

- **Severity**: HIGH
- **Dimension**: Shader Correctness
- **Locations**:
  - `crates/renderer/shaders/triangle.frag:853-858` — gate condition requires `DBG_FORCE_NORMAL_MAP (0x20)` flag set in `BYROREDUX_RENDER_DEBUG`
  - `crates/renderer/shaders/triangle.frag:646-672` — bit catalog: `DBG_BYPASS_NORMAL_MAP = 0x10` (legacy bypass; now redundant since perturbation is off in the default path) + `DBG_FORCE_NORMAL_MAP = 0x20` (the new opt-in)
  - Commit `77aa2de` — workaround landing the gate
- **Status**: NEW
- **Description**: Commit `77aa2de` ("Workaround: disable perturbNormal by default — chrome regression on FNV walls/wood") inverts the perturbNormal call from on-by-default to off-by-default. The fix lands as a `BYROREDUX_RENDER_DEBUG=0x20` opt-in. The full M-NORMALS infrastructure (authored tangent decode at `extract_tangents_from_extra_data`, nifly `synthesize_tangents` fallback, Vertex 84→100 B layout, vertex shader tangent transform, fragment shader Path 1/Path 2 branches, skin-compute 21→25 stride) all stay in place — only the per-fragment call is gated off until the chrome-on-walls reappearance is properly traced.
- **Evidence**:
  ```glsl
  // triangle.frag:853-858 — current default-disabled gate
  if (normalMapIdx != 0u
      && (dbgFlags & DBG_FORCE_NORMAL_MAP) != 0u
      && (dbgFlags & DBG_BYPASS_NORMAL_MAP) == 0u)
  {
      N = perturbNormal(N, fragWorldPos, sampleUV, normalMapIdx, fragTangent);
  }
  ```
  Without `BYROREDUX_RENDER_DEBUG=0x20`, the geometric vertex normal `N = normalize(fragNormal)` is the only normal that reaches the lighting equations.
- **Impact**: Surface bump detail is forfeit on every BC5-normal-mapped surface in the default render path. Macro lighting (RT shadows, GI, direct lighting) still works; surfaces just look flatter than authored. The 05-02 commit `91e9011` was supposed to *resolve* the chrome-walls regression by replacing screen-space-derivative TBN with authored per-vertex tangents; the followup `77aa2de` confirms the regression returned even with authored tangents wired through, suggesting either Path 1's TBN handedness is wrong (sign flip, axis swap) or the synthesized-tangent fallback at `synthesize_tangents` (nifly port) has a `tan_u`/`tan_v` swap. The commit body explicitly cites `feedback_speculative_vulkan_fixes.md` as the rationale for shipping the gate-off workaround instead of guessing.
- **Trigger Conditions**: Every frame on every world surface with a non-zero `normal_map_index`. Net-zero perf cost (the gate elides the texture sample + TBN math); pure visual-quality regression.
- **Suggested Fix**: Track this as a follow-up issue (no GH issue exists yet — the commit body says "follow-up tracking issue" but didn't open one). Diagnosis path:
  1. Capture `BYROREDUX_RENDER_DEBUG=0x28` (tangent viz + force-on) at the chrome-affected camera angle to determine which path fires (green = Path 1 authored, red = Path 2 derivative).
  2. If Path 1 fires green and chrome is visible → tangent handedness or bitangent sign is wrong. Likely fix: flip `vertexTangent.w * cross(N, T)` → `-vertexTangent.w * cross(N, T)`, OR swap the axis convention in `extract_tangents_from_extra_data` (Z-up tangent decode currently negates X axis only — verify against nifly's authoritative axis swap).
  3. If Path 2 fires red on chrome → screen-space derivative `T = dPdx * dUVdy.y - dPdy * dUVdx.y` has the wrong sign convention against Bethesda's UV winding. Likely fix: swap dPdx/dPdy or dUVdx/dUVdy operand order.
- **Related**: `#783` (closed); `feedback_speculative_vulkan_fixes.md`; `feedback_chrome_means_missing_textures.md` (independent root cause that was the previous chrome culprit).

### MEDIUM

#### R-N3 — Tangent transformed via inverse-transpose under non-uniform scale produces cotangent direction

- **Severity**: MEDIUM (dormant — visual surface only when `R-N2` is resolved + perturbNormal re-enabled)
- **Dimension**: Shader Correctness
- **Location**: `crates/renderer/shaders/triangle.vert:181-193`
- **Status**: NEW
- **Description**: When `inst.flags & INSTANCE_FLAG_NON_UNIFORM_SCALE` is set, the tangent is transformed via `transpose(inverse(m3)) * inTangent.xyz`. That expression yields the *cotangent* (covariant transform), which is what a normal needs but is **not** how a tangent transforms. A tangent is contravariant — the correct transform is `m3 * inTangent.xyz`. Under non-uniform scale, the two diverge.

  Worked example (M = diag(2, 1, 1), T_local = (1, 0, 0)):
  - Correct contravariant: `m3 * T = (2, 0, 0)` — direction along stretched-X
  - Currently-coded covariant: `(M⁻¹)ᵀ * T = (0.5, 0, 0)` — same direction here (degenerate axis-aligned case)

  But for M = diag(2, 1, 0.5) and T_local = (1, 1, 0) / √2:
  - Correct: `m3 * T = (√2, 1/√2, 0)`, normalised → tangent rotated toward X
  - Currently-coded: `(M⁻¹)ᵀ * T = (1/(2√2), 1/√2, 0)`, normalised → tangent rotated toward Y

  Direction of the per-fragment T is therefore wrong on any non-uniformly-scaled mesh; the bitangent is then `sign × cross(N, T)` so an erroneous T propagates to B; the normal-map perturbation rotates the bump highlight by the same error.
- **Evidence**:
  ```glsl
  // triangle.vert:183-188 — currently codes covariant transform
  } else if ((inst.flags & 1u) != 0u) {
      float det = determinant(m3);
      t_world = (abs(det) > 1e-6)
          ? transpose(inverse(m3)) * inTangent.xyz
          : inTangent.xyz;
      t_world = normalize(t_world);
  }
  ```
  Per `triangle.frag:597`, the fragment shader does Gram-Schmidt against N (`T = normalize(T - dot(T, N) * N)`), which projects T onto the surface plane — but it cannot recover the correct surface u-axis direction once the input direction is wrong.
- **Impact**: Currently zero, because perturbNormal is disabled by default (`R-N2`). When `R-N2` is resolved and perturbation re-enabled, expect rotated normal-map detail on any mesh placed with non-uniform scale (e.g. NIF nodes whose authoring squashed/stretched a sub-mesh; XSCL on REFRs is uniform, so this is mostly node-internal).
- **Trigger Conditions**: `(inst.flags & 1) != 0` (non-uniform scale flag set) AND `dot(inTangent.xyz, inTangent.xyz) >= 1e-6` (authored tangent present) AND `R-N2` resolved (perturbNormal re-enabled).
- **Suggested Fix**: Replace the inverse-transpose path with `m3 * inTangent.xyz` then `normalize`:
  ```glsl
  } else if ((inst.flags & 1u) != 0u) {
      // Tangents are contravariant: M * T (not M⁻ᵀ * T which is for normals).
      // Magnitude is irrelevant — the fragment shader normalizes again.
      t_world = m3 * inTangent.xyz;
      float t_len2 = dot(t_world, t_world);
      t_world = (t_len2 > 0.0) ? t_world * inversesqrt(t_len2) : vec3(0.0);
  }
  ```
  Bundle with `R-N2` re-enablement (no point shipping the fix while perturbNormal is gated off).
- **Related**: `R-N2`; `#783`; uniform-scale path at `triangle.vert:189-193` is already correct (`t_world = m3 * inTangent.xyz`).

### LOW

#### R-N4 — Vertex shader unconditionally computes tangent transform despite per-fragment perturbation being off by default

- **Severity**: LOW
- **Dimension**: Shader Correctness × Performance
- **Location**: `crates/renderer/shaders/triangle.vert:173-194`
- **Status**: NEW (ride-along observation alongside `R-N2`)
- **Description**: With `R-N2` shipping perturbNormal off by default, the `fragTangent` varying is unused on every fragment in the default render path. The vertex shader still:
  - Reads `inTangent` (location 8) — already in the vertex stage's input bandwidth, sunk cost
  - Branches on `dot(inTangent.xyz, inTangent.xyz) < 1e-6`
  - On non-zero, runs the inverse-transpose-or-mat3 transform + normalize
  - Writes the `vec4` varying — interpolated through the rasteriser to every fragment
  These ops are cheap individually but multiply by every vertex × every frame. At 1578 entities × ~500 verts avg the work is ~800k ops/frame purely to compute a varying that's never read.
- **Evidence**: `R-N2` gates the consumer to `(dbgFlags & DBG_FORCE_NORMAL_MAP) != 0u`; the producer at `triangle.vert:181-194` has no symmetric gate.
- **Impact**: Negligible perf today (well under any frame budget at the target hardware tier). Worth fixing once `R-N2` is resolved one way or the other — either re-enable perturbation (then the producer is needed) or pull the tangent decode to a separate compile of the shader.
- **Suggested Fix**: Defer until `R-N2` resolves. If `R-N2` re-enables perturbation, do nothing. If perturbation stays off long-term, gate the tangent transform on a `sceneFlags` bit so the vertex stage skips the work.
- **Related**: `R-N2`.

---

## Prioritized Fix Order

1. **`R-N1` (CRITICAL)** — one-line revert. Unblocks the UI overlay. Should land alongside a build-time grep guard against `materials[inst.materialId]` in `ui.vert` so this doesn't regress a third time.
2. **`R-N2` (HIGH) diagnostic** — capture `BYROREDUX_RENDER_DEBUG=0x28` on the chrome-affected camera. Open a follow-up GH issue to track. Without RenderDoc-grade visibility into which path fires on the chrome fragments, shipping a speculative TBN-flip fix violates `feedback_speculative_vulkan_fixes.md`.
3. **`R-N3` (MEDIUM)** — bundle with `R-N2` re-enablement. Trivial three-line shader edit; tests live in the surrounding tangent transform path.
4. **`R-N4` (LOW)** — defer until `R-N2` resolves.

The 19 prior-audit open items in the table above remain prioritised per the 2026-05-01 ordering. None blocks the user's currently-targeted Oblivion-class interior fidelity milestone — that bar is reached today (commit `18bbeae`'s body confirms it on FNV `GSDocMitchellHouse`) once `R-N1` is reverted.

---

*Generated by `/audit-renderer` on 2026-05-03. To file findings as GitHub issues: `/audit-publish docs/audits/AUDIT_RENDERER_2026-05-03.md`.*
