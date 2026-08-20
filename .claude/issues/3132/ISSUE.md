# SAFE-2026-08-20-01: WATR floats reach the water UBO without a finiteness gate — f32::clamp is NaN-transparent and two of six decoders read through the unfiltered SubReader::f32()

**Issue**: #3132 — https://github.com/matiaszanolli/ByroRedux/issues/3132
**Finding**: `SAFE-2026-08-20-01`
**Labels**: bug, import-pipeline, medium, safety
**Filed**: 2026-08-20 (comprehensive `/audit-suite` sweep, 25 reports)

---

**Audit**: `docs/audits/AUDIT_SAFETY_2026-08-20.md` — Dimension 9 (NIFAL boundary — NaN/Inf on the GPU)
**Severity**: MEDIUM · **Status**: NEW

## Location
- `crates/plugin/src/esm/records/misc/water.rs:363-400` (`decode_data`) and `:694-730` (`decode_dnam_pre_fo4`)
- NaN-transparent clamps at `:382`, `:385`, `:388`, `:715`, `:718`, `:721`
- Downstream: `byroredux/src/env_translate.rs:715-716`, `:737`, `:753`
- Assembly into the GPU record: `byroredux/src/render/water.rs:223-330`

## Description
`GpuWaterParams` is assembled field-by-field from `WaterMaterial` with no finiteness pass, and there is no `WaterMaterial` analogue of `Material::resolve_pbr` anywhere in the tree (`grep -n "fn sanitize\|fn resolve\|fn validate" crates/core/src/ecs/components/water.rs` returns nothing). Whether a non-finite value can reach it therefore depends entirely on the WATR decoders. Four of the six filter correctly; two do not:

- `read_f32_at` (`water.rs:901-905`) ends with `value.is_finite().then_some(value)`, so every decoder built on it (`decode_dnam_fo4`, `decode_dnam_fo76`, `decode_dnam_starfield`, `apply_skyrim_dnam_tail`) is clean. So is the `NAM1` arm (`:1322`) and the `NAM0` arm (`:1382`), both of which check `is_finite()` explicitly.
- `SubReader::f32` (`crates/plugin/src/esm/sub_reader.rs:152-155`) is a bare `f32::from_le_bytes` with **no** finiteness check. `decode_data` and `decode_dnam_pre_fo4` read nine floats each through it.

The `.clamp()` calls on those values are not a rescue. Rust's `f32::clamp` is specified to return `NaN` when `self` is `NaN` (it is `if x < min {min} else if x > max {max} else {x}`, and both comparisons are false for NaN). The `.max()` calls a few lines away in the *same functions* — `fog_near = v.max(0.0)` — *do* filter, because `f32::max` returns the non-NaN operand. The file mixes a NaN-safe idiom and a NaN-transparent one with no visible distinction.

Four fields are worse still: `wind_speed` (`:369`), `wind_direction` (`:372`), `wave_amplitude` (`:375`, `:709`) and `wave_frequency` (`:378`, `:712`) are assigned **raw**, with neither `clamp` nor `max`.

## Evidence
```rust
// crates/plugin/src/esm/records/misc/water.rs:374-389 (decode_data)
if let Ok(v) = r.f32() { p.wave_amplitude = v; }                        // raw
if let Ok(v) = r.f32() { p.wave_frequency = v; }                        // raw
if let Ok(v) = r.f32() { p.sun_specular_power = v.clamp(1.0, 2048.0); } // NaN-transparent
if let Ok(v) = r.f32() { p.reflectivity = v.clamp(0.0, 1.0); }          // NaN-transparent
if let Ok(v) = r.f32() { p.fresnel = v.clamp(0.0, 1.0); }               // NaN-transparent
if let Ok(v) = r.f32() { p.fog_near = v.max(0.0); }                     // NaN-SAFE
```
Carried through unchanged:
```rust
// byroredux/src/env_translate.rs
715:  mat.fresnel_f0 = rec.params.fresnel.clamp(0.001, 0.20);   // NaN in → NaN out
716:  mat.reflectivity = rec.params.reflectivity;
737:  mat.wave_amplitude = rec.params.wave_amplitude;
753:  mat.sun_specular_power = rec.params.sun_specular_power;
```
Into the UBO record:
```rust
// byroredux/src/render/water.rs:255-272
tune:         [mat.uv_scale_a, mat.uv_scale_b, mat.shoreline_width,
               mat.wave_amplitude * wind_wave_scale],
misc:         [mat.fresnel_f0, mat.wave_frequency, …, mat.sun_specular_power],
tint_reflect: [.., .., .., mat.reflectivity],
```

`decode_dnam_pre_fo4` is the **Skyrim** DNAM decoder (`:1352-1357`) and the `_ =>` fallback for FO3/FNV/Oblivion (`:1359`) — first-tier target games with real water content.

Reachability is not hypothetical: the WATR layout arbitration (#3104–#3110) established that these offset maps are field-shifted on the majority of vanilla WATR — the parser's `wind_speed` is a constant 90.0 across the corpus — i.e. these readers are already interpreting neighbouring bytes (colour bytes, flags, FormIDs) as `f32`. An arbitrary 32-bit word decodes to NaN or ±inf for any exponent-`0xFF` pattern.

The delta explicitly guards the *sibling* input and tests it:
```rust
// byroredux/src/render/water.rs:106  (WindField, an engine resource)
let gust = if gust.is_finite() { gust.max(0.0) } else { 0.0 };
// byroredux/src/render/water_wave_params_tests.rs:266-285
fn non_finite_weather_gust_keeps_water_params_finite() { … assert!(params.tune[3].is_finite()); }
```
The untrusted input received no such treatment.

Verified at HEAD: `sub_reader.rs:152-155` is still an unfiltered `f32::from_le_bytes`; `water.rs:901-905` still carries the `is_finite()` filter that `decode_data` bypasses.

## Impact
A single non-finite WATR field yields a NaN in the water UBO for every visible water plane using that record. `water.vert:188` reads `clamp(water.tune.w, 0.0, 32.0)` for wave amplitude — GLSL `clamp`/`min`/`max` are *undefined* for NaN operands per the GLSL spec, and lower on NVIDIA's `x < y ? y : x` expansion NaN survives — so a NaN amplitude produces NaN vertex positions and an undefined-rasterisation water primitive.

On the shading side a NaN `fresnel_f0` / `reflectivity` / `sun_specular_power` writes NaN into the HDR colour target, which is then consumed by the TAA history and the bloom downsample pyramid; both are neighbourhood filters, so a single NaN pixel spreads rather than staying local.

Blast radius is per-water-record, on Skyrim / FO3 / FNV / Oblivion — the whole classic-game tier.

## Related
**#2687** (OPEN, SAFE-D9-01) is the same class — a renderer-bound producer with no finiteness gate — on the `Material` / save-restore path rather than `WaterMaterial`. **#2489** (OPEN) is the `mat.set` console clamp. This finding is a third, distinct producer. Adjacent: the FIXED #3048 (FaceGen), which established that "checks the inputs, not the outputs" is the recurring shape of this bug. The decoder layout itself is settled by #3104–#3110.

## Suggested fix
The cheapest correct fix is one line in `crates/plugin/src/esm/sub_reader.rs` — but `SubReader::f32` is shared by every ESM record type, so scope it instead: add an `f32_finite()` helper, or route `decode_data` / `decode_dnam_pre_fo4` through the existing `read_f32_at`, which already has the filter.

Belt-and-braces: give `WaterMaterial` a `resolve()` that clamps every scalar to a finite range and call it at the single `env_translate` exit, mirroring `Material::resolve_pbr`. Pin it with a `non_finite_watr_keeps_water_params_finite` test alongside the existing weather-gust one.

## Completeness Checks
- [ ] **SIBLING**: Every `SubReader::f32()` consumer that feeds a GPU-bound struct checked for the same gap, not just the two WATR decoders
- [ ] **CANONICAL-BOUNDARY**: The finiteness gate belongs at the parser→component boundary (or the single `env_translate` exit), never re-derived at render time or pushed into `water.vert`/`water.frag`. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
