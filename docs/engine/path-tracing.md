# Path Tracing — Design

**Status**: PROPOSED (2026-11-02), **exporter sections only**. M51's
Phase 0 ([path-tracing-reference-mode.md §9](../prd/path-tracing-reference-mode.md#9-delivery-sequence))
writes the rest of this design (progressive reference, interactive PT
mode, radiance cache). The two sections below were written early, in
M-LIGHT Phase 0 (#6596), because the owner's reference-lighting Q1
decision (2026-11-02) authorised building M51's exporter (PTR-VAL-004,
#6600) ahead of M51, and the reference-lighting PRD puts the rung
subsection here (its Related table). Requirement IDs use the `PTX-`
prefix.

---

## 1. Cornell scene exporter (PTR-VAL-004)

The exporter writes the scene a `--cornell*` run spawned to a Mitsuba 3
scene, from the live ECS entities, so the reference can never drift from
a hand-kept copy. The external renderer is Mitsuba 3 (M51 Q5,
reference-lighting Q1), installed by the developer into a virtual
environment and never linked by the engine.

### 1.1 Contract

- **PTX-001.** `--export-mitsuba DIR` MUST write, after the first rendered
  frame of a Cornell-family scene, `DIR/scene.xml` and one binary
  little-endian PLY per distinct mesh under `DIR/meshes/`, then let the
  run continue. The export reads `GlobalTransform`, `MeshHandle`,
  `Material`, `LightSource`, `Camera` and `Name`; it renders nothing.
- **PTX-002.** The output MUST be deterministic: entities are written in
  `Name` order (ties by mesh handle), floats in Rust's shortest round-trip
  form, PLY files named by a hash of their bytes, so two exports of the
  same scene are byte-identical and a diff shows any scene drift.
- **PTX-003.** Geometry comes from the vertex and index data the Cornell
  `MeshBuilder` uploaded, kept on the CPU in a `CornellMeshSources`
  resource keyed by mesh handle; the exporter MUST refuse an entity whose
  mesh has no recorded source rather than guess its shape.
- **PTX-004.** Positions MUST be exported in metres (world units ÷
  `BETHESDA_UNITS_PER_METER`, 70), because the engine's inverse-square
  law divides by distance in metres (`include/lighting.glsl`,
  `distanceMeters`); the `to_world` of each shape is its
  `GlobalTransform` with that scale folded in.
- **PTX-005.** The exporter MUST refuse, with an error naming the entity,
  anything reference rungs exclude (RLT-REF-009): an emissive material,
  a glass or eye material, a `LightSource` with `AttenuationModel::LegacySoftRange`
  (RLT-REF-002), an ambient emitter, a fog volume. A rung that needs one
  of them is not a reference rung.
- **PTX-006.** Every factor the exporter applies MUST be written into
  `scene.xml` as an XML comment and into `DIR/export.tsv` (emitter name,
  kind, *k*<sub>exp</sub>, exported value), so the manifest can carry them
  (RLT-REF-004, RLT-EXP-005).
- **PTX-007.** Per-rung overrides (RLT-EXP-006) MUST come from the rung's
  own code through a component on the entity (for example, "export as the
  plain `diffuse` BSDF"), never from a second scene description.

### 1.2 Camera and film

| Engine | Mitsuba 3 |
|---|---|
| `Camera::fov_y` (radians, vertical) | `perspective`, `fov_axis="y"`, `fov` in degrees |
| Camera `GlobalTransform` (looks down −Z, +Y up, right-handed) | `to_world` = `look_at(origin, origin + forward, up)` |
| Render extent (`frame_extents.render`) | `hdrfilm` `width` / `height`, `pixel_format="rgb"`, `component_format="float32"` |
| One sample per pixel centre per frame, no reconstruction filter (debug views force a raw, unjittered blit) | `rfilter type="box"` (one-pixel box) |

The handedness of the image is checked, not assumed: the first exporter
test renders the classic box (red wall on −X) and asserts the red wall
lands on the left of both images.

---

## 2. Reference-lighting rung subsection

This section is the "rung subsection of the exporter design" the
reference-lighting PRD names. It states, in Mitsuba terms, the unit
factors (RLT-REF-010), the BSDF mapping (RLT-REF-008) and the
component recipes (§3.4 of the PRD).

### 2.1 Expected factors *k*<sub>exp</sub>

Derived on paper from the code at HEAD on 2026-11-02, before any rung is
rendered. The exporter multiplies each emitter's exported intensity by
its factor; no factor is ever applied to an image.

| Kind | Engine side | Reference side | *k*<sub>exp</sub> |
|---|---|---|---|
| Point (inverse-square) | Diffuse lobe is the legacy non-/π Lambert, `diffuseBrdf = kD * albedo` (`include/lighting.glsl`); radiance at a receiver *L* = ρ · *I* · cos θ ⁄ *d*<sup>2</sup> with *d* in metres (inside the cull window, outside the source radius) | Lambertian BRDF ρ ⁄ π; a point emitter of intensity *I*′ gives *L* = ρ · *I*′ · cos θ ⁄ (π *d*<sup>2</sup>) | π |
| Sun (directional) | Same lobe; the directional light's colour × intensity *E* arrives with attenuation 1: *L* = ρ · *E* · cos θ | `directional` `irradiance` *E*′: *L* = ρ · *E*′ · cos θ ⁄ π | π |
| Environment (sky) | SH9 coefficients are the clamped-cosine convolution divided by π, "the outgoing radiance of a unit-albedo Lambertian" (`include/sky_sh.glsl`), times albedo in composite; environment specular reads cube radiance directly | `envmap` radiance; a Lambertian's response is ρ · *E* ⁄ π | 1 |

A sphere area emitter standing in for a point emitter of radius *r*
(RLT-REF-002) gets radiance *k*<sub>exp</sub> · *I* ⁄ (π *r*<sup>2</sup>)
with *r* in metres, which gives a facing receiver the irradiance
*k*<sub>exp</sub> · *I* ⁄ *d*<sup>2</sup>.

Two consequences the measurements will show, recorded here so nobody
mistakes them for unit bugs: the engine's direct light is π × brighter
than a physical renderer's for the same authored intensity, which the
point and sun factors absorb; and the GI walk shades its hits as
Lambertian ρ ⁄ π (`include/lighting.glsl`), so any mismatch between the
walk's convention and the primary lobe's appears in the L4 deficit
(RLT-GI-002), which is where it belongs. The calibration rungs
(RLT-REF-010) check the factors: a ratio *c* ≠ 1 there is a bug in this
table or the exporter.

### 2.2 BSDF mapping (Phase 1a scope: flat planes)

| Engine material | Engine lobes | Mitsuba 3 | Known differences |
|---|---|---|---|
| Lambertian rung material (the RLT-GI-010 switch on) | ρ only | `diffuse`, `reflectance` = `diffuse_color` (no sRGB decode: engine colours are scene-linear floats as authored) | None by construction |
| Dielectric, Disney branch (`MAT_FLAG_PBR_BSDF`, metalness 0) | `disneyDiffuseSplit` × π plus GGX, Schlick F0 = 0.04 | `principled`, `metallic` 0, `specular` 0.5 (F0 = 0.04), `roughness` *r*, `base_color` ρ, sheen, clearcoat and `spec_trans` 0 | Smith *k* = (*r* + 1)² ⁄ 8 remap on the engine light loop (`geometrySmith`, `include/pbr.glsl`), absent in Mitsuba (V13); Mitsuba's Burley diffuse version is recorded per roughness by the material-validation rung |
| Conductor (metalness 1) | GGX, Schlick F0 = ρ | `principled`, `metallic` 1, `roughness` *r*, `base_color` ρ | Same Smith remap; no multiple scattering in Mitsuba (V13), so the engine runs `DBG_DISABLE_MULTISCATTER` |

Every rough lobe exports `distribution="ggx"` semantics with α =
roughness², which `principled` uses natively. The material-validation
rung (RLT-REF-014) measures each row at three view angles; a row whose
recorded difference does not explain the measurement is a mapping bug.
Normal-map convention (RLT-REF-008, RLT-CON-007) is added in Phase 3,
with the normal-mapped plane.

### 2.3 Component recipes in Mitsuba terms

| PRD §3.4 quantity | Mitsuba 3 recipe |
|---|---|
| Direct | `path` integrator, `max_depth` 2 (camera vertex plus one scattering vertex), `hide_emitters` true |
| Total | `path` integrator, `max_depth` −1 (unbounded, Russian roulette from depth 5 as default), `hide_emitters` true |
| Indirect | Total − Direct, both at the same sample count from independent seeds |
| Visibility | Direct ÷ Direct of the same export with the rung's casters removed (a per-rung override, PTX-007) |

The renders use the `cuda_ad_rgb` variant where a CUDA device exists and
`llvm_ad_rgb` otherwise; both are RGB, so no spectral upsampling enters
the comparison.

### 2.4 Reference cache (RLT-REF-015)

- **PTX-008.** A rendered reference MUST be stored under a key that hashes
  the exporter's output (`scene.xml` plus every PLY), the Mitsuba version,
  the variant, the recipe and the sample count; nothing in the engine's
  shaders enters the key, so a shader-only change reuses every reference.
