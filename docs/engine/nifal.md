# NIFAL — NIF Abstraction Layer

**NIFAL** (NIF Abstraction Layer; pronounced "NYE-fal") is the engine's
canonical translation tier — the cornerstone of cross-game compatibility.
"Abstraction" is the brand; the mechanism is **canonical translation** (per-game
`Imported*` → one resolved, game-agnostic representation). Throughout the code the
verbs stay `translate` / `canonical` / `resolve`; **NIFAL** is the name for the
layer as a whole.

**Status**: ACTIVE (opened 2026-05-28). Generalizes
[`material-abstraction.md`](material-abstraction.md) from the material slice to
the whole NIF pipeline.

**Goal**: every supported engine version (Oblivion / FO3 / FNV / Skyrim LE/SE /
FO4 / FO76 / Starfield) translates its native, per-game NIF data into **one
canonical, fully-resolved representation** through a single explicit
`translate()` boundary. The engine (ECS systems, renderer, gameplay) consumes the
canonical representation **identically for every game** — no per-game branches
downstream, no `Option` "resolve-it-later" fallbacks, no render-time heuristics.

This formalises the long-standing directives (`feedback_format_translation.md`:
"never per-game branches in the shader; translate at the parser→Material boundary";
`format_abstraction.md`: the GameVariant pattern) — which were documented but only
partially realised.

---

## 1. The three-tier model

```
                 parse                       translate()                   consume
  NIF bytes ─────────────▶  Imported*  ───────────────────▶  Canonical  ─────────────▶  ECS systems
            (per-game block            (raw, per-game,        (resolved,                 renderer
             structs in                 a faithful 1:1        game-agnostic,             gameplay
             crates/nif/blocks)         decode of the wire    single convention)
                                        format)
```

| Tier | What it is | Where it lives | Rule |
|---|---|---|---|
| **Raw / `Imported*`** | A faithful, per-game decode of the NIF wire format. May carry `Option`s, raw enum discriminators, per-game quirk fields. **This tier is allowed to be messy** — it mirrors the file. | `crates/nif/src/import/` (`ImportedMesh`, `ImportedNode`, `ImportedLight`, …) | Decode only; never the engine's source of truth. |
| **`translate()` boundary** | The single function that resolves a raw `Imported*` into the canonical tier. Folds in every per-game quirk so the output is one convention. | One module per category (e.g. `byroredux/src/material_translate.rs`). | Exactly **one** site per category. No duplicate construction sites. |
| **Canonical** | The resolved, game-agnostic type the engine consumes. No `Option` "resolve-later" fields; every classification decided here. | The ECS component, when one already serves the role. | The single source of truth. |

### The canonical-type rule

> **Where an ECS component already serves the game-agnostic, engine-facing role,
> that component IS the canonical type.** Introduce a *new* canonical type only
> where none exists.

We deliberately do **not** add a third `Canonical*` struct that the ECS component
then copies from — that is ceremony with no new capability and an extra copy step.
The ECS components already live low in the crate graph (`byroredux_core`), are
already game-agnostic, and are already what the renderer reads. The canonical tier
is reached by (a) making the `translate()` boundary the sole producer, and (b)
removing any residual `Option`/raw leaks from the component itself.

---

## 2. Per-category leak inventory (2026-05-28)

How close each NIF data category is to the canonical contract.

### Materials — **converged (this session)**

The reference realisation. See §3. The ECS `Material`
(`crates/core/src/ecs/components/material.rs`) is the canonical type; the boundary
is `byroredux/src/material_translate.rs::translate_material`. PBR is fully resolved
(`metalness`/`roughness` are plain `f32`, no `Option`, no render-time fallback);
glass is classified once, alpha-aware; the two previously-duplicated construction
sites are collapsed into the one boundary.

Stale notes in `material-abstraction.md` corrected: the render-side glass heuristic
(its §2 "Leak A" / §4 step-3 "still pending (b)") was already deleted, and the
`Option`-override framing of "Leak B" is now closed.

### Geometry / transform — **converged (reference template)**

Z-up → Y-up conversion (`crates/nif/src/import/coord.rs`), tangent extraction +
Mikkelsen synthesis (`mesh/tangent.rs`), local-bound derivation, degenerate-rotation
SVD repair (`transform.rs`). Per-game vertex decode (NiTriShape / BSTriShape packed
half / BSGeometry UDEC3) all converge to a single `Vec<[f32;3]>` + `Vec<u32>` in
renderer space. This is the cleanest category — it is the model the others should
match. No `Option` leaks; the consumer (`MeshRegistry::upload`) is format-agnostic.

### Skinning — **converged**

`ImportedSkin` emits **global** bone indices (#613 — partition-local remap done at
extraction) and carries the global skin transform (M41 Phase 1b.x). Palette skinning
is game-agnostic downstream.

### Lights — **converged**

`ImportedLight` resolves to a `LightKind` enum (ambient / directional / point /
spot) with a derived effective radius; the renderer never inspects the source block
type.

### Nodes — **LEAKY (next session candidate)**

`ImportedNode` carries a pile of *captured-but-never-consumed* raw passthroughs:
`bs_value_node`, `bs_ordered_node`, `tree_bones`, `range_kind`, `billboard_mode`
(consumed), `no_lighting_falloff` (on meshes), raw `flags`. These are raw-tier
fields surfaced on the import struct with no canonical translation and, in several
cases, no consumer at all. The canonical move: translate each to a resolved ECS
concept (e.g. `RenderOrderHint` from `bs_ordered_node`, LOD/billboard hints from
`bs_value_node`) **or** formally record it as deferred with a tracking issue —
not leave it as an ambiguous half-plumbed field.

### Particles — **emitter base params converged (2026-05-28)**

The scene builder still seeds a **name-heuristic preset** (torch_flame / smoke /
magic_sparkles / embers) by host-node name, but the authored `NiPSysEmitter` base
params now **override** the preset's guesses where they are genuinely authored:

- Parser: `NiPSysEmitter` is now a *typed* block carrying decoded `EmitterBaseParams`
  (the box/sphere/cylinder/array/mesh parsers read the base instead of skipping it;
  byte advancement unchanged, values captured in nif.xml order — `Radius Variation`
  interleaved before `Life Span`).
- Import: `extract_emitter_params` surfaces `ImportedEmitterParams` on
  `ImportedParticleEmitter(+Flat)` (mirrors the `color_curve` / `force_fields`
  precedent).
- Translate: `systems::particle::apply_emitter_params` (one shared helper, both
  load-path sites) applies the **kinematic + lifetime** fields (speed,
  speed_variation, declination, declination_variation, life, life_variation).
  Verified against FNV + Oblivion content (these are authored and distinctive —
  oasis torch `speed 24 / var 45.6 / life 1.33±0.67`). `initial_color` (shipped as
  the white nif.xml default) and `initial_radius` (default 1.0) are **intentionally
  not applied** — colour stays owned by the `color_curve` override, size by the
  preset — to avoid washing out tuned presets with defaults.

Spawn **rate** (particles/sec) is also authored now: `NiPSysEmitterCtlr` is a typed
block carrying its `interpolator_ref`; `extract_emitter_rate` follows it to the
`NiFloatInterpolator` constant value or its `NiFloatData` first key (legacy fallback:
`NiPSysEmitterCtlrData` first birth-rate key), and the translate sets `preset.rate`
when present. Verified authored + sane on FNV/Oblivion (oasis torch 15.0, Oblivion
torch smoke 13.3); legacy `NiParticleSystemController` content has no controller →
keeps the preset rate.

Particle **size** is authored too: the `NiPSysGrowFadeModifier` is a typed block
capturing `base_scale`, and the translate sets a **constant** `start_size = end_size
= initial_radius × base_scale` (base_scale `None` → 1.0). `base_scale` is essential —
FNV oasis smoke is `radius 50 × 0.15 = 7.5` (preset smoke 8→22), so raw radius alone
would be ~7× oversized. The grow→steady→fade *bell shape* the modifier encodes cannot
map to the canonical linear `start_size→end_size`, so only the authored *magnitude*
is translated (a size-over-life curve is future work). `initial_color` is still not
applied (white nif.xml default; colour stays with the `color_curve` override).

**Still pending (follow-ups):** size-over-life *curve* (the grow/fade bell shape needs
a richer canonical size model), and per-emitter (vs scene-first) attribution for
multi-emitter NIFs. Tooling: `crates/nif/examples/emitter_dump.rs`
(`rate / radius / bscale / speed / declination / life / initColor`).

### Collision — **audit pending**

Havok → engine transform is applied (`import/collision.rs`), shapes map to
`CollisionShape` / `RigidBodyData`. Needs an audit pass to confirm the canonical
contract holds across all bhk* shape variants (compressed mesh, mopp, list, convex).

---

## 3. Materials — the reference realisation

The material slice was executed this session as the template. Mechanics:

- **Canonical type**: ECS `Material` (`crates/core/src/ecs/components/material.rs`).
  - `metalness: f32`, `roughness: f32` — **plain, resolved, clamped to the renderer
    ranges** (`metalness ∈ [0,1]`, `roughness ∈ [0.04,1]`). The pre-canonical
    `metalness_override: Option<f32>` / `roughness_override: Option<f32>` + per-draw
    `classify_pbr` fallback are gone.
  - `material_kind: u32` — **kept as-is.** It is the GPU shader-dispatch contract
    (`GpuInstance.material_kind`, the `material_kind == N` ladder in `triangle.frag`).
    Its values (0–20 vanilla `shader_type`; 100/101 engine-synthesized
    GLASS/EFFECT_SHADER) are already resolved-at-parse and game-agnostic — a CPU
    `SurfaceClass` enum would only have to lower back to the same `u32` and would
    add a second place the ladder lives (drift risk vs the shader). **Future-slice
    invariant**: any `SurfaceClass` enum MUST lower to the exact `triangle.frag`
    ladder, and is a shader-adjacent change.
- **The boundary**: `byroredux/src/material_translate.rs::translate_material(mesh,
  paths, extra_material_flags) -> Material`. It:
  1. copies the scalars / colours / flags across;
  2. packs `effect_shader_flags` as the union of the BSEffect SLSF bits, the BGSM
     v>2 bits, and the caller's extra bits (REFR-overlay model-space-normals on the
     cell path; `0` on the loose-NIF path);
  3. seeds `metalness`/`roughness` from the BGSM/BGEM authored override (`Some`) or a
     `NaN` sentinel, then `Material::resolve_pbr()` fills sentinels from the keyword
     classifier (`classify_pbr_keyword`) and clamps;
  4. classifies glass once, alpha-aware (`helpers::classify_glass_into_material`),
     after the PBR resolve so the forced glass roughness wins.
- **De-duplication**: the two ~110-line `Material` construction sites
  (`cell_loader/spawn.rs`, `scene/nif_loader.rs`) now both call the boundary. A field
  added in one place can no longer silently diverge the two load paths.
- **Renderer**: `render/static_meshes.rs` reads `m.metalness` / `m.roughness`
  directly — no per-draw keyword scan.

### Layering note

`translate_material` lives in the top `byroredux` crate (not `core` / `nif`)
because it folds in `classify_glass_into_material` (needs
`byroredux_renderer::MATERIAL_KIND_GLASS`) and consumes the spawn sites' resolved
texture paths (BGSM `material_path` → real textures, `StringPool`-resolved). This is
the expected shape: a category whose translation needs renderer constants or
asset-provider state translates in the top crate; a category whose translation is
self-contained (geometry, skinning) can translate inside `crates/nif`.

---

## 4. Emissive scale — ground-truth measurement (2026-05-28)

`Material.emissive_mult` is fed by three NIF shader-property classes with possibly
different scales (`EmissiveSource`): `Material` (`NiMaterialProperty.emissive_mult`,
legacy), `Lighting` (`BSLightingShaderProperty.emissive_multiple`, Skyrim+/FO4), and
`Effect` (`BSEffectShaderProperty.base_color_scale`, FO4+ — semantically a
diffuse-tint scale, not emissive). Per the no-guessing policy, no normalization is
applied until the per-source scales are measured.

Instrumentation: `crates/nif/examples/material_dump.rs` now prints an `emSrc` column
(`mat` / `lit` / `fx` / `-`) beside `emisM`.

### Findings so far

Only **FNV** + **Oblivion** game data was mounted this session; both are legacy →
`EmissiveSource::Material`. Sampled emissive meshes (neon signs, torches, lava, glow
cards):

| Source | Games measured | `emisM` observed | Notes |
|---|---|---|---|
| `Material` | Oblivion (BSVER 11), FNV (BSVER 34) | **0.5, 1.0, 1.3, 7.5** | `1.0` is the common default; bright neon sign bulbs reach `7.5`. |
| `Lighting` | — | **not measured** | Needs Skyrim/FO4 data (not on disk this session). |
| `Effect` | — | **not measured** | Needs FO4+ data; also confirm it should route to a diffuse-tint path, not emissive. |

**Next step (future session, requires Skyrim/FO4/Starfield data mounted)**: run
`material_dump` over Skyrim + FO4 emissive content, tabulate the `lit` / `fx`
ranges, then decide a per-source normalization (or confirm `Effect` → diffuse-tint).
Do **not** unify the scale before those two rows have real numbers.

---

## 5. Rollout order (later sessions)

1. ~~Materials~~ — done (this session).
2. **Nodes / passthroughs** — translate `bs_ordered_node` / `bs_value_node` /
   `tree_bones` / `range_kind` / `no_lighting_falloff` to resolved ECS concepts or
   formally defer each with a tracking issue.
3. ~~Particles (emitter base)~~ — done (2026-05-28): authored kinematic + lifetime
   params override the preset. Follow-ups: spawn rate, grow/fade size, multi-emitter
   attribution.
4. **Collision** — audit the bhk* → `CollisionShape` translation for canonical
   completeness across all shape variants.
5. **Emissive unification** — once Skyrim/FO4 data is available (§4).

Each step ships independently behind `cargo test`; none touches the Vulkan
render-pass / pipeline (the shader already consumes canonical flags).

## 6. Tooling

- `crates/nif/examples/material_dump.rs` — per-mesh canonical-material dump
  (`kind / metO / rghO / gloss / env / specS / specClum / emisM / emSrc / alpha /
  decal / path`).
- `crates/bsa/examples/bsa_grep.rs` / `bsa_extract_one.rs` — find + extract a single
  NIF from a BSA for inspection.
- `tex.missing` / `mesh.info` debug-server commands — runtime per-entity material
  inspection (`byro-dbg` attach).
