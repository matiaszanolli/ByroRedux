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
(always 0.0) and clusters roughness near 0.8. Legacy NIFs DO carry a real
signal we're ignoring: `NiMaterialProperty.specular_color` /
`specular_strength` / `glossiness` (the Gamebryo Phong model). A
high-specular + high-gloss surface is metal-like; that's the principled
translation source (per `feedback_no_guessing` — derive from authored data,
not invent). Convergence step 2 must replace the keyword guess with a
specular/gloss-derived PBR mapping grounded in the Gamebryo 2.3 material model.

## 4. Convergence plan (incremental, test-gated)

1. **Ground-truth audit** *(in progress)* — `material_dump` example tabulates `material_kind / metO / rghO / glossiness / emisM / alpha / decal / 2side` per mesh. Run on equivalent surfaces (glass / wood / metal / white-wall / emissive) across all 5 games **with the materials BA2 loaded** so BGSM values are visible. Builds the convention-mapping table.
2. **Canonical PBR at parse** — make `classify_pbr` resolve `metalness`/`roughness` to concrete values for every path; collapse the `Option` overrides into always-populated canonical fields. Pin per-path with tests.
3. **Parse-time glass** — move glass classification into the parser; set `material_kind = GLASS` per the per-game authoritative rule. Delete the render-layer heuristic. Regression-pin the two known failures (FNV whiskey bottle, FO4 drinking glass) as "now classified glass."
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
