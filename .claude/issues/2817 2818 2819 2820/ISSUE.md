# Batch: #2817 #2818 #2819 #2820

## #2817 — REN-D19-05: bs_tri_shape.rs 4th tangent branch guard is vacuous, feeds fabricated normals to tangent synthesis

**Labels**: bug, nif-parser, low
**Location**: `crates/nif/src/import/mesh/bs_tri_shape.rs` (the 4th tangent branch)

Guards on `!normals.is_empty()`, which is vacuous — `normals` is unconditionally
populated earlier (`sse_normals`, else mapped `shape.normals`, else
`vec![[0,1,0]; positions.len()]`), so the condition is equivalent to
`!positions.is_empty()`, already tested. With no authored normals the branch
hands the fabricated placeholder to `synthesize_tangents_yup`, producing a
tangent basis derived from data that was never authored — exactly the defect
#2363 fixed in `bs_geometry.rs` (guard changed to a separate
`normals_authored` flag, pinned by
`placeholder_normals_with_uvs_do_not_trigger_tangent_synthesis`); this sibling
was not updated. `sse_recon.rs` has the same shape, so an SSE buffer with
neither `VF_NORMALS` nor `VF_UVS` reaches it with *both* inputs fabricated.

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D19-05).

---

## #2818 — REN-D19-06: extract_tangents_from_extra_data has no test coverage despite load-bearing tangent/bitangent swap

**Labels**: bug, nif-parser, low
**Location**: `crates/nif/src/import/mesh/tangent.rs` (`extract_tangents_from_extra_data`)

The site of the load-bearing #786 `CalcTangentSpace` swap — Bethesda's
`tangents` field holds `∂P/∂V` and `bitangents` holds `∂P/∂U`, so the decoder
reads the **second** 12-byte half into `Vertex.tangent.xyz` — has no test
coverage, while every other tangent producer is unit-tested. Untested
consequences: the half-swap itself, the `blob.len() != num_verts * 24` size
gate (whose failure is a silent warn + fall-through to synthesis), the exact
extra-data name match, and the `zup_to_yup_pos` application to both halves.
Code reads correct today; the symptom of a regression is "chrome-looking
walls", which this project has a standing rule to *mis*attribute to missing
textures.

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D19-06).

---

## #2819 — REN-D17-05: Disney sheen tint multiplies raw albedo instead of the luminance-normalised tint

**Labels**: bug, renderer, medium, vulkan
**Location**: `crates/renderer/shaders/include/pbr.glsl` — `disneyDiffuseSplit` (the `sheenColor` line). Mirror docs: `GpuMaterial::sheen_tint` (`crates/renderer/src/vulkan/material.rs`), `Material::sheen_tint` (`crates/core/src/ecs/components/material.rs`).

`disneyDiffuseSplit` builds its sheen colour as
`mix(vec3(1.0), albedo, sheenTint)`. Both cited references build it from a
luminance-normalised tint. Disney 2012 (`disney.brdf`) computes
`Cdlum = .3r + .6g + .1b`, `Ctint = Cdlum > 0 ? baseColor/Cdlum : vec3(1)`,
`Csheen = mix(vec3(1), Ctint, sheenTint)`; knightcrawler25/GLSL-PathTracer
does the same in `GetSpecColor`. Using raw albedo couples hue and intensity:
at `sheenTint = 1.0` a dark base colour (e.g. black velvet) scales the sheen
lobe down by ~20×, and a base colour above 1.0 scales it up.

Suggested fix: `float lum = dot(albedo, vec3(0.3, 0.6, 0.1)); vec3 ctint = lum
> 0.0 ? albedo / lum : vec3(1.0);` then `sheenColor = mix(vec3(1.0), ctint,
sheenTint)`.

Blast radius today is bounded — no source-format producer writes non-zero
`sheen_tint` (NIFAL boundary writes literal 0.0, #2514) — reachable only via
the `mat.set sheen_tint …` Cornell-harness console arm, but it's a defect on
the reference-validation path itself.

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D17-05).

---

## #2820 — REN-D18-03: build_tod_keys' night anchor is clamped by an unsourced 23.0 that fires on vanilla FNV/FO3 and can go non-monotonic

**Labels**: bug, renderer, medium
**Location**: `byroredux/src/systems/weather.rs` — `build_tod_keys`, the `let night = (sunset_end + 2.0).min(23.0);` binding (key 6). Guard: `tod_keys_are_monotonic_on_realistic_climates`. Sibling of OPEN #2473 (key 4's `afternoon_cool` clamp).

Two problems with one literal:

(a) The doc comment states the model as `sunset_end + 2h` (clamped to 23h).
Every shipped Fallout climate has `sunset_end = 22.0` (FNV `[6,10,18,22]`, FO3
`[5.333,10,17,22]`), so `22+2=24` clamps to `23` on vanilla content — the
interpolator reaches full `TOD_NIGHT` an hour early, compressing the
`SUNSET → NIGHT` ease from 6h to 5h. The clamp only needs to stay under
`keys[0]+24 = 25.0`; `23.0` is 2h stricter than required with no cited
source.

(b) The clamp is absolute rather than relative to its predecessor key 5
(`sunset_begin`), so any climate with `sunset_begin > 23.0` (TNAM bytes
139–144, which pass the `1..=144` validation range) produces
`keys[5] > keys[6]` — a non-monotonic table, the exact invariant #2473
documents the consequences of.

Suggested fix (fold into #2473's fix): clamp each key against its true
predecessor — `night = (sunset_end + 2.0).max(sunset_begin + 0.1).min(24.9)`
— and extend `tod_keys_are_monotonic_on_realistic_climates` to a full
`windows(2)` assertion over a corpus including a late-sunset climate
(`[6.0, 10.0, 23.5, 24.0]`).

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D18-03).
