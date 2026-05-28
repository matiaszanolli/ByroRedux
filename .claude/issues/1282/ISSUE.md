# Issue #1282: Interior sun-shaft leak — gate directional sun on cell_lit.is_exterior (#1277 Workstream C)

**State**: OPEN
**Labels**: bug, renderer, medium

## Body

**Child of #1277 — Workstream C (interior lighting translation).**

The Fallout casino screenshots that drove the original epic show a **hard-edged directional light shaft on the interior floor** with a crisp shadow boundary — consistent with the M34 default exterior sun leaking into an interior cell that should be lit only by its interior lights + XCLL ambient. The lighting translation layer doesn't consistently know "this cell is interior, don't apply the exterior sun."

## Symptom

Visible in the Atomic Wrangler / Strip casino interiors logged in [docs/audits/FALLOUT_SYMPTOMS_2026-05-26.md](../blob/main/docs/audits/FALLOUT_SYMPTOMS_2026-05-26.md) (the epic's exhibit A) and the follow-up screenshots in the #1277 conversation thread. The same cells render correctly on Skyrim SE — Skyrim's XCLL carries `is_exterior` info that gets propagated; FNV/FO3 either doesn't or the propagation drops it.

## Existing infrastructure

- `byroredux/src/render/sky.rs::build_sky_params` returns a `SkyParams` with `is_exterior` field, driven by `SkyParamsRes` resource (rebuilt per exterior load only).
- `crates/renderer/src/vulkan/context/draw.rs:1838` packs `depth_params.x = if sky_params.is_exterior { 1.0 } else { 0.0 }` for the composite shader.
- `triangle.frag` uses `jitter.w` (= `is_exterior`) at lines 529 and 2224 for sky-fill and aerial-perspective gates.
- Volumetrics inject already zeros sun radiance when `!sky_params.is_exterior` (`context/draw.rs:2698` block) — that path is correct.

## What's leaking

The default exterior sun is set up by M34 when no XCLL or weather data carries directional. Today on cell load:

- Skyrim: XCLL.directional_color is authored per-cell, including for interior cells where it's typically zero or very low. Engine reads it, no leak.
- FNV/FO3: XCLL.directional_color may be authored zero for interiors, but if `SkyParamsRes` is present from a prior exterior session and not cleared on interior load, the sun stays in the GpuLight SSBO and the shadow ray finds it.

The Fix #1199 commit (`39ca4bee` — "stop wiping worldspace-scoped weather/sky resources in unload_cell") deliberately KEEPS sky resources across cell transitions for exterior continuity. The right gate is probably "don't add the directional sun to the per-frame GpuLight list when the active cell is interior" — at light-list assembly, not at resource lifecycle.

## Deliverables

- [ ] **Repro test** — a headless launch of an FNV interior (e.g. `GSDocMitchellHouse` or `TOPSCasino`) after first loading an exterior cell, screenshot, look for a hard-edged sun shaft on the floor. Compare against an interior-only launch (no prior exterior load).
- [ ] **Interior sun gate at light-list assembly** in `byroredux/src/render/lights.rs::build_lights`. When `cell_lit.is_exterior == false`, skip the directional sun even if `SkyParamsRes` is present. The volumetrics zero-sun-radiance gate at `draw.rs:2698` is the model.
- [ ] **Test pinning the gate**: a unit test in `lights.rs` building a synthetic interior `CellLightingRes` + populated `SkyParamsRes` and asserting the produced GpuLight list contains no directional-with-sun-color.
- [ ] **Re-screenshot the originally-reported FNV casino interior** and confirm the floor shaft is gone.

## Adjacency

Related to (but distinct from):
- Workstream A — the matte-plastic look (Leak B in material-abstraction.md). The hard shadow boundary of the sun shaft is a separate visible symptom; the matte material makes it look worse.
- #1199 — exterior sky persistence across cell loads (which IS correct, but exposed this leak when the consumer-side gate was missing).

## References

- Parent epic: #1277
- Symptom record: [docs/audits/FALLOUT_SYMPTOMS_2026-05-26.md](../blob/main/docs/audits/FALLOUT_SYMPTOMS_2026-05-26.md) (Atomic Wrangler / Gomorrah screenshots)
- Sky/light assembly: `byroredux/src/render/sky.rs`, `byroredux/src/render/lights.rs`
- The vol-gate model: `crates/renderer/src/vulkan/context/draw.rs:2698`
