# NIFAL Audit — 2026-09-05

**Scope: NARROWED.** This is a `texture-roles-deep` preset run of `/audit-nifal`,
executing **Dimension 1 (Material boundary)** and **Dimension 8 (shader flags /
texture-role vocabulary)** only. Dimensions 2–7 and 9 were **not** executed —
geometry/transform, skinning/lights, nodes, particles, collision, animation and the
cross-cutting completeness harness are untouched by this report and their status
should be read from `docs/audits/AUDIT_NIFAL_2026-08-30.md`, not inferred from
silence here.

Weighting per the preset brief: the audit target is the 2026-07-27 cross-game
texture-role unification (`1d94eb24`, `05d68926`, `c8c8a834`) touching
`MaterialTextureSet`, `ImportedMaterial`, `merge_external_material` and
`translate_material`. Texture roles are the per-game seam, so a mistake there is
invisible on one game and wrong on another; attention was weighted accordingly.

Baseline for the delta: `docs/audits/AUDIT_NIFAL_2026-08-30.md`. All four of its
findings are **closed** — #3731 (`sanitize_finite` descent), #3732 (overlay
colocation), #3733 (ESM water planes), #3734 (`values()` omission guard) — and each
fix was re-verified in place rather than taken on trust. The reviewed code delta is
that report's HEAD to now, which includes #2573, #3073, #3637, #3639, #3796 and
`6236b130`/`d06f6df9`.

Executed in-process, no fan-out. Two corpus censuses were run against installed
vanilla game data (method and numbers inline below); everything else is static
analysis of the live tree, which is what the four tier invariants are decidable by.

## Executive Summary

**4 findings: 0 CRITICAL, 0 HIGH, 1 MEDIUM, 3 LOW.**

The headline is that the texture-role unification is holding well. All 26 role slots
(22 named + 4 decals) agree across five independent walks, every one of them has at
least one producer and every one of them reaches `GpuMaterial` and is read by
`crates/renderer/shaders/triangle.frag` — I checked index by index rather than by count. `smooth_spec` vs
`specular` and `environment` vs `environment_mask`, the two mis-merges the dimension
brief calls likeliest, have disjoint producer sets. The cardinal check — zero
per-game runtime branches in `crates/renderer/shaders/triangle.frag` and its `include/*.glsl` headers —
passes with nothing to qualify.

Two of the guard tests that used to be the weak points are now genuinely
self-maintaining rather than hand-maintained lists: `values_covers_every_field_in_the_set`
(#3734) cross-checks against `map_ref`'s compiler-forced visit count, and #3733
rewrote `every_exterior_spawner_inserts_a_boundary_material` from a file-name table
into a directory scan that fails on its own when a new spawner appears. Both are
better than what they replaced.

The one MEDIUM is a genuine `no-leak` violation that the role unification did not
reach, in the *animation* half of the texture pipeline rather than the material half
— which is presumably why it survived four prior sweeps of this dimension:
`TextureFlipEntry.texture_slot` carries a raw `TexType` slot number onto a canonical
ECS component, and its consumer re-resolves it. I measured the live blast radius
(one vanilla Oblivion mesh) rather than asserting it, so the rating rests on the
structural leak and on a mis-specified remediation comment, not on lost content.

Two of the three LOWs are the same shape as findings this project has already chosen
to fix structurally, in the one place each fix did not reach: a fifth hand-written
role walk with no lockstep guard, and a third prose copy of the role count that
#3465's parity test does not scan. The remaining LOW is a defence-in-depth gap in
today's #3639, whose boundary-side gate and shader-side gate test different things.

**Stale candidates dropped: 5** — listed at the end. Notably the entire
2026-08-30 finding set was re-verified as fixed rather than carried forward.

## Per-Category Tier Matrix

Only the two audited dimensions appear. A blank row is not a pass.

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Boundary / notes |
|---|---|---|---|---|---|
| Material — canonical (Dim 1) | PASS | PASS | PASS | PASS | `translate_material`, 3 production callers + Cornell; signature still narrowed to `&ImportedMaterial`; struct literal is exhaustive (no `..Default::default()`) |
| Material — no source record (Dim 1) | PASS | PASS | PASS | PASS | `translate_texture_only_material`; the #2444 guard is now a directory scan, not a list (#3733) |
| Material — external sidecar merge (Dim 1) | PASS | **partial** — D1-01 | PASS | PASS | `merge_external_material`, still narrowed to `&mut ImportedMaterial`; #3639's new fallback is path-gated where the consumer is handle-gated |
| Material — finiteness (Dim 1) | PASS | PASS | PASS | — | `Material::sanitize_finite` + the two #3731 descents; mechanically re-diffed, 35/35 float slots covered |
| Shader flags / per-game vocabularies (Dim 8) | PASS | PASS | PASS | PASS | dispatched by block type + `TextureSlotLayout::from_bsver`; zero `if game ==` in GLSL |
| Texture roles — material path (Dim 8) | PASS | PASS | **partial** — D8-02 | PASS | 26 roles, 5 walks; only `record_external_texture_sources` is unguarded |
| Texture roles — animation path (Dim 8) | — | PASS | **FAIL** — D8-01 | PASS | `TextureFlipEntry.texture_slot` is a raw `TexType` on a canonical component |
| Role-vocabulary documentation (Dim 8) | — | — | — | — | **FAIL** — D8-03; #3465's parity test covers 2 of 3 prose copies |

## Findings

### MEDIUM

#### NIFAL-2026-09-05-D8-01: `TextureFlipEntry.texture_slot` carries a raw `TexType` onto a canonical ECS component, and the recorded plan for resolving it names the wrong slot table
- **Severity**: MEDIUM
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: `no-leak`
- **Game Affected**: Oblivion (the only vanilla title that authors `NiFlipController` at all — measured, see Evidence); structurally all games
- **Location**: `crates/core/src/ecs/components/animated.rs:188-193` (the leaked field), `:218-224` (`handle_for_slot`), `byroredux/src/render/static_meshes.rs:263` (the consumer re-resolving it), `:133-139` (the mis-specified remediation comment), `byroredux/src/anim_convert.rs:241-244` (the producer)
- **Status**: NEW
- **Description**: Dimension 8's cardinal rule is that a per-game/per-format texture
  slot index must not survive past the NIF import boundary — `MaterialTextureSet`'s 22
  named roles exist precisely so it does not. The texture-role unification converted
  every *material* producer to those roles. It did not convert the *animation*
  producer.

  `TextureFlipEntry.texture_slot: u32` sits on `AnimatedTextureFlip`, a canonical ECS
  component in `crates/core`, and its own doc comment names what it is: *"Raw `TexType`
  slot from the source NIF (0=BASE_MAP, …)"*. The consumer cannot use it without
  re-resolving it, and today it does not resolve it at all — it hard-codes the raw
  number:
  ```rust
  // byroredux/src/render/static_meshes.rs:260-263
  let tex_handle = anim_texture_flip_q
      .as_ref()
      .and_then(|q| q.get(entity))
      .and_then(|f| f.handle_for_slot(0))
  ```
  Every `NiFlipController` targeting a slot other than 0 is therefore silently dropped:
  the flipbook animation never plays, with no warning and no `unrouted_*` counter of the
  kind `slot_to_role` grew for exactly this class of gap.

  The translation is not blocked on missing information. `TexType` is a closed
  12-value enum (`/mnt/data/src/reference/nifxml/nif.xml:383-397`) that maps onto `MaterialTextureSet` one-to-one with
  no per-game ambiguity — BASE→`base_color`, DARK→`dark`, DETAIL→`detail`,
  GLOSS→`smooth_spec`, GLOW→`emissive`, BUMP/NORMAL→`normal`, PARALLAX→`height`,
  DECAL_0..3→`decals[0..3]` — which is the same mapping
  `crates/nif/src/import/material/legacy_properties.rs` already performs for the static
  `NiTexturingProperty` texture set of the very same meshes.

  **The durable half of this finding is the recorded plan, not the current drop.** The
  deferral comment at `byroredux/src/render/static_meshes.rs:133-139` says a flip on another slot *"needs the
  same shader-type-aware `slot_to_role` dispatch `byroredux/src/cell_loader/spawn/mesh_instance.rs`
  uses for XTXR overrides"*. `slot_to_role`
  (`crates/nif/src/import/material/slot_role.rs`) is the **`BSShaderTextureSet`** table
  — a different, incompatible numbering (0 base, 1 normal, 2 glow-or-tint, 3
  height-or-detail-or-greyscale, 4 environment, 5 environment-mask, 6
  inner-layer-or-specular, 7 specular-or-back-lighting). An implementer who follows the
  comment maps `TexType` 1 (DARK_MAP) to `Normal`, 3 (GLOSS_MAP) to
  `Height`/`Detail`/`GreyscaleLut` depending on shader type, and 4 (GLOW_MAP) to
  `Environment`. That is a wrong-role binding written down as the plan, in the file an
  implementer would read first.
- **Evidence**: The leak is visible in the type itself — a `u32` documented as a raw
  source-format enum on a `crates/core` ECS component, consumed by a caller that must
  supply the raw number back (`handle_for_slot(slot: u32)`).

  Live blast radius, **measured** rather than assumed. Census run during this audit
  (temporary `crates/nif/examples` binary, since deleted; `BsaArchive::open` +
  `parse_nif`, downcasting every block to `NiFlipController` and histogramming
  `texture_slot`):

  | Archive | NIFs scanned | `NiFlipController` blocks | slot distribution |
  |---|---|---|---|
  | `Oblivion - Meshes.bsa` | 9,875 | 54 | 53 × BASE_MAP (0), **1 × GLOW_MAP (4)** — `meshes\creatures\endgame\battle.nif` |
  | `Fallout - Meshes.bsa` (FNV) | 19,197 | 0 | — |
  | `Fallout - Meshes.bsa` (FO3) | 13,729 | 0 | — |
  | `Skyrim - Meshes0/1.bsa` | 22,047 | 0 | — |

  So exactly **one** vanilla mesh loses its authored flipbook today. I want that stated
  plainly so the rating is not read as bigger than it is.
- **Impact**: One vanilla Oblivion creature mesh's glow flipbook does not animate, and
  any mod-authored non-base-slot flip is dropped the same silent way. The larger cost is
  structural: a raw source-format slot vocabulary is live on a canonical ECS component,
  which is the exact condition Dimension 8 exists to prevent, and the written plan for
  removing it points at a table with incompatible numbering — so the most likely future
  "fix" makes the engine bind wrong textures instead of no texture.
- **Related**: #2221 (created these sinks; CLOSED), #3251 (`handle_for_slot`'s
  out-of-range aliasing; CLOSED), #2695 (the precedent — one shared slot table, because
  two disagreeing tables changed shading *semantics*), #3814 (`supplemental_texture_indices`
  role pinning; CLOSED). No open issue covers this.
- **Suggested Fix**: Resolve `TexType` to a `MaterialTextureSet` role at
  `byroredux/src/anim_convert.rs`'s import-side hop — the same place the handles are already resolved
  — and store the canonical role on `TextureFlipEntry` instead of the raw `u32`. Then
  correct the `byroredux/src/render/static_meshes.rs:133-139` comment: the resolver for this vocabulary is
  the `NiTexturingProperty` mapping in `crates/nif/src/import/material/legacy_properties.rs`, **not** `slot_to_role`.
  Correcting the comment is worth doing even if the rest is deferred.

### LOW

#### NIFAL-2026-09-05-D1-01: #3639's neutral-roughness fallback is gated on an authored *path* while the shader escape it exists to restore is gated on a resolved *bindless index*
- **Severity**: LOW
- **Dimension**: Material
- **Tier Violated**: `no-fabrication` (defence-in-depth; the boundary decides from a
  weaker signal than the consumer acts on)
- **Game Affected**: FO4 / FO76 / Starfield (any BGSM-authored material)
- **Location**: `byroredux/src/asset_provider/material.rs:1748-1750` (the new gate),
  `crates/renderer/shaders/triangle.frag:1291-1307` (the escape it models),
  `byroredux/src/render/static_meshes.rs:306` (where the gate's premise stops being true)
- **Status**: NEW (extends #3639, closed today by `1ff5fae4`)
- **Description**: #3639 is a correct fix for a real bug and its reasoning checks out —
  I verified the shader premise line by line, and `0.5` is a reuse of
  `classify_pbr_keyword`'s existing neutral rather than an invented constant, so it is
  not a `no-fabrication` violation in the usual sense. The gap is in the gate:
  ```rust
  if leaf.smoothness >= 1.0 && material.textures.smooth_spec.is_none() {
      material.roughness_override = Some(0.5);
  }
  ```
  `textures.smooth_spec.is_none()` asks *"did anything in the template chain author a
  gloss-map path?"*. The shader condition the fallback exists to stand in for is
  `mat.glossMapIndex != 0u`, which asks *"did a gloss map actually resolve to a bindless
  handle?"* — and `gloss_map_index` is `texture_indices.smooth_spec`
  (`byroredux/src/render/static_meshes.rs:306`), i.e. `0` whenever the authored path missed in the archives.

  A BGSM with `smoothness == 1.0` whose authored `smooth_spec_texture` does not resolve
  therefore keeps `roughness = 0.04` and still has no per-pixel escape — the exact
  symptom #3639 closed. The normal-alpha-as-spec re-point does not rescue it either:
  when it fires, `crates/renderer/shaders/triangle.frag` takes the `normalAlphaSpec` branch, which modulates
  `specStrength` and deliberately leaves `roughness` untouched.
- **Evidence**: I tried to disprove this on the grounds that the boundary cannot know
  archive-resolution results, and the codebase itself refutes that as a reason to stop:
  `normal_has_alpha` is a render-side `MaterialTextureHandles` field precisely *"because
  the DDS format is not known there; that is why `normal_has_alpha` is a render-side
  `MaterialTextureHandles` field and not a `Material` field"*
  (`byroredux/src/render/static_meshes.rs:338-341`). A spawn-time resolve-once site that
  already reads the resolved handle exists —
  `byroredux/src/material_translate.rs:950-999`, `resolve_normal_alpha_spec_roughness`,
  which reads `handles.textures.smooth_spec` — and it early-returns on
  `bgsm_pbr_scalars_authored`, which is exactly #3639's population. The structural home
  for the check is already built and already exempts the case that needs it.

  **Population size: unknown.** Sizing it needs an FO4 corpus census of BGSMs with
  `smoothness == 1.0` whose authored `smooth_spec_texture` misses in the installed
  archives, crossed with #3637's just-changed last-wins archive resolution. I did not
  run it and am not estimating it.
- **Impact**: A bounded residue of #3639's own population renders near-mirror instead of
  neutral. The durable defect is that a boundary decision is keyed on a strictly weaker
  predicate than the consumer's, in a pair the codebase elsewhere keeps in lockstep on
  purpose (#2445 made exactly this class hold "by construction rather than by the
  accident of no such population existing").
- **Related**: #3639 (the fix this extends), #2445 (the same predicate-pair discipline,
  applied), #2606 (why the BGSM population is exempted from the sibling resolve),
  #3637 (changes which archive answers a texture lookup, so it changes this population)
- **Suggested Fix**: Move (or mirror) the fallback into
  `resolve_normal_alpha_spec_roughness`, which already has the resolved
  `gloss_map_index`, and gate it on `gloss_map_index == 0` rather than on the authored
  path — keeping `bgsm_pbr_scalars_authored` as the marker for *which* fallback applies
  instead of as a blanket early return.

#### NIFAL-2026-09-05-D8-02: `record_external_texture_sources` is the fifth hand-written role walk and the only one with no lockstep guard
- **Severity**: LOW
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: `no-leak` (latent)
- **Game Affected**: all
- **Location**: `byroredux/src/asset_provider/material.rs:1053-1091`; single call site at
  `:2037`
- **Status**: NEW
- **Description**: After #3349 (`roles()`) and #3734 (`values()`), `MaterialTextureSet`'s
  role walks are in good shape. The full inventory as of today:

  | Walk | Location | Protected by |
  |---|---|---|
  | `map_ref` | `crates/nif/src/import/types.rs:408` | compiler — full struct literal |
  | `zip_map_ref` | `crates/nif/src/import/types.rs:438` | compiler — full struct literal |
  | `roles()` | `crates/nif/src/import/types.rs:374` | `roles_covers_every_field_in_the_set` (#3349) |
  | `values()` | `crates/nif/src/import/types.rs:478` | `values_covers_every_field_in_the_set` (#3734) |
  | `map_secondary_texture_handles` | `byroredux/src/asset_provider/texture.rs:623` | compiler — full struct literal |
  | `MaterialInfo::into_texture_set` | `crates/nif/src/import/material/mod.rs:1279` | compiler — full struct literal |
  | `supplemental_texture_indices` | `byroredux/src/render/static_meshes.rs:708` | source-scan test (#2697) |
  | **`record_external_texture_sources`** | `byroredux/src/asset_provider/material.rs:1053` | **nothing** |

  It is a `macro_rules!` list of 22 `record!(field)` invocations plus a decals loop. I
  verified it is complete today, name by name. But a role added to the struct and
  forgotten here compiles, passes every existing test, and silently mislabels that
  role's provenance.
- **Evidence**: `grep -rn "record_external_texture_sources" byroredux/src` returns the
  definition and one call site, nothing else — no test references it. Contrast the two
  sibling walks that were given guards this window, both in files this one sits beside.
- **Impact**: Diagnostics-only, which is why this is LOW rather than a re-run of #2697.
  `ImportedMaterial.texture_sources` feeds `MaterialTextureDebugInfo`, which `mat.dump`
  and `tex.missing` zip against the bindless handles to prove an override landed in the
  intended role. An omitted role keeps `ImportedTextureSource::default()` —
  `NifTextureSet` — so the tool would confidently report a BGSM/BGEM/`.mat`-supplied
  texture as NIF-authored. That is a correctness oracle telling the auditor the wrong
  thing, in the exact investigation where it would be trusted.
- **Related**: #3349, #3734, #2697 (the three siblings, all closed by adding a guard);
  #3465 (the prose half of the same problem)
- **Suggested Fix**: The pattern already exists ten lines away in
  `crates/nif/src/import/types.rs` — count `map_ref`'s compiler-forced visits and assert
  the number of `record!` lines matches, either by a source scan of this function (as
  #2697 did for `supplemental_texture_indices`) or by rewriting the body as a
  `zip_map_ref` over `before` and `material.textures`, which would make the compiler
  enforce it outright.

#### NIFAL-2026-09-05-D8-03: `.claude/commands/_audit-common.md` still documents 18 texture roles — the one prose copy #3465's parity test does not scan
- **Severity**: LOW
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: — (documentation of the canonical vocabulary)
- **Game Affected**: all
- **Location**: `.claude/commands/_audit-common.md:97`; the guard that misses it is
  `documented_texture_role_list_matches_the_struct`,
  `byroredux/src/material_translate.rs:1835-1898`
- **Status**: NEW
- **Description**: `MaterialTextureSet` has 22 named roles plus `decals: [T; 4]`.
  `.claude/commands/_audit-common.md:97` says:

  > `MaterialTextureSet<T>` (same file) replaces per-game texture slot numbers with 18
  > named source-agnostic roles + `decals: [T; 4]`.

  #3465 was filed because this prose is the checklist an auditor diffs the role walks
  against, and it had drifted. Its fix added a test that scans the docs from Rust and
  fails with the count that is now true. That test scans exactly two files —
  `docs/engine/nifal.md` and `.claude/commands/audit-nifal/SKILL.md` — and both are
  correct at 22, verified. `.claude/commands/_audit-common.md` is a third copy, and it is the file every
  audit skill loads *first* as the authority on project layout. The guard's own
  docstring says *"Both said '18 named roles' long after the struct reached 22"* — this
  is that same sentence, in the location the fix did not reach.
- **Evidence**:
  ```
  $ grep -rn "named source-agnostic roles" .claude/commands/ docs/engine/nifal.md
  .claude/commands/_audit-common.md:97:  ... with 18 named source-agnostic roles + `decals: [T; 4]` ...
  ```
  versus `.claude/commands/audit-nifal/SKILL.md:244` (22, enumerated by name) and
  `docs/engine/nifal.md:489` (22). The `include_str!` list at
  `byroredux/src/material_translate.rs:1861-1870` contains only the latter two.
- **Impact**: An auditor diffing `values()` against `.claude/commands/_audit-common.md`'s count concludes
  four roles are spurious, or misses four that went absent. This is not hypothetical —
  it is the failure mode that produced #3465, and it is why that fix is a test rather
  than an edit. Everything in `.claude/commands/_audit-common.md` is read by every audit skill, so the
  stale number has the widest readership of the three copies.
- **Related**: #3465 (the fix, which covered two of the three copies), #3439 (the
  validate gate's own coverage hole, still OPEN)
- **Suggested Fix**: One-line correction to 22, plus adding
  `.claude/commands/_audit-common.md` to the `include_str!` array at
  `byroredux/src/material_translate.rs:1861-1870` so the third copy is covered by the same test as the
  other two. Note the test currently asserts only the *count* string against
  `docs/engine/nifal.md` and the *names* against the SKILL file; adding this file to the
  count check is sufficient and cheap.

## Per-Dimension Results

### Dimension 1 — Material

#### Verified clean

- **Signature narrowing holds on both boundaries.** `translate_material` is still
  `(&ImportedMaterial, Option<&str>, ResolvedPaths, u32) -> Material`
  (`byroredux/src/material_translate.rs:471-476`), and `merge_external_material` is
  still `(&mut ImportedMaterial, &mut MaterialProvider, &mut StringPool) -> MergeOutcome`
  (`byroredux/src/asset_provider/material.rs:1114-1118`). Neither has widened back
  toward `ImportedMesh`. Material translation and external-sidecar patching both
  provably cannot read geometry, skinning, transforms or scene ownership.
- **Single boundary, three production callers.** `byroredux/src/scene/nif_loader.rs:983`,
  `byroredux/src/cell_loader/spawn/mesh_instance.rs:837`,
  `byroredux/src/cell_loader/placement_lod.rs:542`, plus `byroredux/src/cornell.rs:2073`
  (the synthetic harness). A `Material {` scan across the workspace returns only these,
  the two declared sibling boundaries, Cornell's own helper constructors, and test
  modules.
- **The boundary literal is compiler-exhaustive.** `translate_material`'s `Material {…}`
  has no `..Default::default()` tail, so adding a canonical field is a hard error at the
  boundary rather than a silent default. This is a stronger property than the audit
  checklist claims for it and worth recording.
- **`resolve_pbr` reads the threaded `specular_authored`** (#2573, `20278ddf`), not a
  hardcoded `false` — verified at `crates/core/src/ecs/components/material.rs:1298`,
  with the field present on `Material`, `ImportedMaterial` and the `translate_material`
  copy. The `FORMAT_MAJOR` 20→21 save-shape bump that accompanied it is consistent.
- **`sanitize_finite` is complete, re-diffed mechanically.** Extracted every
  `f32`/`[f32; N]` field of `Material` (35) and every `fix_scalar!`/`fix_vec!` argument,
  and `comm`-diffed them: the only two absent are `metalness`/`roughness`, both handled
  by the `resolve_pbr()` call at the head of the method. #3731's two descents are
  present and each covers its carrier completely — `EffectFalloff` 5/5,
  `ShaderTypeFields` 13/13.
- **#3073 landed as canonical state.** `parallax_height_scale` / `parallax_max_passes`
  are resolved once at the boundary against named `DEFAULT_PARALLAX_*` constants
  (`byroredux/src/material_translate.rs:637-642`) and are in the `sanitize_finite` list. The
  duplicated `.unwrap_or(0.04)` / `.unwrap_or(4.0)` spawn-site reads are gone from both
  load paths.
- **The #2444 boundary guard is now self-maintaining.** #3733 replaced the
  hand-maintained file-name table with a directory scan of `cell_loader/*.rs` that
  requires every file inserting a `MeshHandle(` to call one of the two boundary
  functions, with a `checked_files.len() >= 6` sanity floor so a broken scan cannot pass
  silently. `byroredux/src/cell_loader/water.rs` is inside it now.
- **BGSM/BGEM texture forwarding is complete.** The BGSM chain forwards 12 of
  `BgsmFile`'s 13 texture fields; the 13th, `distance_field_alpha_texture`, is the
  documented #2642 deferral (no canonical role exists for it) and carries its rationale
  inline at `byroredux/src/asset_provider/material.rs:1550-1556`. BGEM forwards all 8 of its texture
  fields. `BgsmFile` has no `envmap_mask_texture`, so the BGSM arm's lack of an
  `environment_mask` fill is correct, not an omission.
- **`apply_cdb_pbr_fallback` is honest about what it did.** It sets only `is_pbr` and
  returns `MergeOutcome::PresenceOnly`; `merge_external_material` is `#[must_use]` with
  a message that spells out why discarding the outcome erases the distinction (#2709).

#### Stale candidates dropped: 2

1. *`Material::sanitize_finite` misses the nested float carriers* (the 2026-08-30
   MEDIUM). Fixed by #3731 (`9d32fbf2`); both descents verified field-complete. Dropped.
2. *ESM-sourced water planes spawn outside the #2444 boundary guard* (the 2026-08-30
   LOW). Fixed by #3733 (`aa6da23b`); `byroredux/src/cell_loader/water.rs` now calls a boundary
   function and the guard is a directory scan that would have caught it. Dropped.

### Dimension 8 — Shader flags / texture roles

#### Verified clean

- **Zero per-game branches in the shader tree — the cardinal check.** Scanned every
  `*.frag`, `*.vert`, `*.comp` and every `include/*.glsl` for game names, `bsver`,
  `GameKind`, and `game ==`, with comment lines stripped. Two hits survive, and they are
  the same non-branch: `crates/renderer/shaders/water.frag:494` divides by
  `STARFIELD_WATER_CONCENTRATION_REFERENCE`
  (`crates/renderer/shaders/include/shader_constants.glsl:188`), applied unconditionally
  to `push.concentration`, which is zero for every non-Starfield record. A game-*named*
  constant, not a per-game branch, and it is `crates/renderer/shaders/water.frag` rather than the lit path.
  `crates/renderer/shaders/triangle.frag` and its includes have **zero**.
- **26 roles, five walks, one order.** Diffed the struct's field order against `roles()`,
  `values()`, `map_ref` and `zip_map_ref` line by line: identical membership, identical
  order, `base_color` first, so `secondary_values()`'s `skip(1)` is sound. 22 named + 4
  decals = 26.
- **Every role has a producer.** Checked individually, including the three that look
  orphaned at a glance: `decals` ← `NiTexturingProperty.decal_textures`
  (`crates/nif/src/import/material/legacy_properties.rs:310-318` →
  `MaterialInfo::decal_maps` → `into_texture_set:1325`); `reflectance` and
  `emittance_gradient` ← `BSEffectShaderProperty`'s FO76 trailing textures
  (`crates/nif/src/import/material/mod.rs:1313-1320`).
- **Every role reaches the GPU and is read by the shader.** Traced index by index rather
  than by count: the ten directly-named lanes (`normal`, `emissive`→`glowMapIndex`,
  `detail`, `smooth_spec`→`glossMapIndex`, `dark`, `height`→`parallaxMapIndex`,
  `environment`→`envMapIndex`, `environment_mask`→`envMaskIndex`,
  `greyscale_lut`→`greyscaleLutIndex`, plus `base_color` as `texture_handle`) and the
  sixteen `supplemental_texture_slot::*` lanes, all present in
  `crates/renderer/shaders/include/bindings.glsl` and all sampled in
  `crates/renderer/shaders/triangle.frag` — including `decalMap0Index`..`decalMap3Index`
  at `crates/renderer/shaders/triangle.frag:308-311`.
- **`smooth_spec` vs `specular` are not merged**, the mis-merge the brief flags as
  likeliest. `smooth_spec` is produced only by the legacy gloss map and BGSM's
  `smooth_spec_texture`; `specular` only by `slot_to_role`'s `Specular` arms and the
  BGSM/BGEM `specular_texture` fills. Disjoint. Same for `environment` vs
  `environment_mask` (separate slot-4/slot-5 arms on every layout, and BGEM's separate
  `envmap_texture` / `envmap_mask_texture` fills, both gated on
  `bgem.env_mapping_enabled()` per #2643).
- **Per-game slot vocabulary genuinely stops at the import boundary.**
  `TextureSlotLayout` is derived from the wire format (`TextureSlotLayout::from_bsver`),
  rides the *raw* `ImportedMaterial`, and appears nowhere in `crates/renderer/src` or
  `byroredux/src/render/` — grep for `TextureSlotLayout`, `slot_to_role`,
  `texture_slot_layout` across both returns only comments. The canonical `Material` has
  no such field (checked against its full 63-field list).
- **#3732 stayed fixed.** The REFR overlay's `pick` closure now consults
  `slot_to_colocated_role` as well as `slot_to_role`
  (`byroredux/src/cell_loader/spawn/mesh_instance.rs:252-257`), so the two picks for
  slot 2 on the tint family both accept and the `*_sk.dds` lands in `tint` *and*
  `lighting_mask`, matching the import loop.
- **#3734 stayed fixed.** `values_covers_every_field_in_the_set`
  (`crates/nif/src/import/types.rs:569-583`) counts `map_ref`'s compiler-forced visits
  and asserts `values().count()` equals it. A real omission now fails a test.
- **#1592's FO4 flags still reach `MaterialInfo`**, and #2695's single shared slot table
  is intact with per-arm corpus counts (#2694, #2997, #2999, #3085, #2693) rather than
  guessed heuristics. `unrouted_texture_slot_bindings` still counts non-empty bindings
  that reached no role, per layout and slot, so future table gaps stay observable.

#### Stale candidates dropped: 3

1. *The REFR overlay's `pick` cannot express a colocated role* (the 2026-08-30 MEDIUM).
   Fixed by #3732 (`0a7ed21e`). Dropped.
2. *`values()` has no omission guard* (the 2026-08-30 LOW). Fixed by #3734 (`fc2f29da`).
   Dropped.
3. *`supplemental_texture_indices` is a role walk with no lockstep test.* Closed as
   #3814 — `every_supplemental_texture_slot_is_written_exactly_once`
   (`byroredux/src/render/static_meshes.rs:985-1049`) scans the slot module for declared
   lanes and the draw builder for writes, and asserts a bijection. Verified working.
   Dropped.

## Documented-limitation ledger

Restated so the next sweep does not re-derive them. Only Dimension 1 and 8 items are
listed; the full ledger is in `docs/audits/AUDIT_NIFAL_2026-08-30.md`.

- **#3515 (OPEN)** — `texture_clamp_mode` carries two different defaults across the
  three material tiers. Unchanged by #3507's BGSM/BGEM `tile_u`/`tile_v` wiring.
- **#3567 (OPEN)** — the Oblivion `APPLY_HILIGHT2` normal-map alpha is consumed as both
  parallax height and the normal-alpha-as-spec mask; the render-side predicate never
  consults `Material::parallax_height_in_alpha`. Renderer-owned, not re-filed here.
- **#2532 (OPEN)** — the canonical-tier completeness harness covers 1 of ~5 declared
  translate boundaries. Dimension 9, not executed this run.
- **#3398 (OPEN)** — Starfield CDB Phase 2. Until it lands, Starfield materials reach
  `translate_material` with `metalness_override` / `roughness_override` at `None`, which
  is what makes `resolve_pbr`'s classifier backstop a live path rather than dead code
  (the #2573 reachability argument).
- **`distance_field_alpha_texture` (#2642)** — BGSM v>=17 slot deliberately not
  forwarded: `MaterialTextureSet` has no role for it. A deferred-consumer gap with an
  inline rationale, not a wiring bug. Do not re-file.
- **BGSM's eleven no-sink scalars (#2704)** — the wetness-control suite, porosity pair,
  `adaptive_emissive_exposure_offset`, `aniso_lighting`, `external_emittance`. Decoded,
  no `ImportedMaterial` sink, flagged in-code so a completeness sweep can tell "not yet
  wired" from "overlooked". `aniso_lighting` specifically is the enable bit without a
  strength scalar — `Material.anisotropic = 0.0` is the no-guessing policy (#3613), not
  missing data.
- **Emissive scale is still a deliberate no-op.** `emissive_mult` / `emissive_source`
  are copied straight across with no normalization constant anywhere. Re-censused
  2026-08-29 (#3337). Do not re-open.
- **Skyrim slot-2 colocation residue — measured this audit, not a finding.**
  `slot_to_colocated_role`'s doc (`crates/nif/src/import/material/slot_role.rs:274-275`)
  says the shader's unit `lightingMask` default *"now remains reachable only for the
  BGSM lane"*. Census over the same 22,047 Skyrim NIFs (73,128 non-stub
  `BSLightingShaderProperty` blocks): 8,222 have `Soft_Lighting` or `Rim_Lighting` set —
  4,054 tint-family (matching #3458's own figure, which cross-validates the method) and
  4,168 non-tint. Of the non-tint set, 4,107 have a populated slot 2 with no `Glow_Map`
  and route correctly to `LightingMask`; **23** have `Glow_Map` *also* set, so slot 2
  routes to `Emissive` alone and `lighting_mask` stays empty while the
  `MAT_FLAG_SOFT_LIGHTING` gate crosses. Those 23 are genuine `_g.dds` glow maps
  (`dlc1harkonashpile01.nif`, `forgemaster.nif`), so routing them to `Emissive` alone is
  defensible and I am **not** filing it. Only the "only the BGSM lane" phrasing is
  overstated. Recorded with numbers so the next sweep can settle it in one read.

## Stale Candidates Dropped: 5

Every candidate was re-checked against current code before inclusion, per the standing
rule that roughly one finding in six in past sweeps was stale. Five were dropped — all
five because the code was already fixed, which is a better ratio than usual and reflects
that the 2026-08-30 report's whole finding set was closed in the interim.

| # | Candidate | Why dropped |
|---|---|---|
| 1 | `Material::sanitize_finite` never descends into `effect_falloff` / `shader_type_fields` | Fixed by #3731 (`9d32fbf2`); both descents verified field-complete (5/5 and 13/13) |
| 2 | ESM-sourced water planes spawn with no canonical `Material` | Fixed by #3733 (`aa6da23b`); the guard is now a directory scan that would catch a recurrence |
| 3 | The REFR overlay's `pick` closure cannot express a colocated role | Fixed by #3732 (`0a7ed21e`); both slot-2 picks now accept on the tint family |
| 4 | `values()` has no omission guard, only a reordering guard | Fixed by #3734 (`fc2f29da`); the new test counts `map_ref`'s visits |
| 5 | `supplemental_texture_indices` is a hand-written role walk with no lockstep test | Closed as #3814; `every_supplemental_texture_slot_is_written_exactly_once` asserts a bijection between declared lanes and CPU writes |

## Note on method

Two corpus censuses were run against installed vanilla game data, each via a temporary
`crates/nif/examples` binary built with `cargo build --release -p byroredux-nif` and
deleted afterward (no repository change remains):

1. `NiFlipController.texture_slot` histogram over Oblivion / FO3 / FNV / Skyrim mesh
   archives — the evidence for D8-01's blast radius.
2. `BSLightingShaderProperty` slot-2 / soft-lighting occupancy over Skyrim
   `Meshes0`+`Meshes1` — the evidence for the ledger's colocation-residue entry. Its
   tint-family count (4,054) independently reproduces #3458's own published figure,
   which is why I trust the 23-property number sitting next to it.

Everything else is static analysis of the live tree. Nothing in this report is asserted
from a prior audit, a doc, or a commit message without re-reading the current code; the
five dropped candidates above are what that produced.

## Next Step

```
/audit-publish docs/audits/AUDIT_NIFAL_2026-09-05.md
```

Domain label `nifal` for all four. Add `import-pipeline` + `animation` +
`game:oblivion` to D8-01, `import-pipeline` + `game:fo4` to D1-01, `test-gap` to
D8-02, and `doc-rot` + `tech-debt` to D8-03.
