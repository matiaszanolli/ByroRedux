# Issues 2245, 2246, 2251, 2255

## #2245 — REN-D19-01: perturbNormal's screen-space derivative fallback double-flips handedness on mirrored UVs

**Location**: `crates/renderer/shaders/include/material_sampling.glsl:134` (`perturbNormal`, Path 2 no-authored-tangent fallback, ~line 170-193)

Path 2 derives `T` from position/UV screen-space derivatives (`T = normalize(dPdx * dUVdy.y - dPdy * dUVdx.y)`) — the un-divided numerator of the standard tangent formula, whose sign already implicitly carries the UV-Jacobian determinant sign. It then separately computes `screenSign = sign(dUVdx.x * dUVdy.y - dUVdx.y * dUVdy.x)` and applies it AGAIN via `B = screenSign * cross(N, T)` — double-counting the mirrored-UV handedness correction. Same defect class as #1104 (REN-D16-002), already fixed for the authored-tangent path. Affects terrain and every renderer-synthetic-tangent mesh; critical for Starfield since `BSGeometry` tangents are empty until #1086 lands an extractor.

**Fix**: verify whether `T`'s own sign already encodes the determinant (no additional correction needed) and remove the redundant `screenSign` application if so, matching #1104's resolution for the authored-tangent path.

## #2246 — REN-D19-02: Starfield's packed bitangent sign isn't normalized to +/-1 like every other game's

**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:166` (bitangent-sign channel from `unpack_udec3_xyzw`)

Starfield's `BSGeometry` packs the bitangent sign via `unpack_udec3_xyzw`, but unlike every other game's import path, the result isn't normalized/clamped to exactly `+1.0`/`-1.0` before being written to `vertexTangent.w`. `material_sampling.glsl` defensively re-normalizes at point of use (`tangentSign = vertexTangent.w < 0.0 ? -1.0 : 1.0`), but any other consumer reading `vertexTangent.w` directly would disagree with the primary-ray tangent frame.

**Fix**: normalize the unpacked bitangent-sign value to exactly `+1.0`/`-1.0` at import time in `bs_geometry.rs`, matching every other game's import path.

## #2251 — REN-D22-02: canonical_light_animation_flags silently assumes Skyrim's LIGH layout for Fallout 76 and Starfield too

**Location**: `byroredux/src/systems/light_anim.rs:47-53` (`canonical_light_animation_flags`)

Only `GameKind::Fallout4` is special-cased; `Fallout76` and `Starfield` fall into the `_` catch-all and are assumed to share Skyrim's `SHARED_LIGHT_ANIMATION_MASK` layout, never individually verified. Pre-existing, predates Session 62. Directly analogous to the shadow-flags gap fixed in #2250 last session (same file, same `_` catch-all shape) — that fix's own test (`every_game_shares_the_same_shadow_mask_today`) explicitly documented this exact unverified-default pattern as acceptable pending evidence.

**Fix**: verify FO76/Starfield's actual LIGH flag layout against available format documentation/tooling; add explicit match arms if either diverges from Skyrim's assumed layout.

## #2255 — TD1-NEW-02: draw_frame's 07-25 extraction fix landed and holds, but the function re-grew around new shadow-policy/volumetrics dispatch code

**Location**: `crates/renderer/src/vulkan/context/draw.rs:872-3001` (`draw_frame`, ~2131 LOC)

The prior tech-debt fix (extracting `build_fsr_frame_parameters`) landed and holds, but `draw_frame` itself grew 2048→2131 LOC since the 07-25 audit, from the shadow-policy refactor (`1fb79038`) and volumetric/local-fog-volume integration adding inline dispatch/barrier code rather than extracted siblings. Purely maintainability, not correctness.

**Fix**: extract the shadow-policy/global-only-mesh BLAS gating setup and/or the volumetrics-UBO-write block into standalone functions (pure data assembly, no borrow-checker reason to stay inline), same pattern as `build_fsr_frame_parameters`. The issue also frames "filing this as a standing tracking issue" as part of the fix, which is satisfied by this issue existing.

## Domain classification
- #2245: renderer (byroredux-renderer) — GLSL shader, `material_sampling.glsl`
- #2246: nif (byroredux-nif) — `crates/nif/src/import/mesh/bs_geometry.rs`
- #2251: binary (byroredux) — `byroredux/src/systems/light_anim.rs`
- #2255: renderer (byroredux-renderer) — `crates/renderer/src/vulkan/context/draw.rs`, tech-debt/refactor only
