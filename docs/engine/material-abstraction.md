# Canonical Material Abstraction — Design & Convergence Plan

**Status**: DESIGN / in progress (opened 2026-05-27)
**Goal**: every supported engine version (Oblivion / FO3 / FNV / Skyrim / FO4 / FO76 / Starfield) translates its native material data into **one canonical `Material`** with **one convention**, classified **at parse time**. The renderer/shader consumes that canonical material **identically** for all games. _All glass behaves the same. All cameras look the same._

This formalises the existing invariants in `feedback_format_translation.md` ("never per-game branches in the shader; translate at the parser→Material boundary") and `format_abstraction.md` (GameVariant trait pattern) — which are documented but **not yet fully realised**.

---

## 1. Why this exists (the observed symptom)

FNV / FO3 / FO4 / Skyrim RT-lighting looks like "different stages of development." The shader is **not** the cause — it branches only on game-agnostic `mat.materialFlags` / `mat.materialKind` / `dalcFlags.x` (verified: zero `if (game == …)` branches in `triangle.frag`). The cause is **upstream**: the canonical `Material` is fed by three different per-game translation paths that never converge to one convention.

## 2. The three leak sites (current state, 2026-05-27)

### Leak A — glass classified at RENDER time by a heuristic

[`byroredux/src/render/static_meshes.rs:372`](../../byroredux/src/render/static_meshes.rs#L372):

```rust
material_kind = GLASS  if  alpha_blend && !decal && metalness<0.3 && roughness<0.4
                            && path_indicates_glass    // English texture-name keywords
```

This depends on `roughness` (derived differently per game, see Leak B) and on `path_indicates_glass` (`glass`/`crystal`/`ice`/`gem`/`window`/`bottle`/`jar`/`vial`).

**Ground-truth failures** (via `material_dump` example):

| Game | Mesh | roughness | glass keyword in path? | classified glass? |
|---|---|---|---|---|
| FNV | `whiskeybottle01` (glass!) | 0.80 | yes (`bottle`) | **NO** — `0.80 ≥ 0.4` gate fails |
| FO4 | `drinkingglass01cleantall` (glass!) | 0.40 | no (`vase01`) | **NO** — gate fails by equality + no keyword |

So real glass in two different games both fail to render as glass, for two **different** reasons. The keyword list used by the render gate also disagrees with the keyword list inside the roughness classifier (Leak B).

### Leak B — metalness/roughness derived by three conventions, with an Option fallback

`ImportedMesh.metalness_override: Option<f32>` / `roughness_override: Option<f32>`:
- **`Some`** → authored (BGSM, FO4/Skyrim+), set by `merge_bgsm_into_mesh` (`roughness = 1 - bgsm.smoothness`).
- **`None`** → falls back to `classify_pbr_keyword` (texture-path keyword + `1 - glossiness/100`) **at render time**.

So:
- FNV/FO3/Oblivion → keyword heuristic (`classify_legacy_pbr` → `classify_pbr_keyword`).
- FO4/FO76 → BGSM authored PBR (only when the materials BA2 is loaded; otherwise silently falls to the keyword path — a second hidden divergence).
- Skyrim inline `BSLightingShaderProperty` → glossiness-derived.

A keyword-guessed `roughness 0.7` (FNV wood) and a BGSM-authored `roughness 0.55` (FO4 wood) feed the same PBR shader → visibly different surfaces.

### Leak C — ambient model diverges by data presence

`triangle.frag`: Skyrim cells with an authored `WTHR.DALC` 6-axis cube (`dalcFlags.x == 1.0`) use directional-cube ambient; FNV/FO3/Oblivion fall to flat XCLL ambient + `AMBIENT_AO_FLOOR`. Legitimate per-cell-data difference, but it means the ambient *look* isn't unified. (Lower priority than A/B — it's data-driven, not a heuristic.)

## 3. The canonical contract (target state)

The canonical `Material` (ECS component) carries, **always populated, one convention**:

- `albedo` (sampled texture × diffuse tint) — linear-after-decode, raw monitor-space per `feedback_color_space.md`.
- `metalness ∈ [0,1]`, `roughness ∈ [0,1]` — **no `Option`, no render-time fallback**. Every translation path resolves these at parse time.
- `emissive_color`, `emissive_mult` — one scale across games (see open question Q2).
- `material_kind` — the shader-dispatch enum, including `GLASS` and `EFFECT_SHADER`, **set at parse time**.
- normal / glow / detail / parallax / env maps — bindless handles, format-agnostic.

**Glass is decided once, at parse time**, per each game's authoritative signal — never a render-layer heuristic:
- FO4/FO76 BGSM: `alpha_blend && smoothness high && <transparency/glass material signal>`.
- Skyrim inline LSP: shader_type / alpha + low roughness.
- Legacy FNV/FO3/Oblivion: `NiAlphaProperty` blend + `NiMaterialProperty` alpha + glossiness — a deterministic rule, not English path keywords (keywords stay only as a last-resort tiebreaker, applied **at parse time** so the convention is uniform).

The renderer then reads `material_kind == GLASS` and the canonical `metalness/roughness/albedo/emissive` **identically for every game**. The `static_meshes.rs` glass heuristic is deleted.

## 3a. Ground-truth table (2026-05-27, via `mesh.info` PBR fields, materials BA2 loaded)

Equivalent surfaces, real engine pipeline (BGSM merge active for FO4):

| Surface | Game | source | `metalness_override` | `roughness_override` | glossiness | material_kind |
|---|---|---|---:|---:|---:|---:|
| metal (door) | FO4 | `institutemetal01.bgsm` | **0.79** | **0.04** | 100 | 1 |
| metal (panel) | FO4 | `institutemetal01a.bgsm` | 0.79 | 0.04 | 100 | 1 |
| floor | FO4 | `institutefloor02d.bgsm` | 0.69 | 0.10 | 90 | 1 |
| wall | FNV | keyword | **0.00** | **0.80** | 10 | 0 |
| glass bottle | FNV | keyword | **0.00** | **0.80** | 50 | 0 |

**The smoking gun**: FNV's keyword classifier collapses *every* surface to
`metalness 0.00 / roughness 0.80` — metal renders as matte plastic, glass as
rough plastic. FO4's BGSM gives real per-material PBR (metal 0.79/0.04). The
same shader fed these two conventions produces the "different dev stages" look.

**Refined root cause** (sharpens Leak B): the legacy `classify_pbr_keyword`
path is *degenerate*, not just "different" — it has no metalness signal at all
(always 0.0) and clusters roughness near 0.8.

A second `material_dump` sweep (env / specular columns added) over 9 FNV
clutter + architecture meshes found **why** the roughness collapses: every
surface reads `env_map_scale = 1.00` — the *neutral `BSShaderPPLighting`
default*, not an authored per-surface value — and `specular_color` is black
(`spec_lum = 0.00`), `specular_strength = 1.00` constant. So:

- **`env_map_scale` is noise** in FNV (constant 1.0). The old `> 0.3` env arm
  caught the default and clamped *every* non-keyword surface to the `0.8`
  ceiling (`1 - 1.0·0.2`), preempting the one real signal.
- **`specular_color` is NOT a usable metalness signal** for FNV legacy content
  (uniformly black) — contrary to the "high-specular ⇒ metal-like" hypothesis
  above. Metalness for legacy content has no authored source beyond the
  texture-path keyword arms; that is a documented limitation, not a guess we
  may invent (`feedback_no_guessing`). Real metal (e.g. `tincan01`) that lacks
  a metal keyword stays dielectric — accepted degraded mode (Q3-adjacent).
- **`glossiness` is the only real per-surface signal** (a 10/30/50/60 gradient
  on those meshes — the authored Phong specular power). The fix lets it drive
  roughness.

**Step 2 experiment — RAISED then REVERTED.** The first attempt raised the env
arm gate to `> 1.0` so the neutral baseline fell through to the
authored-glossiness arm, restoring the gradient (whiskey bottle `0.80 → 0.40`,
etc.). **That regressed non-glass surfaces into chrome**: FNV authors glossiness
60–90 on ordinary cloth / weathered metal, and `1 - gloss/100` maps gloss-60 to
roughness ≈ 0.30. At `< 0.6` roughness the RT reflection path engages, so
weathered Chairman suits at the Tops rendered as mirror chrome ("chrome thugs").
The env=1.0 → 0.8 matte clamp was load-bearing for non-glass content.

**Reverted to `> 0.3`** (matte 0.8 default restored). Crucially this does NOT
re-break glass: glass smoothness is owned by step 3's spawn classifier
(`classify_glass_into_material` forces `roughness_override = 0.10`), which is
independent of this arm. So glass stays glassy *and* ordinary surfaces stay
matte. Lesson: the glossiness *gradient* is not a usable roughness signal for
non-glass FNV surfaces (it over-shines); only the glass path needs low
roughness, and it gets it explicitly. Pinned by
`classify_pbr_neutral_envmap_default_clamps_matte_not_chrome`.

## 4. Convergence plan (incremental, test-gated)

1. **Ground-truth audit** *(in progress)* — `material_dump` example tabulates `material_kind / metO / rghO / glossiness / emisM / alpha / decal / 2side` per mesh. Run on equivalent surfaces (glass / wood / metal / white-wall / emissive) across all 5 games **with the materials BA2 loaded** so BGSM values are visible. Builds the convention-mapping table.
2. **Canonical PBR at parse** *(env-arm experiment reverted)* — `resolve_classifier_overrides` collapses the `Option`s at material-insert time. The env-arm `> 1.0` experiment was REVERTED (chrome regression, above): the matte 0.8 default for neutral-env surfaces is correct because the glossiness gradient over-shines non-glass content. Glass smoothness is owned by step 3, not this arm. Pinned by `classify_pbr_neutral_envmap_default_clamps_matte_not_chrome`. Metalness for legacy content stays keyword-only (no authored source — documented above).
3. **Parse-time glass** *(alpha-aware classification at material-insert; legacy
   path DONE)* — glass is now decided **once, alpha-aware, at spawn**
   (`helpers::classify_glass_into_material`, called from both `cell_loader::spawn`
   and `scene::nif_loader` right after `resolve_classifier_overrides`). Rule:
   `material_kind < 100 && has_alpha && !is_decal && metalness < 0.3 &&
   (glass_keyword(texture) || glass_keyword(name))` → set
   `material_kind = MATERIAL_KIND_GLASS` and force `roughness_override = 0.10`
   (roughness is a *consequence* of glass, not a gate). The render code's
   existing `material_kind >= 100` branch preserves it, so the surface clears
   both the CPU `< 0.4` and shader `< 0.35` glass roughness gates and renders
   through IOR refraction — **no render-heuristic or shader change**.

   **Two-tier keyword contract** (the design subtlety the ground-truth forced):
   - The alpha-UNAWARE roughness classifier (`classify_pbr_keyword`) keeps only
     "glass *material*" tokens `glass/crystal/ice/gem` → unconditional smooth
     0.1 (those textures ARE glass: `glasspitcher`, `brokenglasssheet`).
   - The wide `is_glass_keyword_path` (+ `window/bottle/jar/vial`) is used ONLY
     by the alpha-gated sites (spawn classifier + render gate). A container
     token alone never earns smooth roughness in the alpha-unaware classifier
     — this **reverts the step-3-initial over-shine** where an opaque
     `windowframe` / `bottlecap` texture went shiny.
   - The **mesh-name** source catches texture-less glass: FNV `ShotGlass` /
     `DrinkingGlass` share the atlas `kitchenutensils01.dds` (no keyword) but
     their NIF node name has "glass". The **alpha gate** keeps an opaque
     `PawnShopWindow` (name "window", no blend) OUT (verified by ground-truth).

   Pinned by `glass_material_tokens_are_unconditionally_smooth`,
   `glass_container_tokens_match_render_gate_but_not_classifier_arm` (core),
   and the `helpers::glass_classification_tests` suite (byroredux).
   **Still pending**: (a) the FO4 BGSM glass material flag (a BGSM glass bottle
   with no keyword in texture/name won't classify — needs the BGSM
   transparency signal plumbed); (b) deleting the now-subsumed render-side
   glass heuristic in `static_meshes.rs` (spawn is a superset of it; left as a
   defensive fallback for now).
4. **Emissive scale unification** — reconcile `emissive_mult` scale across games (Q2).
5. **Ambient** *(optional, lower priority)* — consider a synthesized DALC-equivalent for non-Skyrim cells so the ambient model is uniform, or accept the data-driven difference.

Each step ships independently with `cargo test` coverage; no step touches the Vulkan render-pass / pipeline (the shader already consumes canonical flags).

## 5. Open questions

- **Q1** — Glass authoritative signal for legacy: is `NiAlphaProperty.blend + low NiMaterialProperty.alpha` sufficient, or do we still need a (parse-time) keyword tiebreaker? Needs the ground-truth table.
- **Q2** — `emissive_mult` scale: FO4 BSEffect `base_color_scale`, legacy `NiMaterialProperty.emissive_mult`, and Skyrim LSP `emissive_multiple` may not share a scale. Tabulate before unifying.
- **Q3** — Does BGSM-less FO4 loading (materials BA2 absent) need an explicit "PBR unavailable" path, or is the keyword fallback acceptable as a documented degraded mode?

## 6. Tooling added for this work

- `crates/nif/examples/material_dump.rs` — per-mesh canonical-material dump.
- `crates/nif/examples/dump_nolighting.rs` / `dump_alpha.rs` — shader-property + alpha-blend inspection.
- `crates/bsa/examples/ba2_grep.rs` / `ba2_extract_one.rs` — BA2 path search + single-file extract.
