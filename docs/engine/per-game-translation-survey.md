# Per-Game Translation Survey — Where the Abstraction Layer Has to Land

**Status**: SURVEY — generated 2026-05-28 from four parallel scans (NIF parser /
NIF importer / ESM + cell-loader / renderer); hand-corrected 2026-08-25 (#3281 —
two stale passages fixed, see notes inline at §2 and §4.3). Child of
[`nif-engine-translation-layer.md`](./nif-engine-translation-layer.md) (issue
[#1277](https://github.com/matiaszanolli/ByroRedux/issues/1277)).

> Historical survey, retained as the evidence snapshot for #1277. Several
> findings below have since shipped; use the current compatibility tables in
> [`ROADMAP.md`](../../ROADMAP.md) and [`feature-matrix.md`](../feature-matrix.md)
> for present support claims. The `~70+ per-game branches` headline count
> below is **unverified against the current tree** (not re-counted by the
> 2026-08-25 correction pass) and should be treated as unreliable until the
> document is regenerated — some counted classes remain accurate, so treat
> this survey as partially, not wholesale, stale.

**TL;DR**: The renderer is genuinely clean — zero `if (game == …)` branches in
shaders or in renderer Rust. The invariant from `feedback_format_translation.md`
holds at the renderer boundary. **Where the abstraction layer is incomplete is
upstream**: the parser/importer/cell-loader carry **~70+ per-game branches** —
some scattered as hardcoded BSVER constants where named helper methods already
exist but call sites bypass them, others as outright gaps (FO4 collision silently
dropped, FO4-only records with no game guards).

**Why Fallout is worse than Skyrim** — section 7 below — is a structural
consequence: Fallout spans the widest BSVER range (24 → 155 = FO3 → FO76), it
introduced format-incompatible changes at every major version, and several
Fallout-only paths (BGSM materials, bhkNPCollisionObject, CRC32 shader flags,
half-float verts, inline tangents) silently fall back to wrong defaults instead
of being routed through a clean per-variant strategy.

---

## 1. The good news

The renderer audit found **zero** per-game branches in:

- `triangle.frag` (3,400+ lines) — material-kind branches use universal constants
  (`MATERIAL_KIND_GLASS = 100`, `MATERIAL_KIND_EFFECT_SHADER = 101`); no
  `if (bsver)` / `if (game)` patterns; the DALC ambient-cube gate at line 3264 is
  data-driven (`dalcFlags.x > 0.5` per-cell-data), not a game proxy.
- `composite.frag` — cubic-fog gate `if (fog_clip > 0.0 && fog_power > 0.0)`
  is per-cell XCLL data, not a game detector; the aerial-perspective fallback is
  exterior-cell-gated, not game-gated.
- `crates/renderer/src/` — zero `GameKind::` / `GameVariant::` / `bsver` checks
  in the renderer Rust.
- `byroredux/src/render/` — GpuInstance / GpuMaterial flag packing is
  per-entity, format-agnostic. Render-time draw enumeration has no game knowledge.

The translation-layer invariant **holds at the renderer boundary today**. Any
divergence the user sees is upstream.

## 2. The bad news — and the real "Fallout looks broken" cause

**This section's original worked example is stale — see the note below.** The
renderer is fed values from a translation layer that historically **didn't
normalise per game**: `classify_pbr_keyword` used to collapse every
non-keyword, non-glass surface to the matte default regardless of its
authored specular color, which chromed or flattened surfaces depending on
which per-game shader property happened to feed the classifier. That specific
bug was fixed by #1873 (commit `634873db`, "gate PBR env-map metalness lift on
authored specular, not the struct default") — the classifier
(`crates/core/src/ecs/components/material.rs:663+`) now runs an evidence-cited
keyword + `specular_authored` gate rather than a blanket matte default. See
`material-abstraction.md`'s live examples for the current classification
behavior; the specific `metalness 0.79` / `metalness 0.00` numbers below are a
historical snapshot of the pre-fix bug, not a live claim about current output.

So this survey is the input map for the **canonical translation layer** that
`material-abstraction.md` is one axis of. The other axes are below.

## 3. Existing abstraction primitives + where they fail

Two enums already exist but **neither carries a feature-flag API and neither is
used consistently**:

| Enum | Crate | Method surface | Use pattern |
|---|---|---|---|
| `NifVariant` ([`crates/nif/src/version.rs:271`](../../crates/nif/src/version.rs#L271)) | `byroredux-nif` | `detect()`, `bsver()`, plus ~7 feature-flag helpers (`has_effects_list`, `has_properties_list`, `has_material_crc`, `has_shader_alpha_refs`, `uses_bs_tri_shape`, …) | **Helpers exist but call sites bypass them** — parser uses raw `stream.bsver() > 34` / `>= FALLOUT4` everywhere instead. |
| `GameKind` ([`crates/plugin/src/esm/reader.rs:85`](../../crates/plugin/src/esm/reader.rs#L85)) | `byroredux-plugin` | `from_header(variant, hedr_version)`, then plain `match` on the enum | Used **cleanly** in some record parsers (CLIMATE WLST, items WEAP/ARMO/AMMO), **completely absent** in others (SCOL/PKIN/MOVS/MSWP have no game guard, XCLL dispatches on byte length instead). |

There is **no trait-based per-game strategy pattern anywhere** (`grep` for
`trait .*Variant` returns nothing in `crates/`). All dispatch is plain `match`
on enum variants or raw BSVER comparisons.

The duplication itself is a problem: `NifVariant` knows about NIF BSVER bands,
`GameKind` knows about ESM HEDR bands, and consumers downstream need both —
there's no single authoritative "Game" type the engine queries.

## 4. Findings by layer — full inventory

Detailed file:line references for each finding live in the agent transcripts.
Below is the categorical roll-up.

### 4.1 NIF parser (`crates/nif/src/blocks/`, `version.rs`)

**Hardcoded threshold literals scattered across 30+ sites, most since named
as `version::bsver::*` constants (the doctrine — see §5 Pattern A, corrected
2026-08-30):**

| Threshold | Meaning | Sites |
|---|---|---|
| `bsver > 34` | FO3/FNV (≤34) vs Skyrim+ (>34) | 8+ across `base.rs`, `particle.rs`, `tri_shape/`, `extra_data.rs`, `collision/` |
| `bsver < FALLOUT4 (130)` | Pre-FO4 vs FO4+ | 12+ across `shader.rs`, `node.rs`, `light.rs`, `particle.rs`, `tri_shape/`, `texture.rs` |
| `bsver == 155` (strict) | FO76 only | 4+ in `particle.rs`, `tri_shape/bs_tri_shape.rs`, `shader.rs` |
| `bsver in [83..=139]` | Skyrim/FO4 legacy shader type | 1 in `shader.rs:786` |
| `bsver in [130..=139]` | FO4-only sub-block | 1 in `shader.rs::BSLightingShaderProperty::parse` |
| `bsver >= 132` | FO4_CRC_FLAGS — CRC32 shader-flag encoding | 3+ in `shader.rs` |
| `bsver >= 152` | FO76 SF2 CRCs | 1 in `shader.rs:400` |
| `bsver >= 173` | Starfield dev-build form_id field | 1 in `node.rs:798` |
| `bsver > 14` | FO3_REFRACTION trailing fields | 1 in `shader.rs:82` |
| `bsver > 24` | FO3_PARALLAX (strict `>` — FO3@24 must NOT carry) | 1 in `shader.rs:90` |
| `bsver <= 28` | Pre-anim-notes (Oblivion / early FO3) | 1 in `controller/sequence.rs:154` |
| `bsver < 9` | Pre-collision-v2 | 1 in `collision/collision_object.rs:67` |
| `bsver <= 34` (binary-extra-data tangent legacy) | FO3/FNV tangent blob layout | 1 in `extra_data.rs:1133` |
| `version <= V20_0_0_5` (Oblivion-only constraint payload) | Oblivion vs FO3+ | 2 in `collision/constraints.rs:61, 285` |

The removed "Helper available?" column named seven `NifVariant` feature-flag
predicates that do not exist in the tree — see §5 Pattern A's correction.
The real gap this table describes is scattered threshold *literals*, not
bypassed helper predicates; most sites above now compare against a named
`version::bsver::*` constant per the enforced doctrine rather than a bare
number.

**`BSLightingShaderProperty::parse` split into per-variant parsers, landed**
(`shader.rs:903 parse_skyrim`, `:1009 parse_fo4`, `:1159 parse_fo76_plus`,
plus `parse_shader_type_data_fo4` / `_fo76`) — the monolith this section
used to describe as "the textbook candidate for splitting" no longer exists
as a monolith.

### 4.2 NIF importer (`crates/nif/src/import/`)

**Per-game divergences that already produce different canonical values:**

1. **Shader-property family dispatch** — the walker checks for
   `BSLightingShaderProperty` (Skyrim+), `BSEffectShaderProperty` (Skyrim+),
   `BSShaderPPLightingProperty` (FO3/FNV), `BSShaderNoLightingProperty` (FO3/FNV),
   plus `NiMaterialProperty` (legacy cascade) — in sequential `if let Some(...) =
   scene.get_as::<>()` arms. Same canonical `Material` slot, but populated from
   different sources with different conventions per game.
2. **Texture-slot routing** ([`material/walker.rs:172-257`](../../crates/nif/src/import/material/walker.rs#L172))
   — `match shader.shader_type` for `BSLightingShaderType` (FaceTint=4 routes
   slot 4 to detail-map; MultiLayerParallax=11 routes slots 4/5/7 differently;
   default routes 4→env, 5→env_mask). FO3/FNV `BSShaderPPLightingProperty` has
   no equivalent routing — slot 4 is always env_map.
3. **Shader-flag bit collision across eras** — `flags2 bit 21` is `ALPHA_DECAL`
   on FO3/FNV, `Cloud_LOD` on Skyrim, `Anisotropic_Lighting` on FO4. Three
   different meanings on the same bit position. `is_decal_from_legacy_shader_flags`
   vs `is_decal_from_modern_shader_flags` routes around this — but the consumer
   doesn't know which path produced the result.
4. **FO76+ CRC32 shader-flag fallback** — when BSVER ≥ 132 the parser writes
   shader flags as `sf1_crcs` / `sf2_crcs` arrays and zeros the legacy u32
   fields. Two consumer helpers exist (`is_decal_from_modern_shader_flags`,
   `modern_effect_shader_bit`) but the "is this CRC-encoded or u32-encoded"
   distinction is invisible downstream of the boolean.
5. **FO76 SkinTint shader-type remap** ([`material/shader_data.rs:111-113`](../../crates/nif/src/import/material/shader_data.rs#L111))
   — FO76 numbers SkinTint=4 (`Color4`), legacy and the renderer's
   `materialKind == 5u` branch expect 5; the importer hardcodes a remap. Future
   variants will need similar remaps — currently no place to register them.
6. **Tangent extraction has 4 distinct paths** — `NiBinaryExtraData` Z-up blob
   (Oblivion/FO3/FNV), inline packed half-float (Skyrim SE BSTriShape), SSE
   skin-reconstruction Y-up (`sse_recon.rs`), UDEC3 Y-up (Starfield BSGeometry).
   Synthesis fallback has TWO variants (`synthesize_tangents` Z-up,
   `synthesize_tangents_yup` Y-up) and callers pick the right one based on
   knowing their input space — no enforcement.
7. **SSE-skin reconstruction is gated implicitly** ([`mesh/bs_tri_shape.rs:28-39`](../../crates/nif/src/import/mesh/bs_tri_shape.rs#L28))
   on `shape.vertices.is_empty()` — works for Skyrim SE, **comment explicitly
   notes "extending to FO4 requires copying the half-precision rule"**, but no
   FO4 path exists. So FO4 skinned content with `data_size == 0` silently fails.
8. **Material PBR classification diverges by source** — FNV/FO3/Oblivion =
   keyword heuristic (`classify_legacy_pbr` collapses everything to
   metalness=0/roughness=0.8); FO4 = BGSM external; Skyrim = inline LSP
   glossiness; FO76 = `.mat` JSON. Same canonical slot, four conventions —
   already documented as Leak B in `material-abstraction.md`.
9. **Emissive scalar conflation** ([`material/walker.rs:331-345`](../../crates/nif/src/import/material/walker.rs#L331))
   — `BSEffect.base_color_scale` is a diffuse-tint modulator but the importer
   routes it into `emissive_mult` alongside Skyrim LSP `emissive_multiple` and
   legacy `NiMaterialProperty.emissive_mult`. Three different semantics, one
   output field. Documented in code but unresolved.
10. **`bhkNPCollisionObject` silently dropped** ([`import/collision/mod.rs`](../../crates/nif/src/import/collision/mod.rs))
    — `extract_collision` calls `scene.get_as::<BhkCollisionObject>()` only.
    FO4+ uses `bhkNPCollisionObject` (Niagara Physics rewrite), the importer
    returns `None` for every FO4 architecture mesh, **player falls through
    every FO4 floor**. This is FALLOUT_SYMPTOMS F3's root cause — already
    worked around via render-geometry trimesh fallback (commit `15016ee0`) but
    the actual fix is a per-variant `extract_collision` strategy.
11. **`BsDismemberSkinInstance` (FO4+) silently skipped** if `NiSkinInstance`
    fails — currently in `sse_recon.rs::try_reconstruct_sse_geometry`, but
    bone-index remap is incomplete (partition-local indices aren't translated
    to global).
12. **Material-path format unknown to consumer** — `is_material_reference`
    captures `.bgsm`/`.bgem`/`.mat` uniformly; consumer can't tell BGSM
    (binary) from `.mat` (JSON). Format-specific parsing has nowhere to dispatch.
13. **Coord-space synthesis selection is by convention, not type** —
    `synthesize_tangents` vs `synthesize_tangents_yup` are picked by the
    caller "knowing" its input is Z-up or Y-up. A future caller getting it
    wrong silently produces incorrect tangents.

### 4.3 ESM + cell-loader (`crates/plugin/src/esm/`, `byroredux/src/cell_loader/`)

**Where `GameKind` is used well** (the model we want everywhere):

- `CLIMATE` WLST entry-size ([`records/climate.rs:68`](../../crates/plugin/src/esm/records/climate.rs#L68))
  — explicit `match game { Oblivion => 8, _ => 12 }`. Pre-#540 used
  size-autodetect which collapsed because 24 is a multiple of both 8 and 12.
- `WEAP`/`ARMO`/`AMMO` DATA dispatch ([`records/items.rs:143-449`](../../crates/plugin/src/esm/records/items.rs#L143))
  — explicit `match game` on each record's DATA/DNAM layout.
- `WTHR` Skyrim split ([`records/weather.rs:278`](../../crates/plugin/src/esm/records/weather.rs#L278))
  — `if matches!(game, GameKind::Skyrim) { return parse_wthr_skyrim(...); }`.
- `NPC_` Oblivion ATTR/DNAM/VNAM/PNAM/UNAM/XNAM ([`records/actor/mod.rs`](../../crates/plugin/src/esm/records/actor/mod.rs))
  — `is_oblivion` cached, used per sub-record.

**Where `GameKind` is completely absent (and should be present):**

- `XCLL` CELL lighting ([`cell/walkers.rs:255-325`](../../crates/plugin/src/esm/cell/walkers.rs#L255))
  — dispatches purely on byte length: ≥28 (shared), ≥40 (FNV tail), ≥92 (Skyrim
  tail). **A malformed FNV cell at 88 bytes silently parses as
  "Oblivion + partial FNV fields"**. No validation that the size matches the
  expected game.
- `LTMP` lighting-template sub-record ([`cell/walkers.rs:149`](../../crates/plugin/src/esm/cell/walkers.rs#L149))
  — parsed on every cell; only Skyrim ships it. A modder who adds LTMP to a
  FO3 cell gets it silently consumed.
- `XCMT` (Oblivion/FO3/FNV music enum) vs `XCCM` (Skyrim climate-override FormID)
  ([`cell/walkers.rs:216`](../../crates/plugin/src/esm/cell/walkers.rs#L216))
  — both parsed unconditionally on every cell. Mutually exclusive per game in
  practice but the schema allows both.
- `SCOL` ([`records/scol.rs`](../../crates/plugin/src/esm/records/scol.rs)) —
  FO4+ only by definition. **No game gate.** Cell loader expands SCOL on every
  game load (no-op on pre-FO4, but the code path runs).
- `PKIN` ([`records/pkin.rs`](../../crates/plugin/src/esm/records/pkin.rs)) —
  FO4+ only. **No game gate.**
- `MOVS` ([`records/movs.rs`](../../crates/plugin/src/esm/records/movs.rs)) —
  FO4+ only. **No game gate.**
- `MSWP` ([`records/mswp.rs`](../../crates/plugin/src/esm/records/mswp.rs)) —
  FO4+ only. **No game gate.**
- REFR DATA ([`cell/walkers.rs:460`](../../crates/plugin/src/esm/cell/walkers.rs#L460))
  — assumes uniform 24-byte position+rotation across all games. Oblivion
  trailing fields (if any) not validated.
- `RACE` DATA ([`records/actor/mod.rs`](../../crates/plugin/src/esm/records/actor/mod.rs))
  — **fixed since this survey was written.** A dedicated Skyrim arm
  (`records/actor/mod.rs:1225`, gated on `GameKind::Skyrim` + `len` of 128 or
  164) now exists alongside the Oblivion/FO3/FNV 36-byte arm, verified
  byte-for-byte against vanilla `Skyrim.esm` (2026-08-12).

**Already-fixed model finding** — `BSXFlags` bit 5 semantic flip
([`cell_loader/references/import.rs`](../../byroredux/src/cell_loader/references/import.rs))
is correctly BSVER-gated post-#560. This is the pattern: *one* bit-semantic flip
required a game-aware gate; the other 60+ bit semantics in BSXFlags / `flags2`
may have similar latent flips that haven't surfaced yet.

**HEDR collapse risk** — FO3 GOTY (0.94) and pre-GOTY (0.85) both route to
`GameKind::Fallout3NV`; the code comment acknowledges this is uniform-for-now
but a layout divergence in any sub-record would silently corrupt one or the
other. No empirical audit has been done to verify they're identical.

### 4.4 Renderer (`crates/renderer/`, `byroredux/src/render/`)

**Clean** — verified zero per-game branches. The "spawn-time leak sites" (glass
keyword classifier, `IsFxMesh` keyword scan, the BSVER editor-marker gate)
correctly live at the parser/spawn boundary, not in the renderer. The shader
reads `material_kind` / `MAT_FLAG_*` / `dalcFlags.x` / `fog_params.zw` —
all data-driven values that the upstream classifier is responsible for filling
consistently.

The renderer's cleanliness is **conditional on the upstream classifier
producing convention-uniform values per game**. Today it doesn't. That's the
canonical-material work plus the geometry-translation work plus everything in
sections 4.1–4.3.

## 5. Cross-cutting patterns

Three patterns repeat across every layer:

### Pattern A: hardcoded BSVER thresholds without a named constant

**CORRECTED 2026-08-30 (LC-2026-08-30-D6-01, #3795) — this section originally
prescribed the exact inverse of enforced doctrine.** It named seven
`NifVariant` feature-flag predicates (`has_effects_list`,
`has_properties_list`, `has_material_crc`, `has_shader_alpha_refs`,
`uses_bs_tri_shape`, `uses_fo4_shader_flags`, `uses_fo76_shader_flags`) as
"helpers" call sites should migrate to. Those predicates **do not exist**:
#938 removed three, #1511 removed six more, #1840 removed seven more
(including `has_material_crc`, `has_shader_alpha_refs`, `has_effects_list`,
`uses_bs_tri_shape`), and #1897 removed the last survivor. `version.rs:699-718`
records why — every one had zero production call sites, and "keeping a
call-site-less predicate as an approved helper alongside the raw-bsver path
was an architectural foot-gun": a contributor adopting one reintroduces the
one-bsver-step transitional-export mis-parse those call sites were fixed to
avoid. **No feature-flag predicates remain on `NifVariant` — this doctrine is
fully enforced.**

The actual doctrine the tree enforces is named **constants**, not named
**predicates**: a raw `stream.bsver()` compared against a named
`version::bsver::*` constant (e.g. `bsver::FALLOUT4`), with the nif.xml
`vercond` quoted at the call site (base.rs / node.rs, per #160 / #1331 /
#1838 / #1839). The gap this pattern actually describes is scattered
*threshold literals* that should be named constants, not bypassed helper
predicates — see §4.1's table, which the same correction pass re-titled.

~~**Highest-leverage starter** because it's a pure-refactor with zero behavior
change and locks the helper-bypass regression class out (a clippy lint or
custom test can enforce "no raw `bsver()` comparison outside `version.rs`").~~
**Retracted** — this would reintroduce the removed predicates. If a guard is
wanted here, it is the inverse: a test asserting `NifVariant` exposes **no**
feature-flag predicates, matching the doctrine `version.rs:699-718` declares
enforced.

### Pattern B: feature-flag-on-an-enum, not trait-per-variant

`NifVariant` is currently a flat enum with `bsver()` and `~7` feature flags
hung off the enum's impl block. As more feature flags accumulate (the survey
identifies ~25 new ones needed) the impl block becomes a directory. The
upgrade is a `GameVariant` trait with methods like:

```rust
pub trait GameVariant {
    // Format facts the parser needs.
    fn bsver(&self) -> u32;
    fn has_properties_list(&self) -> bool;
    fn uses_crc32_shader_flags(&self) -> bool;
    fn tangents_inline_in_vertex(&self) -> bool;
    fn bsxflag_bit5_is_multibound(&self) -> bool;
    fn xcll_tail_length(&self) -> usize;
    fn havok_scale(&self) -> f32;  // 7.0 pre-Skyrim, 69.99 Skyrim+
    fn material_format(&self) -> MaterialFormat; // None | Bgsm | Bgem | JsonMat

    // Per-variant strategy methods for the divergent extraction paths.
    fn extract_collision(&self, scene: &NifScene, ref: BlockRef)
        -> Option<(CollisionShape, RigidBodyData)>;
    fn extract_tangents(&self, /* … */) -> Vec<[f32; 4]>;
    fn classify_pbr(&self, props: &PropertyChain) -> PbrClassification;
}
```

This keeps the per-game logic in one place per variant and makes it impossible
for a consumer to "ask the wrong question" — the trait method signature is the
contract.

### Pattern C: variant-enum struct shapes for divergent records

Where a record's actual *fields* differ per game (not just a flag), the right
shape is a variant enum, not a flat struct with `Option` everywhere:

```rust
pub enum CellLighting {
    Oblivion { /* 28 B fields */ },
    Fnv { /* 40 B fields, including fog_clip/power */ },
    Skyrim { /* 92 B fields, including DALC cube, fog_far_color */ },
}

pub enum WeaponData {
    Oblivion { type_: u32, speed: f32, reach: f32, health: u32, /* … */ },
    Fallout3Nv { value: u32, health: u32, weight: f32, damage: u16, clip: u8, /* … */ },
    SkyrimPlus { value: u32, weight: f32, damage: u16 },
}

pub enum ShaderFlags {
    Legacy { sf1: u32, sf2: u32 },        // BSVER < 132
    Crc32 { sf1_crcs: Vec<u32>, sf2_crcs: Vec<u32> },  // BSVER ≥ 132
}
```

Consumers pattern-match instead of reading `Option<Field>` and guessing whether
the field is meaningful for their input game. Adding a new game variant means
adding one arm in one place, not scattering `Option` checks across the consumer.

## 6. Proposed abstraction-layer architecture

```
                       ┌─────────────────────────────────────┐
   NIF header  ────▶   │   GameVariant::detect_from_nif()    │
                       │   (NifVariant → impl GameVariant)   │
   ESM HEDR   ────▶    │   GameVariant::detect_from_esm()    │
                       │   (GameKind   → impl GameVariant)   │
                       └─────────────────────────────────────┘
                                       │
                          one trait object per scene
                                       │
       ┌───────────────────────────────┼───────────────────────────────┐
       ▼                               ▼                               ▼
   NIF parser                    NIF importer                      ESM parser
   (queries feature flags:       (queries strategy methods:        (queries strategy methods:
    has_*, uses_*, …)             extract_collision,                parse_cell_lighting,
                                  extract_tangents,                 parse_weapon_data,
                                  classify_pbr, …)                  parse_armor_data, …)
       │                               │                               │
       └───────────────┬───────────────┴───────────────┬───────────────┘
                       ▼                               ▼
              parsed scene blocks               canonical Material /
              (still NIF-shaped)                Transform / FogVolume /
                                                CollisionShape / NPCData
                                                       │
                                                       ▼
                                                ECS components
                                                       │
                                                       ▼
                                      Renderer reads canonical values
                                       (no game knowledge whatsoever)
```

**Step 1** (concrete starting point): unify `NifVariant` + `GameKind` behind one
`GameVariant` enum (or two structs implementing one trait). Today's call sites
keep working; new sites query the trait.

**Step 2**: migrate parser raw-BSVER comparisons to `variant.<helper>()` calls,
adding helpers as needed. Mechanical refactor, zero behavior change.

**Step 3**: implement the four critical strategy methods, in priority order:

1. `extract_collision` — fixes FO4 `bhkNPCollisionObject` gap (player falls
   through floors today; worked around with trimesh fallback).
2. `classify_pbr` — fixes the FNV "metalness 0/roughness 0.8 collapse" that
   makes Fallout look matte-plastic.
3. `extract_tangents` — unifies the 4 paths (legacy blob / SSE inline / SSE
   recon / Starfield UDEC3) and lets FO4 skinned content reconstruct.
4. `extract_shader_flags` — bridges the BSVER≥132 CRC32 encoding so consumers
   see a uniform `ShaderFlags` enum.

**Step 4**: convert the high-divergence record types to variant enums
(`CellLighting`, `WeaponData`, `ArmorData`, `AmmoData`, `ShaderFlags`).

**Step 5**: add game gates to FO4-only records (`SCOL`/`PKIN`/`MOVS`/`MSWP`) so
cross-game plugin stacks don't silently consume stale entries.

**Step 6**: add a translation-completeness audit dimension (Axis E from the
epic) that asserts equivalent surfaces across games produce
convention-identical canonical values. This is the regression guard for the
whole abstraction.

## 7. Why Fallout is worse than Skyrim — structural answer

The user's observation has a direct cause-list from this survey:

1. **Fallout spans the widest BSVER range** (FO3=24 → FO76=155). Skyrim sits in
   a narrow band (LE=83, SE=100). Every BSVER boundary the parser dispatches on
   bites the Fallout line; few bite Skyrim.
2. **Fallout introduced format-incompatible changes at every major version**:
   - FO4 (130): half-float verts, inline tangents, BGSM external materials,
     `BsDismemberSkinInstance`, `bhkNPCollisionObject` (Niagara Physics rewrite),
     `BSEffectShaderProperty` implicit alpha-blend, BSXFlags bit 5 re-purposed,
     SCOL/PKIN/MOVS/MSWP records, `BSConnectPoint`, `BSSubIndexTriShape`.
   - FO76 (155): CRC32 shader-flag encoding (replaces typed u32 fields),
     `BsTriShape` adds `bound_min_max` (24 B), `SkinTint=4` numbering change.
   - Starfield (172): `BSGeometry` (replaces `BSTriShape`), UDEC3 tangent
     packing, `.mat` JSON materials.
3. **Fallout-only records have no game gates** (SCOL/PKIN/MOVS/MSWP) — works by
   accident today because pre-FO4 ESMs don't carry them, but any cross-game
   plugin breaks the assumption.
4. **`bhkNPCollisionObject` silently dropped** — every FO4 cell has no static
   collision; the trimesh fallback works around this but isn't the real fix.
5. **FNV `BSShaderPPLightingProperty` has 3+ trailing BSVER sub-gates** packed
   into one parse method (BSVER>14 refraction, BSVER>24 parallax, BSVER>34
   emissive). One wrong threshold and FNV mesh imports break silently.
6. **`flags2 bit 21` triple collision** — FO3/FNV decal vs Skyrim cloud-LOD
   vs FO4 anisotropic. Three games, same bit, three meanings.
7. ~~**FNV `classify_pbr_keyword` collapses everything to matte 0.8 roughness**~~
   — **SUPERSEDED, see §2's correction note above.** This specific bug was
   fixed by #1873 (commit `634873db`): the classifier
   (`crates/core/src/ecs/components/material.rs:663+`) now runs an
   evidence-cited keyword + `specular_authored` gate rather than a blanket
   matte default. `material-abstraction.md`'s own "Leak B" passage carries the
   same corrective banner. This item stood unmarked for one correction pass
   after §2 was fixed (LC-2026-08-27-D6-01 / #3537); flagged here so it
   cannot be read as a live claim.

Skyrim is "easier" because its BSVER band is narrow, its property class is one
(`BSLightingShaderProperty`), its inline LSP carries usable PBR scalars (so the
PBR-collapse never fires), and BGSM is optional (mods only). The importer was
originally designed around Skyrim's format, so Skyrim is the path of least
resistance.

## 8. Concrete starter tasks (prioritised)

These are independently shippable, each closes a specific finding from above:

1. **`extract_collision` per-variant** — close FO4 `bhkNPCollisionObject` gap.
   Highest impact (player physics across every FO4 interior + exterior).
2. **Convert `BSLightingShaderProperty::parse` to variant dispatch** — split
   the 12-BSVER-comparison monolith into `Skyrim`, `Fo4`, `Fo76Plus` paths.
   Highest *complexity-reduction* win in the parser.
3. **Add `GameKind` gates to SCOL/PKIN/MOVS/MSWP** — close the FO4-only record
   silent-acceptance gap. Five-line fix each.
4. **`CellLighting` variant enum** — replace the 28/40/92-byte size-based
   dispatch with a typed variant. Closes the FNV-malformed-as-Oblivion class.
5. ~~**Migrate raw `bsver()` comparisons to `NifVariant` helpers**~~ —
   **REVERTED, see §9's task-5 row.** #1840 / #1897 subsequently removed
   every remaining `NifVariant` feature-flag predicate as an architectural
   foot-gun (`version.rs:699-718`). Do not re-propose this task.
6. **`ShaderFlags` variant enum** — bridge BSVER<132 (u32) vs BSVER≥132 (CRC32)
   under one consumer-visible type.
7. **`classify_pbr` per-variant strategy** — unifies the FNV keyword path / FO4
   BGSM / Skyrim LSP / FO76 `.mat` paths under one trait method. Largest
   user-visible win (fixes Fallout-looks-matte-plastic).
8. **Cross-game translation-completeness test harness** — Axis E from the epic.
   Loads the same "wood door" equivalent surface across FNV/FO3/FO4/Skyrim,
   asserts canonical `Material` values are within tolerance. Regression guard
   for the whole abstraction.

Tasks 1, 3, 4 are 1-day each. Tasks 2, 5, 6 are 2–3 days. Task 7 is the
existing canonical-material workstream (already in progress per
`material-abstraction.md`). Task 8 is the audit-infrastructure piece
(workstream E from the epic).

## 9. Progress (2026-05-28)

Six of eight starter tasks landed on `feat/nif-translation-layer-epic`:

| # | Task | Commit | Notes |
|---|---|---|---|
| 1 | `extract_collision` per-variant | `8d3a6861` | + `CollisionAuthoring` discriminator; FO4 NP path is a tracked stub; phantoms recognised. 6 dispatch tests. |
| 3 | `GameKind` gates on SCOL/PKIN/MOVS/MSWP | `dbabd880` | Single-point gate at `parse_esm` dispatcher; 8 tests including FO76/Skyrim positive+negative. |
| 4 | XCLL game-size sanity gate | `7ffda15d` | Plumbed `GameKind` into `parse_cell_group`; warns on non-canonical (game, size) pairs. 6 tests pin canonical sizes + survey's 88-byte FNV regression class. |
| 5 | ~~`bsver()` → `NifVariant` helpers~~ | `2bd447d5` | **REVERTED** (2026-08-30 correction, #3795). Landed as 6 sites migrated, 3 new helpers added — then #1840 removed 7 more feature-flag predicates and #1897 removed the last survivor. `version.rs:699-718`: every one had zero production call sites, and keeping a call-site-less predicate as an "approved helper" alongside the raw-bsver path was an architectural foot-gun. **No `NifVariant` feature-flag predicates remain; every parser queries `stream.bsver()` directly per the raw-bsver doctrine.** Do not re-migrate. |
| 6 | `ShaderFlags` typed variant view | `525a2262` | Three-variant enum (`LegacyFo3Fnv`/`LegacySkyrimFo4`/`Crc32`) with classify + `is_decal`/`is_two_sided`. Opt-in API; existing 35-call-site surface untouched. |
| 8 | Cross-game translation-completeness harness | `294e68f1` | `cargo test … --test translation_completeness -- --ignored`. Per-game `MaterialStats` over 200-NIF sample; structural-consistency hard gate. Live output shows the `m_kind%` ramp (0 → 5.6 → 9.6 → 27.4 → 38.3 from Oblivion to FO4) — material-abstraction.md Leak B made visible. |

**Task 2 (split `BSLightingShaderProperty::parse` into variant dispatch)** —
**LANDED** (correction 2026-08-30, #3795; this row previously read "deferred
to a dedicated session" long after the fact). `BSLightingShaderProperty`
now dispatches to `parse_skyrim` (`shader.rs:903`), `parse_fo4` (`:1009`),
and `parse_fo76_plus` (`:1159`), plus dedicated `parse_shader_type_data_fo4`
/ `_fo76` helpers — the 12+-BSVER-comparison monolith this row used to
describe as future work no longer exists in that shape.

Branch state: 7 commits on `feat/nif-translation-layer-epic` for the
task work, plus 4 earlier commits for the FogVolume + tooling +
epic-doc work. Not pushed.
