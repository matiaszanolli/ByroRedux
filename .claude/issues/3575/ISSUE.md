# #3575 — REN-2026-08-30-D17-02: the soft-shadow emitter disk is re-derived in the shader from the CULL radius (`position_radius.w`) instead of reading the canonical source radius the CPU already uploads in `params.y` — the two formulas agree only in the un...

**Labels**: `medium,renderer,shaders,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3575 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Soft Shadows
- **Location**: `crates/renderer/shaders/triangle.frag` (ReSTIR arm, line 3326; legacy-WRS arm, line 3479 — identical literal, duplicated). Canonical source: `crates/core/src/lighting.rs` (`Emitter::from_legacy_world_units`, lines 256-265; `LEGACY_LIGHT_CULL_RANGE_MULTIPLIER`, line 18), `byroredux/src/render/lights.rs` (`gpu_light_from_emitter`, lines 89-113)
- **Status**: NEW. `issues.json` keyword sweep for `penumbra|soft shadow|source radius|shadow disk` returns nothing; `grep -n "source_radius\|lightDiskRadius" docs/audits/AUDIT_RENDERER_2026-08-27.md` → no hits.
- **Description**: Both shadow-sampling arms compute the point/spot penumbra disk as
  `float lightDiskRadius = max(radius * 0.025, 1.5);`
  where `radius` is `lights[i].position_radius.w` — which `gpu_light_from_emitter` (lights.rs:94) uploads as `emitter.range.to_bethesda_units() * LIGHT_RANGE_EXTENSION`, i.e. the **cull** radius, deliberately `2.0×` the authored range (`LEGACY_LIGHT_CULL_RANGE_MULTIPLIER = 2.0`, `crates/core/src/lighting.rs:18`; `pointSpotAtten` recovers the authored radius from it as `kneeFrac * R`, lighting.glsl:66-68).

  Meanwhile the same `GpuLight` already carries a canonical emitter size:
  `params[1] = emitter.source_radius.to_bethesda_units()` (lights.rs:110), derived once at the translation boundary as
  `(range_world_units * 0.05).clamp(1.0, 32.0)` (`crates/core/src/lighting.rs:256-260`). That value is not ignored elsewhere — `pointSpotAtten` reads it as `sourceRadius` for the inverse-square arm (lighting.glsl:47), and `traceShadowTransmittanceDetailed` receives it as `emitterRadius` for the near-emitter shell test (lighting.glsl:266, `shadow_transport.glsl:31`), i.e. it is in scope at the very call the shadow sampler is making.

  In the linear middle the two agree by coincidence (`radius * 0.025 = range * 2.0 * 0.025 = range * 0.05`). They diverge at both clamps and under any change to the culling constant.
- **Evidence**:
  - **No ceiling in the shader.** CPU clamps the source radius at 32 units; the shader grows linearly forever. A 1024-unit FNV exterior lamp: shader disk `1024 * 2.0 * 0.025 = 51.2` vs canonical `32` (1.6×). A 4096-unit worldspace light: `204.8` vs `32` (6.4×).
  - **Floor mismatch.** Shader floor `1.5`; CPU floor `1.0`.
  - **Procedural emitters diverge badly.** `crates/renderer/src/vulkan/volumetrics.rs:709-736` builds combustion lights with a *physically derived* source radius — `(3V/4π)^(1/3)` clamped to `[0.02, 8.0]` m — written into `params[1]`, while `position_radius.w` is `range_metres * 70 * COMBUSTION_LIGHT_RANGE_EXTENSION`. A 3 m-range flame with the minimum 0.02 m luminous radius: canonical `params.y = 1.4` units; shader disk `max(3 * 70 * 2 * 0.025, 1.5) = 10.5` units — 7.5× too soft, and the shader's own `1.5`-unit floor alone already exceeds the canonical value. `BETHESDA_UNITS_PER_METER = 70.0` (`crates/core/src/lighting.rs:16`).
  - **Culling tunable silently owns shadow softness.** `LIGHT_RANGE_EXTENSION` is `pub const LIGHT_RANGE_EXTENSION: f32 = byroredux_core::lighting::LEGACY_LIGHT_CULL_RANGE_MULTIPLIER;` (lights.rs:55) — a pure cull-window constant. Changing it to 1.5 would shrink every penumbra by 25% with no lighting intent expressed anywhere.
  - The two shader sites are byte-identical copies, so any future retune has to be applied twice.
- **Impact**: Penumbra width is wrong wherever either clamp binds — over-soft on large-range authored lamps (interior chandeliers, exterior street lights) and grossly over-soft on the volumetric combustion lights, which are precisely the emitters for which someone did the work to compute a real physical radius. Because the disk only jitters the ray direction, the error shows up as an over-blurred contact shadow, which the ReSTIR EMA + TAA then happily converge to — it looks like a stable, deliberate soft shadow rather than a bug. It also puts a second, drifting definition of "how big is this lamp" in the tree, contradicting the `feedback_format_translation` doctrine the surrounding code cites.
- **Suggested Fix**: Replace both literals with the canonical value already in the struct:
  `float lightDiskRadius = max(lights[i].params.y, 1.0);`
  (the `1.0` floor mirrors the CPU-side `clamp(1.0, 32.0)` so a zero `params.y` from a hand-built emitter still yields a visible penumbra). This is a one-line change at each of triangle.frag:3326 and :3479, needs no new upload lane, and makes the shadow sampler consistent with `pointSpotAtten`'s inverse-square arm and with `traceShadowTransmittanceDetailed`'s shell test, which already read the same field. Add a shader-source assertion that neither arm contains `radius * 0.025`, so the cull-radius derivation cannot come back. Note this changes penumbra widths on real content, so it wants the `--bench-hold` + `byro-dbg` visual A/B the repo already uses for shadow tuning — not a blind merge.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D17-02

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
