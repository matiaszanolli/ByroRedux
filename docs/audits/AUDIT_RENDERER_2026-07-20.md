# Renderer Audit — 2026-07-20

**Scope**: Full 22-dimension sweep (`/audit-renderer --focus deep`), 6 agent groups (max 3 concurrent), each cross-checked against `/tmp/audit/renderer/issues.json` (54 open issues) and the most recent full sweep (`AUDIT_RENDERER_2026-07-16.md`, 4 days prior) plus relevant narrow follow-ups. Dimension 22 (Light Animation) is brand new this pass — added to the skill today to cover a same-day commit, no prior baseline.

**Trigger**: Two commits landed earlier today — `883f57cd` ("stable surface ID for temporal shadowing and caustics") and `41eedfe1` ("light animation + material properties refactor"). Every group weighted its coverage toward these commits' blast radius; older, previously-audited paths got lighter regression-guard-level re-checks where the commits didn't touch them.

**Process note**: Three of six agent groups (Dim 1-3, Dim 4-7, Dim 8-10) stalled mid-investigation on their first pass, ending on a bare tool-result with no final report — the same failure mode the 2026-07-16 audit hit. All three were resumed in place with a "stop investigating, write up now" directive and completed normally; findings below are from the completed second pass.

## Executive Summary

| Severity | Count | Findings |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 1 | REN-D14-01 (caustic pass mis-indexes instance SSBO for opaque pixels) |
| MEDIUM | 3 | REN-RESTIR-01 (reservoir surface-tag truncation), D22-1 (light-anim flag mis-assignment, confirmed against external reference), REN-D15-01 (existing, water noise precision) |
| LOW | 6 | REN-D9-01, REN-D15-02 (both existing), REN-D17-01, D22-2, D22-3 (new), plus doc-rot on `shader-pipeline.md` (existing, tracked) |
| Resolved since last sweep | 1 | REN-D18-01 (procedural-fallback clock reset — confirmed fixed) |

**Net take**: today's two commits are mostly clean — the surface-ID refactor is correctly wired through GPU-struct layout, TAA/SVGF disocclusion, ReSTIR-DI, mesh-ID encoding, and command recording, and the material-properties refactor's glass-behavior application is provably boundary-clean and texture-preserving. But the surface-ID switch broke one downstream consumer that inherited the old instance-index assumption — the caustic compute pass — and a second (ReSTIR reservoir tag) narrowed silently. Separately, the new Dimension 22 sweep found the light-animation refactor fixed a real symptom (FO4 shadow-spotlights flickering) by masking around a mis-valued constant rather than fixing it, so the same underlying bug still fires on every other supported game.

## RT Pipeline Assessment

- **AS/SSBO contract intact.** `instance_custom_index` still resolves to the compacted per-frame SSBO position (`ssbo_idx`), untouched by the surface-ID commit — confirmed by direct read of `AccelerationManager::build_tlas`. BLAS geometry format, build-flag constants, and deferred-destroy guards all re-confirmed holding.
- **Ray-query/shader correctness holds** for the fresh thin-glass gate (`MAT_FLAG_THIN_GLASS`, bit 11) and the 5-site `GpuInstance` shader mirror lockstep (all carry `surfaceId`).
- **ReSTIR-DI**: the spatial-reuse geometric-normal-cone gate is unchanged and correct; the surface-identity tag it now uses (`inst.surfaceId & RESERVOIR_SURFACE_MASK`) is where the one MEDIUM finding below lives.
- **Caustic pass is the one real regression** — see REN-D14-01. It sits outside the RT/AS pipeline proper (a screen-space compute pass reading the G-buffer), so it does not implicate TLAS/BLAS correctness, but it does share the surface-ID root cause with the ReSTIR finding.

## GPU-Struct & Memory Assessment

- `GpuInstance` stays 112 B; the former `_pad_albedo` lane is now `surface_id: u32` at offset 108, pinned by `gpu_instance_field_offsets_match_shader_contract` and two new tests (`restir_history_uses_stable_surface_id_not_instance_order`, `gbuffer_history_uses_stable_surface_id_but_caustics_keep_draw_lookup`).
- `GpuMaterial` stays 300 B; `MAT_FLAG_THIN_GLASS` (bit 11) is a new flag bit only, no struct growth. `Material.ior` (new ECS field) is a plain scalar, no allocation, correctly wired into `material_hash` for dedup (glass 1.45 vs. generic dielectric 1.5 now stay distinct table entries).
- Sync/lifecycle dimensions (4, 5) got zero new findings — neither commit touched barrier or lifecycle code, and the 2026-07-16 sweep already cleared those paths.
- One pre-existing, already-tracked doc-rot item: `docs/engine/shader-pipeline.md`'s `GpuInstance` table still lists offset 108 as padding and its material-flags table stops before bit 11 — same class as **Existing: #1915, #1918**, not re-filed.

## Findings

### HIGH

#### REN-D14-01: `caustic_splat.comp` mis-indexes the instance SSBO for opaque pixels after the stable-surface-ID switch
- **Severity**: HIGH (escalates toward CRITICAL for entity IDs ≥ `MAX_INSTANCES` — a genuine OOB SSBO read with `robustBufferAccess` disabled)
- **Dimension**: Caustics / SSBO-Indexing
- **Location**: `crates/renderer/shaders/caustic_splat.comp` (the `meshId = meshIdRaw & 0x7FFFFFFFu; if (meshId == 0u) return; ... instIdx = meshId - 1u; instances[instIdx]` block). Upstream: `crates/renderer/shaders/triangle.frag` (mesh-ID encode). CPU source: `crates/renderer/src/vulkan/context/draw.rs` (`surface_id: draw_cmd.entity_id.wrapping_add(1)`).
- **Status**: NEW — independently confirmed by two separate audit groups (Dim 8-10 and Dim 11-14), reading the shader from different angles.
- **Description**: The shader masks off bit 31 and rejects only `meshId == 0`, then unconditionally derives `instIdx = meshId - 1` and reads `instances[instIdx]`. Before today's commit, opaque G-buffer pixels packed `instance_index + 1` in those bits — a valid live per-frame slot, safely rejected downstream by the `INSTANCE_FLAG_CAUSTIC_SOURCE` gate (caustic sources are always alpha-blend). After the commit, opaque pixels instead carry `stableSurfaceId = inst.surfaceId & 0x7FFFFFFF` (`surface_id = entity_id + 1`) — an ECS identity unrelated to the per-frame draw-order index `instances[]` is keyed by. No `(meshIdRaw & 0x80000000u) == 0u` opaque-reject guard was added to compensate; the shader's own in-source comment still states the old (now false) safety premise.
- **Evidence**: Confirmed via direct read by both groups; `draw.rs`'s `surface_id` assignment; `world.rs::spawn()`'s monotonic, never-recycled `next_entity` counter (so `entity_id` is unbounded across a session, unrelated to `MAX_INSTANCES`); `device.rs` does not enable `robust_buffer_access`.
- **Impact**: (1) Always-on visual corruption in any scene with a caustic source (glass/water — common): opaque pixels read an arbitrary current-frame `instances[entity_id]` slot, splatting spurious caustics onto opaque surfaces whenever the aliased slot happens to have the caustic-source flag set. (2) Conditional OOB read: once cumulative session spawns exceed `MAX_INSTANCES` (262,144) — plausible in a long exterior-streaming session — `instances[entity_id]` reads past the fixed SSBO allocation with no `robustBufferAccess`, which is undefined behavior up to device-lost.
- **Related**: `883f57cd`; shares its unbounded-`surface_id` root cause with REN-RESTIR-01 below; `gbuffer_history_uses_stable_surface_id_but_caustics_keep_draw_lookup` is a CPU-side layout test and gives false coverage confidence here — it cannot exercise this shader's runtime path.
- **Suggested Fix**: Add `if ((meshIdRaw & 0x80000000u) == 0u) return;` immediately before deriving `instIdx` in `caustic_splat.comp` — caustic sources are always alpha-blend, so rejecting opaque pixels before the SSBO read restores the pre-commit invariant and is strictly cheaper (skips the read for the opaque majority). Refresh the now-stale in-source comment.

### MEDIUM

#### REN-RESTIR-01: ReSTIR-DI reservoir surface tag truncates the now-unbounded `surface_id` to 22 bits
- **Severity**: MEDIUM (one reviewing group independently assessed this as LOW — see note below; kept at MEDIUM here as the more cautious of the two independent takes, given the shared root cause with the HIGH finding above)
- **Dimension**: Ray Queries / Denoiser (ReSTIR-DI surface identity)
- **Location**: `crates/renderer/shaders/triangle.frag` — `RESERVOIR_SURFACE_MASK = 0x3FFFFFu` (22 bits), the `uint surfaceId = inst.surfaceId & RESERVOIR_SURFACE_MASK;` reuse site.
- **Status**: NEW — reported independently by two groups (as REN-D2-SURFACEID-01 and REN-D8-02), same root cause.
- **Description**: The reservoir's surface-identity tag was switched from `fragInstanceIndex + 1` (bounded by `MAX_INSTANCES = 0x40000`, comfortably under the 22-bit field) to `inst.surfaceId & RESERVOIR_SURFACE_MASK`, where `surface_id = entity_id + 1` is unbounded across a session (entity IDs are never recycled). Past ~4.19M cumulative spawns, two distinct live surfaces can alias onto the same 22-bit tag, letting the spatial-reuse pass mis-accept a neighbour reservoir belonging to a different surface. The adjacent in-source comment justifying the field width ("comfortably above MAX_INSTANCES") is now factually stale. Separately, the mesh-ID/TAA-SVGF path uses the full 31 bits for the same `surface_id`, so above 2^22 spawns the reservoir and mesh-ID paths would disagree on surface identity for the same fragment.
- **Impact**: Visual-only (direct-shadow bleed across aliased coplanar surfaces at the aliasing threshold), no crash/corruption. One reviewing group characterized this as realistically inert — self-correcting via the per-sample final visibility ray, and requiring a multi-hour session to reach the 2^22-spawn regime — which is why it's flagged MEDIUM rather than HIGH despite sharing a root cause with REN-D14-01.
- **Related**: REN-D14-01 (identical unbounded-`surface_id` root cause, different consumer); `883f57cd`.
- **Suggested Fix**: Update the stale comment to state the tag now holds `entity_id + 1` (unbounded, aliases every 2^22 spawns, self-correcting). Optionally hash `surface_id` into 22 bits rather than truncating, or widen the reservoir's surface field if bits can be spared from the light index.

#### D22-1: `LIGHT_FLAG_PULSE_SLOW = 0x400` is mis-assigned to the Shadow-Spotlight bit; the FO4 special-case masks the symptom instead of fixing the root cause
- **Severity**: MEDIUM (visual-only per the Light Animation severity floor)
- **Dimension**: Light Animation (new Dimension 22)
- **Location**: `crates/core/src/ecs/components/light.rs` (`LIGHT_FLAG_PULSE_SLOW`), `byroredux/src/systems/light_anim.rs` (`canonical_light_animation_flags`)
- **Status**: NEW, CONFIRMED — verified during this audit against the `fopdoc/Fallout3/Records/LIGH.md` reference (fetched live): `Pulse Slow = 0x100`, `Spot Shadow = 0x400`. The codebase's `LIGHT_FLAG_PULSE_SLOW = 0x400` is the wrong bit; this is not a per-game divergence as the in-source comment claims, but a flat mis-assignment.
- **Description**: `canonical_light_animation_flags` special-cases FO4 to mask out `0x400` (documented in-code as "FO4's Shadow Spotlight bit"), implying other games genuinely use `0x400` for Pulse Slow. External verification shows `0x400` is Spot Shadow across the FO3-lineage layout the codebase itself cites as authoritative for "the relevant prefix." Consequences: (1) non-FO4 games (Skyrim/FO3/FNV/Oblivion/FO76/Starfield) still decode any Shadow-Spotlight light as Pulse Slow and slow-pulse the whole-scene light — only FO4 was special-cased; (2) genuine Pulse-Slow lights (authored with the real `0x100` bit) never animate in any game, since no constant matches `0x100`; (3) the FO4 mask (`FLICKER | PULSE` only) also strips the legitimate `0x40` Flicker-Slow bit, so FO4 dying-fire/low-oil lights get zero animation.
- **Impact**: Visual-only, no crash/corruption. Affects the animated-light slice across every supported game except FO4 (which trades this bug for a different gap).
- **Related**: The new regression tests (`fallout4_shadow_spotlight_is_not_slow_pulse`, `fallout4_real_flicker_and_pulse_map_to_shared_behavior`) correctly test the masking *logic* but inherit and lock in the wrong `0x400` premise.
- **Suggested Fix**: Set `LIGHT_FLAG_PULSE_SLOW = 0x0000_0100`. Once corrected, `0x400` no longer collides with any animation bit, so the FO4 special-case can likely be dropped entirely (all games mask to `SHARED_LIGHT_ANIMATION_MASK`), which also restores FO4 Flicker-Slow animation. Update the `light.rs`/`light_anim.rs` docstrings asserting the false Skyrim-vs-FO4 divergence. Recommend one more cross-check against Skyrim's specific LIGH layout before shipping — the UESP Skyrim LIGH page returned HTTP 403 during this audit and could not be independently checked, only the FO3-lineage fopdoc reference.

#### REN-D15-01: Procedural water-noise precision guard is comment-only (existing)
- **Severity**: MEDIUM
- **Location**: `crates/renderer/shaders/water.frag` (`sampleScrollingNormal`), `crates/core/src/ecs/components/water.rs`, `byroredux/src/env_translate.rs`
- **Status**: Existing (`AUDIT_RENDERER_2026-07-15_DIM15.md` REN-D15-01) — confirmed still holds, unaffected by today's commits
- **Description**: The procedural wave-normal branch still feeds absolute world-XZ into the hash function, and remains the default (no Skyrim-style XCWT) for FNV/FO3/Oblivion water. The `#1502` precision comment isn't wired to an actual render-origin rebase.
- **Suggested Fix**: Rebase the hash input to `vWorldPos.xz - renderOrigin.xz`; update the stale comment.

### LOW

- **REN-D9-01** (existing, confirmed still open): `SkinSlotPool` doc comment arithmetic error ("1366 → 1365 allocatable" should read "1365, minus reserved slot 0 → 1364 allocatable"). `crates/core/src/ecs/resources/skin_slot_pool.rs`.
- **REN-D15-02** (existing, confirmed still open): `RenderLayer::Decal` comment claims a depth-bias the water pipeline doesn't apply (`depth_bias_enable(false)`, no `DEPTH_BIAS` dynamic state). `byroredux/src/cell_loader/water.rs` / `crates/renderer/src/vulkan/water.rs`.
- **REN-D17-01** (new): glass-branch F0 comment in `triangle.frag` still says "mat.ior defaults to 1.5 (glass)" — stale after `41eedfe1` moved canonical glass IOR to 1.45 (`GLASS_SURFACE_BEHAVIOR`), leaving 1.5 as the generic-dielectric default only. No runtime impact; risk is a future edit re-hardcoding 1.5 for glass.
- **D22-2** (new): `fxlight`/`fxlightrays`/`fxfog` LIGH placements (`byroredux/src/cell_loader/references/mod.rs` effect-mesh branch) never get a `LightFlicker` attached — a completeness gap (steady light instead of authored flicker), not a correctness regression.
- **D22-3** (new, tech-debt): `byroredux/src/cell_loader/spawn.rs`'s meshed-placement path re-implements `attach_light_flicker_if_needed`'s body instead of calling it — a hand-sync drift hazard, not a live bug today.
- **Doc-rot** (existing, already tracked): `docs/engine/shader-pipeline.md`'s `GpuInstance`/`material_flags` tables lag the `surface_id`/`MAT_FLAG_THIN_GLASS` additions — same class as **Existing: #1915, #1918**.

### Resolved since 2026-07-16

- **REN-D18-01**: the procedural-fallback `GameTimeRes` insert in `byroredux/src/scene/world_setup.rs` is now guarded against clobbering an in-progress session clock, matching the `CloudSimState`/WTHR branches. Confirmed fixed.
- **V-DIM16-01** (from the 2026-07-14 pass): the stale `sun_dir` doc comment in `crates/renderer/src/vulkan/volumetrics.rs` has been corrected to state the "direction TO the sun" convention. Confirmed fixed.

## Prioritized Fix Order

1. **REN-D14-01** (HIGH) — one-line fix (`caustic_splat.comp` opaque-reject guard), release-relevant visual corruption + conditional OOB.
2. **D22-1** (MEDIUM) — one-constant fix (`LIGHT_FLAG_PULSE_SLOW`), but recommend the Skyrim-specific cross-check called out above before changing, per this repo's no-guessing policy.
3. **REN-RESTIR-01** (MEDIUM) — comment fix is trivial and safe immediately; the optional hashing/widening change is lower urgency given the self-correcting, very-long-session-only failure mode.
4. **REN-D15-01** (MEDIUM, existing) — render-origin rebase, unaddressed across three prior audits.
5. **LOW items** — doc corrections and the two Dim-22 completeness/tech-debt items, batch whenever convenient.

## Needs-RenderDoc

None newly raised this pass. Prior carried-forward items (caustic atomic-add → SHADER_READ ordering, G-buffer → compute-consumer transitions in `draw.rs`) remain statically-plausible-but-GPU-unverifiable per the 2026-07-16 sweep; no barrier/sync code was touched by today's two commits, so nothing new to flag here.

## Appendix: Still-Open Skill-Doc Suggestions (carried forward from 2026-07-16, not code bugs)

Not re-verified independently this pass beyond what's noted inline above; restating for visibility since they remain unaddressed in `.claude/commands/audit-renderer/SKILL.md`:
1. Dimension 20's checklist phrase "`cmd_reset_query_pool` before re-recording brackets" — implementation correctly uses host-side `device.reset_query_pool`; reword to "reset (host-side or command-buffer)".
2. Dimension 1's checklist phrase "`TRIANGLE_FACING_CULL_DISABLE` on all instances" — current (correct) behavior gates this per `draw_cmd.two_sided`; reword to reflect the conditional.
3. The historical "13-bit DBG_* catalog" wording is stale (grown further since); reword to reference the guard mechanism rather than a fixed count.
