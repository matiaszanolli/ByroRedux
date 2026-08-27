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
(`crates/core/src/ecs/components/material.rs`) is the canonical type. Its boundary
is the ordered pipeline formed by `merge_external_material`,
`pack_imported_material_flags`, `translate_material`, and the final
`Material::resolve_pbr` invariant enforcement — plus a **second, post-texture-resolution
phase** for the two fields that cannot be known until texture handles exist
(#2330; see §3's *Two-phase boundary*). PBR is fully resolved
(`metalness`/`roughness` are plain `f32`, no `Option`, no render-time fallback);
glass is classified once, alpha-aware; the two previously-duplicated construction
sites both use this pipeline.

Stale notes in `material-abstraction.md` corrected: the render-side glass heuristic
(its §2 "Leak A" / §4 step-3 "still pending (b)") was already deleted, and the
`Option`-override framing of "Leak B" is now closed.

Residual gap (#2284 / MAT-D1-NEW-04, fixed 2026-08-05): six
`BSLightingShaderProperty` shading scalars (`lighting_effect_1/2`,
`subsurface_rolloff`, `rimlight_power`, `backlight_power`, `fresnel_power`) now
land on the canonical `Material`. **The GPU-side follow-up closed on
2026-08-25:** all six are present in the 432-byte `GpuMaterial`, hashed in both
material-table paths, mirrored in GLSL, and consumed by the canonical direct,
glass, and GI response. Soft/rim/back feature flags and their lighting-mask /
back-light texture roles are normalized before the renderer; the shader never
asks which game or source format authored them.

That paragraph and the `Material` doc used to call this "matching the existing
`grayscale_to_palette_scale` precedent". It was not one, and the correction
(#2592 / SKY-D7-04) is what kept this section's **converged** verdict honest:
`grayscale_to_palette_scale` did not reach `Material` at all. It was captured on
`ImportedMaterial` (`asset_provider/material.rs` reads it off the BGSM) and
then dropped by `translate_material` — an actual boundary omission, one tier
earlier than the #2284 fields.

**#2443 (MAT-D3-01) closed it**: `Material::grayscale_to_palette_scale` now
exists and `translate_material` copies it, so the field has caught up to the
#2284 six and the two groups really are the same shape. The 2026-08-25 GPU
follow-up added `GpuMaterial.grayscale_to_palette_scale` and applies it as a
bounded blend in both effect-shader and lit BGSM palette paths, so authored
non-1.0 values now affect rendered output without a format-specific branch.

### Mesh water — **converged at the NIFAL/WATAL seam**

Dedicated `WaterShaderProperty` / `BSWaterShaderProperty` meshes cross NIFAL
through `byroredux/src/material_translate.rs`, rather than constructing water
components independently at their consumers. The live boundary has four
reviewable helpers:

- `water_material_from_mesh` translates canonical `Material` optics and the
  authored normal/flow-map handles into `WaterMaterial`;
- `water_kind_from_mesh_geometry` combines conservative name classification
  with mesh orientation, keeping horizontal waterfall-named planes as rivers;
- `water_volume_from_mesh` derives the Y-up gameplay volume for non-waterfall
  surfaces; and
- `attach_mesh_water` composes `WaterPlane`, optional `WaterFlow`, and optional
  `WaterVolume` once, including canonical foam-by-kind.

Both consumers—`cell_loader/spawn/mesh_instance.rs::spawn_mesh_instance` and
`scene/nif_loader.rs::load_nif_bytes_with_skeleton`—call
`attach_mesh_water`. Mesh-water damage remains deliberately parked at
`damage_per_second = 0.0`: neither NIF water shader property authors a gameplay
hazard, unlike ESM WATR/XNAM content. See `docs/engine/watal.md` for the shared
rendering/physics contract.

### Geometry / transform — **converged (reference template)**

Z-up → Y-up conversion (`crates/nif/src/import/coord.rs`), tangent extraction +
Mikkelsen synthesis (`mesh/tangent.rs`), local-bound derivation, degenerate-rotation
SVD repair (`crates/nif/src/rotation.rs` — `sanitize_rotation` /
`repair_rotation_svd_or_identity`, fired once at parse; `transform.rs::compose_transforms`
assumes already-sanitized rotations). Per-game vertex decode (NiTriShape / BSTriShape packed
half / BSGeometry UDEC3) all converge to a single `Vec<[f32;3]>` + `Vec<u32>` in
renderer space. This is the cleanest category — it is the model the others should
match. No `Option` leaks; the consumer (`MeshRegistry::upload`) is format-agnostic.

### Skinning — **half-stale (2026-08-07): loose-NIF path only**

`ImportedSkin` emits **global** bone indices (#613 — partition-local remap done at
extraction) and carries the global skin transform (M41 Phase 1b.x). Palette skinning
is game-agnostic downstream — but only for the entities that actually get a
`SkinnedMesh` component, and until #2440 (NIFAL-D2-02) that was the loose-NIF
path exclusively.

`ImportedMesh.skin` is populated identically by the shared mesh extractors on
BOTH load paths, but only `scene/nif_loader.rs` (the sole `SkinnedMesh::
new_with_global` call site) translates it into the canonical component —
resolving each bone name against a per-placement `node_by_name` map the
loose-NIF loader builds while spawning the full NiNode hierarchy as entities.
The cell loader (`cell_loader/spawn.rs`) has no equivalent per-placement node
map at all — it flattens each NIF to a mesh list (see the "Nodes" entry above
for the same structural split re: `billboard_mode`/#2206) and reads
`mesh.skin` exactly once, as a boolean negative filter for the
architecture-trimesh collider fallback. Any cell-placed REFR with skinned
geometry (Skyrim/FO4 wind-animated cloth banners, chains, hanging/moveable
statics using `NiSkinInstance`) spawns with skin data parsed and per-vertex
weights uploaded, but no palette binding — it renders frozen in bind pose.
NPC actors are unaffected; they always route through the loose-NIF path.

Not fixed inline: building `SkinnedMesh` correctly on the cell-loader path
needs bone nodes to exist as entities in the first place, which requires the
cell loader to grow a per-placement node-entity map it does not have today —
a materially larger, riskier change than collapsing an existing translation
onto a shared boundary (contrast #2439's `translate_light`, three producers
converging on data already flowing through each site). Recorded here per
#2440's own suggested resolution rather than attempted as a rushed structural
change to the cell loader's spawn architecture.

**Residual note (#2441 / NIFAL-D2-03):** `SkinnedMesh.bones` /
`skeleton_root` carry `Option`s past the translation boundary
(`crates/core/src/ecs/components/skinned_mesh.rs`) rather than resolving
before construction — `compute_palette_into` substitutes `Mat4::IDENTITY`
for `None` entries. This is a terminal "bone-name lookup against the
placement's node map failed" sentinel, logged at the producer
(`scene/nif_loader.rs`'s unresolved-bone warning), not a resolve-later leak
that some future pass is expected to close out. Recorded explicitly so a
future audit reading this section doesn't have to rediscover that distinction
from the code.

### Lights — **converged**

`ImportedLight` resolves to a `LightKind` enum (ambient / directional / point /
spot) with a derived effective radius; the renderer never inspects the source block
type. `LightKind` itself now lives on the canonical `LightSource` component
(`crates/core/src/ecs/components/light.rs`) — `byroredux_nif::import::LightKind` is
a re-export of the same type, not a second copy.

Was **half-stale 2026-07-27 → 2026-08-02** (NIFAL-D3-01 / #2205): `kind` /
`direction` / `outer_angle` were resolved correctly at import but the canonical
`LightSource` had no field to receive them, so every placed light — including
Oblivion's ubiquitous `NiDirectionalLight` fixtures — spawned as a point light.
Fixed by adding the three fields to `LightSource` and wiring
`GpuLight.color_type.w` / `direction_angle` from them at the render boundary
(`byroredux/src/render/lights.rs`); the renderer-side point/spot/directional
support this unblocks already existed (`lighting.glsl`).

Was **half-stale, one tier up, 2026-08-02 → 2026-08-07** (NIFAL-D2-01 /
#2439): #2205 fixed the field, but only the direct-`NiPointLight`/
`NiSpotLight` NIF-import producer ever populated it — the three ESM-LIGH-
sourced producers (`cell_loader/spawn.rs`, `cell_loader/references/mod.rs`
×2) hand-copied radius/color/flags/falloff and hard-defaulted the rest, so
every ESM-placed spotlight in every supported game still rendered as a full
omnidirectional point light. `LightData` had no `fov_degrees` field (LIGH
DATA/DAT2 byte 20-23, present but unread) and the codebase had no constant
for xEdit's `'Spot Light'` shape bit (0x200) — only the DISTINCT `'Shadow
Spotlight'` projection-technique bit (0x400, `LIGHT_FLAG_SHADOW_SPOTLIGHT`)
existed, which the audit's own suggested fix would have conflated with the
shape signal. Fixed by adding `LIGHT_FLAG_SPOT` (0x200) and a
`translate_light(ld, game, ref_rot) -> LightGeometry` boundary
(`byroredux/src/systems/light_anim.rs`, sibling to
`canonical_light_shadow_flags`) that all three ESM producers now collapse
onto; `direction` is `ref_rot` applied to Gamebryo's `(1, 0, 0)` `NiSpotLight`
model direction (verified against `gamebryo-v32/Include/NiSpotLight.h`, NOT
local -Z), the same convention already validated for `NiDirectionalLight` via
`euler_zup_to_quat_yup_tests.rs`.

### Nodes — **triaged (2026-05-28)**

The live node data is canonical: `name`, `flags` (→ `SceneFlags`), `collision`
(→ Havok-transformed `CollisionShape`/`RigidBodyData`), and `billboard_mode`
(→ `Billboard`) are all consumed at the spawn sites. Unlike materials, the
`ImportedNode` → ECS step is **not** a duplicated literal to dedupe: the two load
paths handle nodes structurally differently (the loose-NIF loader spawns the full
NiNode hierarchy as entities; the cell loader uses a flattened placement-root), so
there is no single `translate_node` boundary to collapse them into.

Was **half-stale 2026-05-28 → 2026-08-03** (NIFAL-D4-02 / #2206) for
`billboard_mode` specifically: that claim held for the loose-NIF path
(`ImportedNode::billboard_mode` → `Billboard`, `scene/nif_loader.rs`) but not the
cell-loader path — `walk_node_flat` never produces `ImportedNode`s at all (it's a
flat mesh list, not a hierarchy), so `CachedNifImport::placement_root_billboard`
sat hardcoded `None` and no cell-loaded `NiBillboardNode` (213–1,527 vanilla
instances per game archive) ever rotated to face the camera. Four prior audit
sweeps restated the PASS from this doc's prose instead of the code. Fixed by
propagating the nearest-ancestor billboard mode onto `ImportedMesh` itself in
`walk_node_flat` (the flat walk's per-mesh sibling of `ImportedNode::
billboard_mode`) and attaching `Billboard` per mesh entity in
`byroredux/src/cell_loader/spawn.rs`, reusing the #1235 `flags`-parity pattern.
The `.spt` SpeedTree placeholder path (#994) is unaffected — it never goes
through `walk_node_flat` and keeps using `placement_root_billboard` on its own
single-node scene.

Four fields are **raw-tier-parked with translation formally deferred** — verified
(2026-05-28) to have *zero* engine consumers. They are NOT leaks: they sit on the
raw `ImportedNode` (which the tier model permits to carry per-game data) and have
**not** reached any canonical ECS component. Each is blocked on a consumer feature
that does not exist yet; translating them now would mean inventing ECS components
nothing reads. Deferred deliberately, not overlooked:

| Field | Source block | Authored data | Blocked on (future consumer) |
|---|---|---|---|
| `bs_value_node` | `BSValueNode` | LOD-distance override / billboard-mode hint (FO3/FNV) | M35 LOD selector / billboard hinting |
| `bs_ordered_node` | `BSOrderedNode` | alpha-sort bound + draw-order hint | `RenderOrderHint` + `build_render_data` sort-key tweak |
| `tree_bones` | `BSTreeNode` | SpeedTree branch/trunk bone names | SpeedTree wind/bend simulation |
| `range_kind` | `BSRange/DamageStage/Blast/DebrisNode` | destructible/blast/debris discriminator | destructible-switching / blast / debris systems |

When any of those consumer features lands, its slice translates the parked field into
the canonical ECS concept (the data is already captured, so no parser/import change is
needed then). Until then this table is the record that the gap is known and bounded.

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
  the white nif.xml default) is **intentionally not applied** — colour stays owned by
  the `color_curve` override — to avoid washing out tuned presets with defaults.
  `initial_radius` **is** applied, via `initial_radius × base_scale`; see the
  "Particle **size** is authored too" paragraph below, which owns that contract.

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

This paragraph is the authority on particle size. Until #2488 the bullet above still
carried the pre-size-work claim that `initial_radius` was "intentionally not applied"
and that size stayed owned by the preset — the opposite of both this paragraph and
`systems::particle::apply_emitter_params`, and the first of the two an auditor reads.
A change that "restored the documented invariant" by dropping the size override would
regress FNV oasis smoke back to ~7× oversized.

**Still pending (follow-ups):** size-over-life *curve* (the grow/fade bell shape needs
a richer canonical size model), and per-emitter (vs scene-first) attribution for
multi-emitter NIFs. Tooling: `crates/nif/examples/emitter_dump.rs`
(`rate / radius / bscale / speed / declination / life / initColor`).

**Starfield: particle slice N/A** (#2354 / SF-D8-03, 2026-08-03 audit). This
whole slice — `extract_emitter_params`/`extract_emitter_rate` dispatching on
`NiPSysEmitter`/`NiPSys*FieldModifier` — is structurally unreachable on
Starfield content, not a silent leak: the full Meshes01 corpus (31,058 files,
22 distinct block types) contains zero `NiPSys*`/`NiParticleSystem` blocks.
Starfield authors particle systems entirely outside the NIF container (the
BSGeometry-only import path, Dimension 2 of the same audit, has no
NiNode/NiParticleSystem hierarchy for Starfield content at all). Pinned by
`crates/nif/tests/per_block_baselines.rs::starfield_corpus_has_no_particle_blocks`
(always-on, reads the checked-in corpus histogram — no game data needed) so a
future format discovery flips a test red instead of the particle regression
suite (#1411/#1434/#1445/#1771/#1775, all Oblivion/FO3/FNV/Skyrim-driven)
silently saying nothing about Starfield coverage. FO76 (same BA2/BGSM/
BSGeometry era) is unconfirmed either way — no FO76 audit has run this check
yet.

### Collision — **audited (2026-05-28; remediation 2026-07-30)**

Havok → engine transform + `havok_scale` are applied uniformly in
`import/collision/shape.rs::resolve_shape`, and the bhk* shapes map to `CollisionShape` /
`RigidBodyData`. The audit diffed every parsed `bhk*Shape` struct against the
translated set and found **two leaks** (parsed for byte-correctness, then dropped at
the "unsupported shape" fallback → the authored collision silently vanished):

- `BhkMultiSphereShape` → now a `Compound` of `Ball` children at each sphere's
  (scaled) center (single centred sphere unwraps to a plain `Ball`).
- `BhkConvexListShape` → now a `Compound` of resolved convex sub-shapes (mirrors
  `BhkListShape`; FO3/FNV/Skyrim destructibles + debris).

The authoritative shape-coverage inventory is the type-dispatch in
`import/collision/shape.rs::resolve_shape_inner`; do not copy its arm count into
this spec. Every parsed shape is either mapped there, explicitly routed through a
wrapped child, or deliberately rejected to the documented mesh/trigger fallback.
Remaining collision *non*-leaks are documented limitations, not gaps:
`BhkNPCollisionObject` (FO4+ Havok-serialised blob — decoder is a separate project)
and `BhkPCollisionObject` phantoms (need a `TriggerVolume` ECS path, not a rigid
body) — see the table at the top of `import/collision/mod.rs`.

**`BhkNPCollisionObject` fallback coverage** (#2355 / SF-D8-04, 2026-08-03 audit,
closed by `8ee151e0`/`716b7ee9`/`8d67c700`): this is Starfield's *entire* collision
authoring — the corpus census found zero `bhk*Shape` blocks and 100% `BhkSystemBinary`.
`cell_loader/spawn.rs::missing_collision_fallback` picks the proxy by `RenderLayer`:
`Architecture` (structural — walls/floors/built-in containers) gets a precise
per-submesh synthesized trimesh; `Clutter`/`Actor` get a conservative placement-
following AABB proxy (`PackedAabbProxy`, added specifically to close the "no collider
at all" gap the audit found for non-Architecture content) gated on
`CollisionAuthoringSummary::needs_packed_havok_fallback()`. Per-cell counts of
approximated vs. unresolved placements log via `references/mod.rs`'s
`packed_collision_fallbacks` / `unresolved_packed_collision` line. The real fix
(decoding the `BhkSystemBinary` blob itself) remains future work tracked in the
PHYSAL notes — this is a spawn-time compatibility proxy, not a NIFAL translation-
boundary change.

The 2026-07-30 playable-cell remediation corrected four canonical-boundary bugs
found by the later real-data audit: compressed-mesh chunk indices are direct
vertex indices (only the authored vertex component count is divided by three);
strip runs may be followed by a plain triangle-list tail; a resolved `TriMesh`
must contain at least one finite, in-range, non-degenerate triangle or return
`None` so the cell loader's synthesized fallback remains available; and cuboid
half-extents use the Z-up → Y-up axis permutation without the position
transform's sign. Release runs against Skyrim SE now keep the player grounded in
both `BleakFallsBarrow01` and the `WhiterunDragonsreach` control. Oblivion's
separate inverted-contact-normal follow-up remains tracked by #2193.

### Animation / controllers — **converged (surveyed 2026-06-02)**

Both entry points — `anim::import_kf` (KF sequences) and `anim::import_embedded_animations`
(mesh-embedded controllers) — funnel through one set of `extract_*_channel_at`
cores (the `extract_*` wrappers are ControlledBlock→block-index adapters, not
duplicated logic), and `anim_convert::convert_nif_clip` is the single NIF→core
`AnimationClip` boundary. The canonical type is the ECS `AnimationClip`
(`crates/core/src/animation/`).

Correction (#2442 / NIFAL-D2-04, 2026-08-07): "no parallel struct" above
overstated it — `byroredux_nif::anim::AnimationClip` (`crates/nif/src/anim/
types.rs`) is a distinct, identically-named raw-tier type the tier model
explicitly permits (§1's canonical-type rule is about ECS components, not raw
parse-tier structs). It's correctly type-qualified at every call site and
never reaches the ECS unconverted — `anim_convert::convert_nif_clip` is still
the one boundary that turns it into the canonical type. The defect was only
doc precision: a grep-based "single producer" check for `AnimationClip` is
ambiguous between the two same-named structs without reading this note.
Reworded rather than renaming the raw type to `ImportedAnimationClip` (the
`Imported*` convention every other raw-tier type in this doc follows) — a
rename is still open as a low-risk future cleanup if the ambiguity proves
costly in practice; not required to close this finding.

Every per-game variation is resolved at import: B-spline compressed interpolators (FO3/FNV + Skyrim+) are
sampled to linear keys, XYZ-Euler rotation keys are composed to quaternions,
TBC/Hermite tangents are decoded, and Z-up→Y-up runs once — the player/stack
consumers see only game-agnostic quaternion keys with no `Option`/era branches.
Text-key events are wired (`NiControllerSequence.text_keys_ref` →
`AnimationClip.text_keys` → `AnimationTextKeyEvents` ECS → scripting); embedded
controllers set `text_keys: Vec::new()` by design (mesh-local controllers carry no
event keys). Intentionally parked (captured, no renderer consumer yet, *not*
a leak): per-light **ambient** colour channels. **Morph-weight** channels are
NOT in this state — since `a8b0cf64` they reach a live `AnimatedMorphWeights`
ECS sink every frame (confirmed by `sink_lifecycle_end_to_end_tests`); they
only lack a downstream GPU/mesh-vertex-blend consumer, tracked separately by
#2221.

`anim_convert::convert_nif_clip` is the single NIF→core boundary, but not the
only production boundary for the canonical `AnimationClip`: Skyrim's cart/
furniture idles ship as Havok 2010 packfiles (`.hkx`), not NIF, so
`byroredux/src/asset_provider/animation.rs::convert_hkx_clip` is a second,
declared boundary — same source-agnostic `AnimationClip` target, no parallel
struct (#2305 / NIFAL-D7-NEW-01). It reads `HkxSkeleton`/`HkxAnimation` (the
`hkx` crate's parse of the packfile) and builds `TransformChannel`s directly,
same Z-up→Y-up conversion as the NIF path. One deliberate, documented
exception to no-fabrication: `behavior_completion_events` synthesizes two
text-key events — `ExitCartEnd` and `IdleFurnitureExit` — appended at
`animation.duration` for any idle whose event name matches the
`idlecart*exit` pattern, when neither is already present in the authored
annotations. Those completions live in Skyrim's behavior graph, not in the
per-clip `.hkx` data the parser reads, so `convert_hkx_clip` fabricates the
two the ECS scripting layer needs to detect cart/furniture-exit completion
without a full behavior-graph interpreter.

### Shader flags / texture sets / effect shaders — **converged (surveyed 2026-06-02)**

The "GameVariant trait" the early docs called aspirational is realised *as the
correct shape*, not as a trait: per-game flag vocabularies live as namespaced
constants in one file (`shader_flags.rs` — `fo3nv_f1`, `skyrim_slsf1`, `fo4_slsf1`,
+ FO76/Starfield CRC32 arrays), dispatched by **block type** (the wire format
already discriminates the game), with `#[test]`-gated runtime equivalence asserts
(bits 26/27, e.g. `fo3nv_and_skyrim_decal_bits_agree`) guarding the bit-meaning
collisions (e.g. bit 21 flags2 = Alpha_Decal/Cloud_LOD/Anisotropic across three
games). The `ShaderFlags<'a>` typed view that used to wrap these constants was
deleted as transitively dead (#1897); production import reads the constants
directly via `is_decal_from_legacy_shader_flags` / `is_decal_from_modern_shader_flags`
/ `is_two_sided_from_modern_shader_flags` (`crates/nif/src/import/material/mod.rs`).
Decal / two-sided are read once per property type into `MaterialInfo`; the renderer reads
`material.is_decal` / `two_sided` with no per-game branch (verified: `triangle.frag`
has zero `if game ==`). Texture-slot→role mapping (`BSShaderTextureSet`) is one
decision tree keyed on `shader_type` (block structure, not a game check). All 9
`BSLightingShaderProperty` shader-type variants now forward their trailing data
(SkinTint/HairTint/Parallax/MultiLayer/Eye/Sparkle — the pre-#343 8-of-9 drop is
closed). `BSEffectShaderProperty` is captured + routed (`material_kind == 101`,
EFFECT_* flags); the one *deferred* item is the `base_color_scale`
diffuse-tint-vs-emissive render path (§4) — tagged via `EmissiveSource::Effect`, not
dropped.

SLSF1 `Refraction` (bit 15, shared position/semantic across `fo3nv_f1` /
`skyrim_slsf1` / `fo4_slsf1`) is a partial exception, documented rather than
fixed (SKY-D7-02 / #2327): `refraction_strength`, the scalar it gates, is
captured into `ImportedMaterial` for every Skyrim+ material
(`dedicated_shader.rs::apply_bs_lighting_shader` — shared code across
Skyrim/FO4/FO76/Starfield, not per-game) but only reaches the canonical
`Material.ior` field when paired with `Fire_Refraction` too
(`material_translate.rs::material_optical_scalar`, `material_kind ==
MATERIAL_KIND_FIRE_REFRACTION`). This is deliberate, not a translation
leak: nif.xml's own spec for "Refraction Strength" states it is "**not
based on physically accurate refractive index**" (0-1 distortion amount),
so it cannot correctly ride `ior` — a real 1.0+ physical index the RT
refraction path traces against — for an ordinary dielectric. `Refraction`
authored *without* `Fire_Refraction` (ordinary refractive glass/ice/crystal)
therefore has no engine consumer for its authored distortion intent today;
that gap needs its own canonical field + shader consumer to close, not a
reuse of `ior`.

`ImportedMesh` now owns one source-agnostic `ImportedMaterial` payload. The
NiTriShape, BSTriShape, and BSGeometry extractors all delegate
`MaterialInfo` → `ImportedMaterial` construction to the same boundary instead of
forwarding the material field set independently. `ShaderTypeFields` is likewise
defined once in `byroredux_core` and carried through the import boundary directly;
there is no NIF-local mirror or `to_core()` copy to keep synchronized.
`ImportedMesh` does not dereference implicitly to its material: consumers cross
the boundary explicitly through `mesh.material`, while external BGSM/BGEM/`.mat`
resolution and canonical ECS translation accept `ImportedMaterial` directly.
FO4 precombined CSG geometry also carries this full payload; the former
`PrecombineMaterial` subset and field-by-field patch operation were removed.

Texture paths cross the NIF boundary in one generic semantic contract:
`MaterialTextureSet<T>`. Its 18 named roles plus four ordered decal layers cover
legacy `NiTexturingProperty`, `BSShaderTextureSet`, inline effect shaders, and
BGSM/BGEM material files. The source-specific slot vocabulary is gone before
runtime spawning; the same shape is reused as `MaterialTextureSet<Option<String>>`
for resolved paths and `MaterialTextureSet<u32>` inside
`MaterialTextureHandles`. Tint, inner-layer, standalone specular, reflectance,
emittance-gradient, and legacy decal layers now reach fragment shading alongside
the established base/normal/emissive/detail/smoothness/dark/height/environment
roles. Lighting, flow, and wrinkle roles are preserved through the GPU contract
but remain deliberately unsampled until their authored lookup coordinates or
actor controls are available; treating them as ordinary RGB modulation would be
format-specific guesswork at the supposedly game-agnostic boundary. Generic
`values()` / `secondary_values()` traversal is the exhaustive lifecycle contract;
cell unload uses it directly rather than maintaining a separate role list.

### Passthroughs — parked / dropped inventory (surveyed 2026-06-02)

The 2026-06-02 coverage sweep traced every `ImportedScene`/`ImportedNode`/`ImportedMesh`
field to its consumer. Beyond the four §"Nodes" fields, these are **parsed but not
yet consumed** — each blocked on a feature that does not exist, so translating now
would invent an ECS component nothing reads (the no-fabrication rule). This table is
the record that each gap is known and bounded, with its unblocking consumer:

| Data | Source block | State | Blocked on (future consumer) |
|---|---|---|---|
| `ImportedTextureEffect` | `NiTextureEffect` | extracted (`import_nif_texture_effects`) but the fn is **never called** — and **content-absent**: 0 occurrences across Oblivion / FNV / Skyrim mesh archives (measured 2026-06-02 via `nif_stats --tsv`). The dead extractor is dead *because there is nothing to consume*, not a leak. A renderer projector pass would render a feature no shipped content drives — **do not build speculatively**; revisit only if real content (modded / later titles) surfaces it | (none — content-absent) |
| `bs_lod_cutoffs` | `BSLODTriShape` | raw-parked on `ImportedMesh` — **this is the content-bearing in-cell LOD** (Skyrim ~43 meshes; mesh-level LOD0/1/2 triangle-count cutoffs). Foundation already present; only the runtime draw-count switch is deferred | in-cell LOD draw-count consumer (draw fewer indices by camera distance) |
| `lod_group` | `NiLODNode` → `NiRangeLODData` | **foundation done (2026-06-02):** `NiRangeLODData` now parsed (+ dispatcher + test) and surfaced as `ImportedNode.lod_group` (center + per-level near/far, Y-up). Import still walks child 0 only. BUT `NiLODNode` is **content-absent** in shipped archives (0 across Oblivion/FNV/Skyrim/FO4 — measured) — this is forward-compat (mods / other titles), not a shipped-content gap | per-frame distance-switch system (deferred — load-bearing walker change for perf-only gain, see below) |
| `bs_sub_index` | `BSSubIndexTriShape` | raw-parked | dismemberment / locational-damage system |
| furniture marker | `BSFurnitureMarker` | **consumed** since #2010 / M41.5 Phase B — walked into `ImportedFurnitureMarker` (`extract_furniture_markers`), lifted to the `Furniture`/`FurnitureMarker` ECS components (`furniture_component`, `cell_loader/references/attach.rs`), and read by the sandbox sit/lean/sleep system (`byroredux/src/systems/sandbox.rs`) | (consumed — no longer blocked) |
| inv marker | `BSInvMarker` | parsed, not walked into `Imported*` | inventory-icon system |
| `NiSwitchNode` identity | `NiSwitchNode` | walked via **active-index** (furniture states, sheaths, destruction); the type discriminator is not surfaced. Content-present (Skyrim ~165, FO4 ~51) | geometry state-switching driver (gameplay) |
| `bs_bound` | `BSBound` extra-data | consumed on the **loose-NIF** path only (`nif_loader.rs`), not the cell path | a cell-path bound consumer (low value — the cell path already derives `WorldBound` from geometry) |

**In-cell LOD (2026-06-02, user-directed):** measured prevalence before building. `NiLODNode`
(node-level Z-depth LOD) is **content-absent** across all target games; the parser +
`lod_group` surfacing landed as forward-compat foundation + format coverage. The actual
content-bearing in-cell LOD is **`BSLODTriShape`** (mesh-level), whose foundation
(`bs_lod_cutoffs`) was **already parked** — so the foundation goal is met on both fronts.
The runtime switch (either node-child visibility or mesh draw-count) is deferred: it is a
load-bearing walk/draw change for a **perf-only** gain on modest content while the engine
runs with frame headroom — poor risk/reward until a perf need surfaces (same measure-first
verdict as `NiTextureEffect`).

When any consumer feature lands, its slice translates the already-captured field — no
parser change needed then.

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
- **Boundary pipeline**: all four sites below are part of the material contract.
  External-material enrichment runs only when that source exists; the remaining
  sites form the common lowering path rather than renderer post-processes:

  1. `byroredux/src/asset_provider/material.rs::merge_external_material` resolves
     BGSM/BGEM data into `ImportedMaterial` before canonical lowering.
  2. `byroredux/src/material_translate.rs::translate_material` is the sole lowering
     step. Its live signature takes
     `(&ImportedMaterial, mesh_name: Option<&str>, ResolvedPaths,
     extra_material_flags: u32) -> Material`. In particular, it cannot inspect an
     `ImportedMesh`; geometry-dependent material translation is excluded by the
     type boundary.
  3. `byroredux/src/cell_loader.rs::pack_imported_material_flags` is the shared
     feature-bit packer called inside `translate_material`; despite its module
     location, it runs for both cell and loose-NIF lowering. Placement-only bits
     arrive separately through `extra_material_flags`.
  4. `Material::resolve_pbr` runs inside the lowering step and guarantees finite,
     clamped canonical PBR scalars before the result reaches ECS consumers.
  5. `byroredux/src/material_translate.rs::resolve_normal_alpha_spec_roughness` and
     `::resolve_msn_z_source` finish the two texture-dependent fields once
     `MaterialTextureHandles` is attached — the second phase, below.

- **Two-phase boundary** (#2330). `translate_material` runs *before* texture
  handles exist, so any field whose value depends on which textures actually
  resolved cannot be finished there. Two are:

  | Phase | Site | Writes |
  |---|---|---|
  | 1 — base lowering | `translate_material` | the `Material` literal: scalars, colours, flags, glass classification, `resolve_pbr` clamping |
  | 2 — post-texture | `resolve_normal_alpha_spec_roughness` | `Material::roughness` for the normal-alpha-as-spec convention (#1480) |
  | 2 — post-texture | `resolve_msn_z_source` | `MAT_FLAG_MSN_HAS_AUTHORED_Z` for model-space normals (#2826) |

  Both Phase-2 resolvers run at **both** spawn sites, immediately after
  `MaterialTextureHandles` is attached. Both are idempotent and read only
  canonical components, so this is a staging constraint rather than a
  mutable-state leak — and it is why the render path carries no material
  heuristic of its own (#1480's "resolve once at spawn" contract).

  This is load-bearing for Skyrim: it ships no dedicated gloss map and puts its
  spec mask in the normal-map alpha, so most Skyrim architecture's shipped
  roughness comes from Phase 2, not Phase 1's literal. **Per-game material logic
  must account for both write sites** — a Phase-1-only change will not stick for
  a field Phase 2 also writes.

  The lowering step:

  1. copies the scalars / colours / flags across;
  2. packs `effect_shader_flags` as the union of the BSEffect SLSF bits, the BGSM
     v>2 bits, and the caller's extra bits (REFR-overlay model-space-normals on the
     cell path; `0` on the loose-NIF path);
  3. seeds `metalness`/`roughness` from the pre-resolved override (`Some`) or a `NaN`
     sentinel. For NIF-imported content the keyword classifier already ran at import
     (`classify_legacy_pbr` in `crates/nif/src/import/mesh/`), so `Some(…)` is always
     present and `Material::resolve_pbr()` only clamps — its classifier arm (the `NaN`
     sentinel path) is a backstop for future non-pre-classified sources. The result is
     the same either way: explicit scalars, no render-time fallback. (#1346 / D7-01)
  4. classifies glass once, alpha-aware (`helpers::classify_glass_into_material`),
     after the PBR resolve so the forced glass roughness wins.
- **De-duplication**: the two ~110-line `Material` construction sites
  (`cell_loader/spawn/mesh_instance.rs`, `scene/nif_loader.rs`) now both call
  the common lowering boundary. A field added in one place can no longer silently
  diverge the two load paths.
- **Renderer**: `render/static_meshes.rs` reads `m.metalness` / `m.roughness`
  directly — no per-draw keyword scan.

### Layering note

`translate_material` lives in the top `byroredux` crate (not `core` / `nif`)
because it folds in `classify_glass_into_material` (needs
`byroredux_renderer::MATERIAL_KIND_GLASS`) and consumes the spawn sites' resolved
common texture set (BGSM `material_path` → real textures, `StringPool`-resolved).
This is the expected shape: a category whose translation needs renderer constants
or asset-provider state translates in the top crate; a category whose translation
is self-contained (geometry, skinning) can translate inside `crates/nif`.

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

### Findings — **all three sources measured (2026-05-28), no normalization needed**

Sampled equivalent emissive meshes (neon/torches/lava/candles/glow cards/muzzle
flashes) across Oblivion + FNV + Skyrim SE + FO4:

| Source | Games measured | `emisM` observed | Exemplars |
|---|---|---|---|
| `Material` | Oblivion (BSVER 11), FNV (BSVER 34) | **0.5, 1.0, 1.3, 7.5** | neon signs, torches, lava |
| `Lighting` | Skyrim SE, FO4 | **0.9, 1.0, 1.0, 1.0** | imperial candle, ice torch, FO4 lantern |
| `Effect` | FO4 | **1.0, 1.2, 1.0** | fxglow card, minigun/flamejet muzzle flash |

**Conclusion: the three sources already share one ~1.0 scale — no per-source
normalization is required.** Every authoring source clusters its multiplier at 1.0;
the legacy `Material` 7.5 is an authored bright-neon *outlier*, not a scale-convention
difference (the same high values would appear in any source for deliberately bright
content). Applying a normalization constant would be inventing a correction for a
divergence that the ground truth shows does not exist (a `feedback_no_guessing`
violation in the other direction). The one genuine non-scale distinction —
`BSEffectShaderProperty.base_color_scale` is semantically a *diffuse-tint* multiplier,
not emissive — is already captured by the `EmissiveSource::Effect` discriminator and
is left for a future BSEffect-proper render path; it does **not** manifest as a scale
mismatch (Effect emisM 1.0–1.2 matches the others). Open question Q2 in
`material-abstraction.md` is hereby **resolved as no-op**.

---

## 5. Rollout order (later sessions)

1. ~~Materials~~ — done (this session).
2. ~~Nodes / passthroughs~~ — triaged (2026-05-28): the four unconsumed fields are
   formally recorded as raw-tier-parked with deferred translation (see the Nodes
   leak-inventory entry), each blocked on a not-yet-existing consumer feature.
3. ~~Particles (emitter base)~~ — done (2026-05-28): authored kinematic + lifetime
   params override the preset. Follow-ups: spawn rate, grow/fade size, multi-emitter
   attribution.
4. ~~Collision~~ — audited (2026-05-28): found + fixed two dropped shapes
   (`BhkMultiSphereShape`, `BhkConvexListShape`). The live coverage inventory is
   `import/collision/shape.rs::resolve_shape_inner`; remaining gaps (FO4+ NP blob,
   phantoms) are documented limitations.
5. ~~Emissive unification~~ — resolved no-op (2026-05-28): all three `EmissiveSource`
   variants measured across Oblivion/FNV/Skyrim/FO4 already share a ~1.0 scale (§4);
   no normalization needed.

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
