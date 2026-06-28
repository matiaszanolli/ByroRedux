# NIFAL Audit — Canonical Translation Layer — 2026-06-28

Deep audit of **NIFAL** (the NIF Abstraction Layer; spec: `docs/engine/nifal.md`).
Scope: the *translate* side — does each parsed NIF data category reach one
canonical, game-agnostic representation through a single explicit `translate()`
boundary, with no leak / fabrication / render-time fallback? (Parse-side
correctness belongs to `/audit-nif`.)

All 9 dimensions ran as concurrent Task agents against the live tree @ `830d90b5`.
Dedup baseline: `/tmp/audit/issues.json` (35 OPEN) + prior
`docs/audits/AUDIT_NIFAL_2026-06-23.md` (0 NEW), `AUDIT_NIFAL_2026-06-14.md`,
`AUDIT_NIFAL_2026-06-13.md`. Game data present for **all 7 titles** (Oblivion /
FO3 / FNV / Skyrim SE / FO4 / FO76 / Starfield) — the Dim 9 completeness harness
ran live against every one.

---

## Executive Summary

**Severity tally: 0 CRITICAL · 0 HIGH · 0 MEDIUM · 1 LOW (NEW). 0 regressions.**

Every per-category convergence claim in `nifal.md` §2 was re-verified against the
current code and holds. The four tier invariants — **single-boundary**,
**no-fabrication**, **no-leak**, **no-render-time-fallback** — pass on every
category. The five commits that landed on the NIFAL surface since the
2026-06-23 sweep were each verified clean:

| Commit | Dimension | Verdict |
|---|---|---|
| #1775 `05f3342b` — forward `NiPSysEmitter.radius_variation` as size jitter | Particles | **clean** — routes through the single `apply_emitter_params` sub-helper; jitter is the authored value × `base_scale` (no fabricated constant); lands in a plain-`f32` `start_size_variation`; both spawn paths get it; added to the finite sweep |
| #1771 `73592e33` — authored emitter rate of exactly 0.0 → preset-fallback | Particles | **clean** — in the shared `sane()` filter (`0.0 < r < 3.0e38`); defensible "0.0 = not-authored" interpretation, same sentinel discipline as #1363/#1364 |
| #1744 `39992b32` — don't apply `havok_scale` to `bhkNiTriStripsShape` | Collision | **clean** — genuine per-authoring-space rule (NiTriStripsData = render-mesh game units; packed/compressed = Havok units, which correctly keep the scale), grounded in `nif.xml`; not a moved leak |
| #1539 `db2c3a72` — surface dropped ragdoll constraints | Collision | **clean** — logging-only; decoded `Ragdoll`/`LimitedHinge` still flow to canonical `RagdollConstraintSpec`; no new `Option` leak |
| #1540 `db2c3a72` — fix trimesh bone inertia | Collision | **clean** — convex-hull-of-same-vertices substitution; no fabricated inertia constant (Rapier derives the tensor from geometry + existing mass override) |

The one NEW finding (**D6-01**, LOW) is a long-pre-existing parsed-then-dropped
field on `bhkPackedNiTriStripsShape` — surfaced now by the explicit shape-by-shape
scale diff that #1744 prompted. Default-identity in vanilla content; near-zero
real-world impact; reported for completeness as the canonical NIFAL leak class.

Convergence status vs spec §2 leak inventory:

| Category | Spec status | Verified 2026-06-28 |
|---|---|---|
| Materials | converged | ✅ confirmed (single boundary, 2 prod callers, plain-`f32` resolved PBR with `[0,1]`/`[0.04,1]` clamps, glass-once alpha-aware) |
| Geometry / transform | converged | ✅ confirmed (SVD-once at parse `stream.rs:674,:697`, Z-up→Y-up at import) |
| Skinning | converged | ✅ confirmed (#613 global bone indices, u16 guard, `global_skin_transform`, no partition re-derivation) |
| Lights | converged | ✅ confirmed (no renderer/spawn `match` on source block type; GPU discriminator runtime-derived) |
| Nodes | triaged | ✅ confirmed (all 7 parked fields = zero canonical consumers) |
| Particles | converged | ✅ confirmed (single overlay boundary; #1775 + #1771 clean) |
| Collision | audited | ⚠️ 16/16 parsed shapes resolve + 3 recent commits clean, **but 1 NEW LOW field-leak (D6-01)** |
| Animation / controllers | converged | ✅ confirmed (one `convert_nif_clip` boundary, 6 callers; #1725 refactor clean) |
| Shader flags / effects | converged | ✅ confirmed (zero `if game ==` in shaders incl. `#include`s; FO4 OR-in intact; 9/9 variants forward) |

The 2026-06-13 D7-NEW-01 (miscalibrated completeness floors) remains addressed:
the harness uses conservative documented thresholds (`>= 60.0` texture-path,
`>= 40.0` tangents) and passed green on all 7 games.

---

## Per-Category Tier Matrix

Boundary fn cited per category; `single-boundary` = N-A where the spec documents
the absence of one boundary as deliberate (Nodes).

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Boundary fn (verified site) |
|---|---|---|---|---|---|
| Material | PASS | PASS | PASS | PASS | `material_translate::translate_material` (`material_translate.rs:78`) — 2 prod callers (`scene/nif_loader.rs:796`, `cell_loader/spawn.rs:880`) |
| Geometry / Transform | PASS | PASS | PASS | PASS | `import/coord.rs` + `rotation::sanitize_rotation` (parse-time, `stream.rs:674,:697`) |
| Skinning | PASS | PASS | PASS | N-A | `import/mesh/skin.rs` (extraction-time #613 remap) |
| Lights | PASS | PASS | PASS | PASS | `import/walk/mod.rs:1141-1198` → `LightKind` |
| Nodes | N-A (by design) | PASS | PASS | PASS | none — two structural load paths (spec §2 Nodes) |
| Particles | PASS | PASS | PASS | N-A | `systems::particle::apply_emitter_overlays` (`systems/particle.rs:64`) — 2 prod callers |
| Collision | PASS | **PASS\*** | **PASS\*** | N-A | `import/collision.rs::resolve_shape` (`:504`) — 16 parsed `Bhk*Shape` : 16 resolve arms |
| Animation | PASS | PASS | PASS | PASS | `anim_convert::convert_nif_clip` (`anim_convert.rs:54`) — 6 callers, all consumers |
| Shader flags / Effects | PASS | PASS | PASS | PASS | `shader_flags.rs` (block-type dispatch) + `MaterialInfo` |

\* Collision passes at category level — every dispatched shape resolves and the
recent commits are clean — but D6-01 is a single low-severity field-leak inside an
otherwise-resolving shape (the shape's *geometry* translates; only its per-axis
`Scale` modifier is dropped). Not a category-level failure.

### Shape-arm count reconciliation (Dim 6 vs Dim 9)

The two collision-touching dimensions reported different shape counts: Dim 6 said
**16 parsed `Bhk*Shape` : 16 resolve arms**; Dim 9 said **17⇔17**. Re-verified
directly: anchored `pub struct Bhk[A-Za-z]+Shape\b` over
`crates/nif/src/blocks/collision/` returns **16 structs**, and
`downcast_ref::<Bhk*Shape>` in `resolve_shape_inner` returns **16 arms** — Dim 6's
count is exact. The "17th" is `BhkSimpleShapePhantom` (and `BhkAabbPhantom`), which
are **phantom/trigger objects, not shapes** (they end in `Phantom`, so the anchored
regex correctly excludes them); the phantom path resolves to a documented `None`
(`collision.rs:776`, trigger-volume limitation). The discrepancy is a counting
convention — both dimensions agree **every dispatched shape has a resolve arm**,
now self-enforced by the CI guard test `every_dispatched_bhk_shape_has_resolve_arm`
(`collision.rs:1783`), which parses the source and asserts `dispatched − resolved
== ∅`.

---

## Findings

### D6-01: `bhkPackedNiTriStripsShape` per-axis `Scale` field parsed-then-dropped at translate
- **Severity**: LOW
- **Dimension**: Collision
- **Tier Violated**: `no-leak` (parsed `Imported`-tier field never consumed at translate — the canonical NIFAL parsed-then-dropped leak class)
- **Game Affected**: FO3 / FNV / Skyrim+ (any game using packed-strips collision); manifests only on non-identity per-shape scale
- **Location**: `crates/nif/src/blocks/collision/shape_mesh.rs:55,93` (field stored) → `crates/nif/src/import/collision.rs:754-758` (dropped)
- **Status**: NEW (not in `/tmp/audit/issues.json`; not in prior NIFAL/NIF audits — the `AUDIT_NIF_2026-04-30` packed-scale finding is `BSPackedCombinedGeomDataExtra` terrain LOD, a different block)
- **Description**: `BhkPackedNiTriStripsShape` parses and **stores** its per-axis
  `Scale` Vector4 (`pub scale: [f32; 4]`, `shape_mesh.rs:55`; read at `:93`, with
  `_scale_copy` discarded at `:95`). The resolve arm at `collision.rs:754` calls
  `resolve_packed_mesh(data, scale)` passing only `scene.havok_scale` — the stored
  `s.scale` is never read in the resolve path (the only `.scale` reads in
  `collision.rs` at `:745-746` belong to `BhkMeshShape`, not the packed shape).
  `resolve_packed_mesh` (`collision.rs:871`) applies only the uniform `havok_scale`.
- **Evidence**: `reference/nifxml/nif.xml` `bhkPackedNiTriStripsShape` (line 3193)
  carries `Scale type="Vector4" default="#VEC4_1110#"` plus a `Scale Copy` ("Same as
  scale"). The struct field is live (not `_`-prefixed) yet has no consumer. Contrast
  `BhkMeshShape`, whose authored per-axis Scale IS folded in (`collision.rs:745-750`).
  Sibling-but-distinct: `BhkNiTriStripsShape.Scale` is read as `_scale` and discarded
  at `shape_mesh.rs:31` — same low-impact identity-default field, also pre-existing.
- **Impact**: Low. The packed scale defaults to `(1,1,1,0)` and vanilla Bethesda
  content authors it as identity virtually universally — the world scale is carried
  by `havok_scale`. Only a modded/custom packed-strips shape with a genuinely
  non-identity per-shape scale would render mis-sized collision. Pre-existing
  (predates #1744 — the `resolve_packed_mesh(data, scale)` call traces to
  `42aef192`/`75474e71`, not the recent commits).
- **Related**: Sibling drop `BhkNiTriStripsShape.Scale` (`shape_mesh.rs:31`); contrast
  the correct `BhkMeshShape` fold (`collision.rs:745-750`).
- **Suggested Fix**: Pass `s.scale` into `resolve_packed_mesh` the same way
  `BhkMeshShape` folds its Scale (with the same finite/non-zero guard that falls back
  to identity), or document the drop with a cited justification if packed scale is
  provably always identity in target content. Fold (or document) both
  `bhkPackedNiTriStripsShape` and the sibling `bhkNiTriStripsShape.Scale` together.

---

## Per-dimension verification notes (what was checked, why it passed)

**Dim 1 — Material (HIGH blast radius).** `translate_material`
(`material_translate.rs:78`) is the only `ImportedMesh → Material` site; exactly two
production callers (`nif_loader.rs:796`, `spawn.rs:880`), the cell path's only extra
being the REFR-overlay `MODEL_SPACE_NORMALS` `extra_material_flags`. Other `Material
{…}` literals are the `--cornell` scene + unit-test fixtures, not import paths.
`metalness`/`roughness` are plain `f32` (`material.rs:217,:223`); `resolve_pbr`
clamps `[0,1]`/`[0.04,1]` (`material.rs:655-656`) and fills only NaN sentinels —
never overwrites an authored BGSM/BGEM override (regression-guarded by
`resolve_pbr_preserves_upstream_translator_values`). Glass classified once, post-`resolve_pbr`,
alpha-aware (`helpers.rs:44`, invoked at `material_translate.rs:161`). Renderer reads
`m.metalness`/`m.roughness`/`m.material_kind` directly (`static_meshes.rs:309-310,:380`);
the render-side glass heuristic stays deleted (#1280 sub-step 3c). **Zero material-file
commits since the last audit** — byte-for-byte preserved.

**Dim 2 — Geometry / Transform.** `sanitize_rotation` fires only at parse
(`stream.rs:674,:697`); `compose_transforms` + `zup_matrix_to_yup_quat` assume valid
rotations, never re-check. All three per-game decoders converge to
`positions: Vec<[f32;3]>` + `indices: Vec<u32>` in Y-up (`ni_tri_shape.rs:126`,
`bs_tri_shape.rs:50`, `bs_geometry.rs:85`); `MeshRegistry::upload` does no coordinate
handling. `local_bound_radius` derived in Y-up at extraction, read directly by
consumers. The sole inline axis-swap leaks (`tangent.rs:92,:254`, normal used only for
bitangent-sign) are the **already-OPEN #1753 / TD2-005** — not re-reported.

**Dim 3 — Skinning / Lights.** `skin.rs` carries the #613 partition-local→global
remap (`remap_bs_tri_shape_bone_indices:285-366`), the `bone_refs_slice.len() >
u16::MAX` guard (`:147-153`), and `global_skin_transform` on every extractor. GPU
consumer (`nif_loader.rs:611-629`) reads already-global `[u16;4]`; full
`byroredux/`+`crates/renderer/` sweep for `NiSkinPartition`/partition-palette access =
zero hits outside `skin.rs`. Light block downcasts are confined to the raw/extraction
tier (`walk/mod.rs:1141-1198` = the `translate()` boundary producing `LightKind` +
derived `radius`; `anim/entry.rs:166-177` = controller-target resolution). No
renderer/spawn `match` on source block type; `GpuLight` discriminator is
runtime-derived from `CellLightingRes`, not the NIF block.

**Dim 4 — Nodes.** All seven raw-tier-parked fields (`bs_value_node`,
`bs_ordered_node`, `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs`,
`bs_sub_index`) have **zero canonical ECS consumers** — every non-test hit is
producer-side (walk/mesh extractor initializers, `cell_loader.rs`/`asset_provider.rs`
`None`-seeds, SpeedTree placeholder producer); the only `.field` reads are 4
assertions inside `spt/src/import/mod.rs` `#[cfg(test)]`. Live canonical node data
(`name`, `flags`→`SceneFlags`, `collision`→`CollisionShape`/`RigidBodyData`,
`billboard_mode`→`Billboard`) is consumed at both spawn sites. `parked-not-leak`
confirmed.

**Dim 5 — Particles.** `apply_emitter_overlays` (`systems/particle.rs:64`) is the
single overlay site; 2 callers (`nif_loader.rs:526`, `spawn.rs:436`). A grep for every
canonical `ParticleEmitter` field-write outside the boundary returns only the
in-`apply_emitter_params`/`apply_emitter_overlays` assignments. **#1775** size-jitter
routes through the single sub-helper into plain-`f32` `start_size_variation`
(`particle.rs:256`), authored value × `base_scale`, consumed once at spawn
(`particle.rs:427`), added to the finite sweep (`walk/mod.rs:730`). **#1771**
zero-rate fallback lives in the shared `sane()` filter. `initial_color` still
deliberately unapplied; FLT_MAX sentinel rejected; force fields Z-up→Y-up at overlay.

**Dim 6 — Collision (HIGH blast radius).** 16 parsed `Bhk*Shape` : 16 resolve arms
(self-enforced by `every_dispatched_bhk_shape_has_resolve_arm`); `BhkPlaneShape`
returns documented `None` (#1334). `havok_motion_type` (`collision.rs:145-153`) is
bit-exact to `nif.xml`'s `hkMotionType` enum (`1..=5|8 → Dynamic`, `6 → Keyframed`,
`7 → Static`, `9 → CharacterKinematic`, else `Static`); no consumer reads the raw byte
(all `.motion_type` sites read the canonical enum). Havok→engine transform routes
through canonical `zup_to_yup_pos` (#1617); no consumer re-applies `havok_scale`. The
three recent commits (#1744/#1539/#1540) verified clean (see Executive Summary). **One
NEW LOW finding: D6-01.**

**Dim 7 — Animation.** `convert_nif_clip` (`anim_convert.rs:54`) is the single
NIF→`AnimationClip` boundary; its 6 callers are all consumers (correct). B-spline
(dispatched by *block type*, not era — correct for FO3/FNV per
`feedback_bspline_not_skyrim_only`), XYZ-Euler→quat, TBC/Hermite, Z-up→Y-up all
resolved at import. Player/stack/interpolation consumers carry **zero** era/`Option`
key branches (uniform `Const|Linear|Quadratic|Tbc` arms). Text-keys wired end-to-end;
embedded controllers' `text_keys: Vec::new()` is deliberate. Parked channels (per-light
ambient `let _ = value` at `animation.rs:194`; morph-weight lands in
`AnimatedMorphWeights` but no renderer reads it) confirmed parked. The one
animation-touching commit since the last audit — `9488eeb0 Fix #1725` — is a per-frame
scratch-buffer hoist, no boundary/key/era change.

**Dim 8 — Shader flags / Effects.** `grep -rniE
'fnv|fallout|skyrim|oblivion|fo3|fo4|starfield|game =='` over
`crates/renderer/shaders/` → **63 hits, every one a `//` comment; zero runtime
per-game branches**. The only era-flavoured runtime conditionals are data-driven flag
checks (`dalcFlags.x == 1.0`), never a game-id branch. FO4 `Model_Space_Normals`
(F4SF1 bit 12) + `Alpha_Test` (F4SF2 bit 25) + FO76 `MODELSPACENORMALS` CRC reach
`MaterialInfo` (`walker.rs:320-343`, #1592; range shifted ~10 lines from the prior
audit's 310-338 by added comments — logic intact). All 9 `ShaderTypeData` variants
forward their trailing payload across three BSVER-dispatched parsers. Compile-time
bit-26/27 + bit-21 equivalence asserts present.

**Dim 9 — Completeness + cross-cutting.** Harness ran green across all 7 games
(`cargo test ... --ignored` → `test result: ok. 1 passed`); all per-game fill-rate
floors passed; `consistent% = 100.0` everywhere (hard structural invariant). The three
large per-game divergences were each chased into the extractor and proven
content-absent or by-design, not a leak (see below). Zero `if (game ==` render-time
branches in `triangle.frag` + includes. Every category that needs a boundary declares
exactly one.

### Completeness harness output (Dim 9, live run, 7 games)

```
  game          imported   tex%   mat_path%  m_kind%  metO%  rghO%  nrm%   tan%   consistent%
  Oblivion      2117      99.9%    0.0%       0.0%   100.0% 100.0%  0.0%  99.9%   100.0%
  FO3            596      98.7%    0.0%       5.0%   100.0% 100.0% 94.1% 100.0%   100.0%
  FNV            629      95.1%    0.0%       8.1%   100.0% 100.0% 89.2%  97.3%   100.0%
  SkyrimSE        97     100.0%    0.0%      60.8%   100.0% 100.0% 84.5% 100.0%   100.0%
  FO4            269     100.0%   77.0%      30.5%   100.0% 100.0% 90.7% 100.0%   100.0%
  FO76           293       9.6%   90.4%       9.6%   100.0% 100.0%  5.5% 100.0%   100.0%
  Starfield      176       0.0%  100.0%       0.0%   100.0% 100.0%  0.0% 100.0%   100.0%
```

Leads chased (large per-game divergence → verified content-absent / by-design, not a leak):
- **`nrm%` Oblivion 0.0%** — content-absent: 200/200 sampled NIFs carry
  `NiTexturingProperty` but **0** have a bump/normal slot; the extractor
  (`walker.rs:657-662`) reads both slots correctly. Era fact (Oblivion static meshes
  ship no tangent-space normal maps).
- **`nrm%` FO76 5.5% / Starfield 0.0%** — content-absent *at the measured boundary*:
  the normal map lives in `.bgsm`/CDB, populated downstream by `merge_bgsm_into_mesh`
  (`asset_provider/material.rs:738`) / CDB resolution — a layer the pure-NIF harness
  deliberately does not drive. `material_path%` carries the identity (90.4% / 100.0%).
- **`m_kind%` Oblivion/Starfield 0%, FNV 8.1%** — by design: FNV
  `BSShaderPPLightingProperty` never sets `material_kind`; only the Skyrim+
  `BSLightingShaderProperty` arm does.

**Informational note (not a finding):** the harness's `nrm%` (and any slot populated
only by the BGSM/CDB merge) is structurally blind on FO4/FO76/Starfield because
`collect_stats` drives `import_nif_with_resolver` (pure NIF) and stops before the
material merge. Consistent with the harness's own "What this does NOT catch"
docstring — not a translation defect. A BGSM-merge-aware second pass to make `nrm%` a
live modern-game regression signal is an *enhancement*, not a leak; not filed.

---

## Documented-limitation ledger (parked-not-leak — do NOT re-report next sweep)

Each is blocked on a consumer feature that does not exist, so translating now would
invent an ECS component nothing reads (`no-fabrication`). Spec: `nifal.md` §2.

- **Node/mesh passthroughs**: `bs_value_node`, `bs_ordered_node`, `tree_bones`,
  `range_kind`, `lod_group`, `bs_lod_cutoffs`, `bs_sub_index` — zero canonical
  consumers (re-verified per-field).
- **`NiTextureEffect`** — extractor dead because content-absent (0 occurrences,
  measured 2026-06-02). Do not build a projector pass speculatively.
- **`BhkNPCollisionObject`** (FO4/FO76/Starfield `BhkSystemBinary` blob) — decoder is
  a separate project; consumer falls back to `spawn.rs::synthesize_static_trimesh`.
- **`BhkPCollisionObject`** phantoms (Skyrim+ triggers) — need a `TriggerVolume` ECS
  path, not a rigid body; the `is::<…>` discriminators distinguish the two.
  `BhkSimpleShapePhantom` / `BhkAabbPhantom` resolve to documented `None`
  (`collision.rs:776`).
- **`BhkPlaneShape`** (#1334) — no half-space `CollisionShape` variant; explicit
  `None` → trimesh fallback.
- **`Other` ragdoll constraints** (bhkHinge / bhkBallAndSocket / bhkPrismatic /
  bhkStiffSpring) — dropped with a naming `log::warn!` (#1539); decoded
  `Ragdoll`/`LimitedHinge` flow to canonical `RagdollConstraintSpec`. Telemetry for the
  documented limitation (related open #1718), not a fix of the drop.
- **`BhkNiTriStripsShape.Scale` / `BhkPackedNiTriStripsShape.Scale`** — per-axis scale
  modifiers, default-identity, parsed-then-dropped (the latter is D6-01 NEW; the former
  the sibling). Fold or document together.
- **Particle size-over-life curve** — only authored *magnitude* translated
  (`start_size = end_size = initial_radius × base_scale`); the grow→fade bell shape
  needs a richer canonical size model. Full rate-curve sampling over time tracked #1402.
- **Emissive normalization** — resolved no-op (`nifal.md` §4); all three
  `EmissiveSource` variants measured at ~1.0 scale. Re-adding a normalization constant
  would be a `no-fabrication` violation in reverse.
- **Per-light ambient colour channels + morph-weight animation channels** — captured,
  no renderer consumer yet.
- **`NiLight.kind`** (LightKind discriminator) — NIF per-mesh lights spawn as a
  point-ish `LightSource` (`spawn.rs:320-360`); directionality not consumed. Consistent
  with the "renderer never inspects source block type" contract — parked, not a leak.
- **`base_color_scale`** (`BSEffectShaderProperty` diffuse-tint-vs-emissive render
  path) — tagged via `EmissiveSource::Effect`, deferred not dropped.

## Pre-existing OPEN issues touching this layer (Existing — not re-reported)

- **#1333** — modern `NiParticleSystem` local transform discarded → emitter ignores
  host-relative offset. Import-side; tracked.
- **#1659** (SKY-D3-03) — `BSDismemberSkinInstance` per-partition body-part flags
  parsed but discarded at import. Maps to the parked `bs_sub_index` / dismemberment gap.
- **#1711** (SPT-NEW-03) — OBND `bs_bound` discarded on the cell route (loose-NIF path
  consumes it). Adjacent parked-data gap.
- **#1718** — `Other` ragdoll constraint types not yet built (the #1539 warn telemetry
  surfaces these).
- **#1753** (TD2-005) — inline Z-up→Y-up axis-swap literals at `tangent.rs:92,:254`
  (normal-for-bitangent-sign). LOW/tech-debt; the canonical-coord-module hardening item.
- **#1580** (SF-D9-02) — BGEM `grayscale_to_palette_alpha` bool parsed but not
  forwarded. BGSM/BGEM material-domain, outside NIF shader-flag scope.
- **#1627** (TD5-002) — `GpuMaterial::glass()` transmission TODO names a closed issue;
  preset unused. Renderer-side tech-debt, not a NIFAL translate leak.
