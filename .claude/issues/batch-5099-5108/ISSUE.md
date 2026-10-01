## #5099 [OPEN] NIFAL-D1-2026-09-29-01: the spec names a #[cfg(test)]-only glass classifier as the live boundary step, and #4855's from_bgsm provenance input is recorded nowhere
labels: documentation, low, nifal, doc-rot

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-29.md`
**Severity**: LOW
**Dimension**: Material · **Tier Violated**: doc / record-keeping · **Game Affected**: all
**Location**:
- `docs/engine/nifal.md` §3 step 4 ("classifies glass once, alpha-aware (`helpers::classify_glass_into_material`)") and the Layering note below it; the same §3 paragraph naming `cell_loader/placement_lod.rs` as the exempt production caller;
- `byroredux/src/material_translate.rs`, intra-doc link `[`crate::helpers::classify_glass_into_material`]` in `translate_material`'s rustdoc;
- `docs/engine/asset-pipeline.md` and `docs/engine/material-abstraction.md` (`helpers::classify_glass_into_material`);
- comments in `byroredux/src/asset_provider/material/merge.rs` (two sites).

## Description
`2b1b7fc5c` made `helpers::classify_glass_into_material` a `#[cfg(test)]` shim that passes `from_bgsm = false`. Production now calls `classify_glass_into_material_with_provenance` (9 arguments, including the new `from_bgsm`). The live boundary therefore consumes two distinct provenance signals:
- `external_material_resolved` gates keyword promotion of effect carriers (#4283);
- `from_bgsm` gates overriding an authored lit dispatch `2..=20` (#4855).

Every spec/doc site still names the shim, and none records the two-signal split. The same §3 paragraph lists `placement_lod.rs` as the Phase-2-exempt production caller but omits `object_lod.rs`, which has been a `translate_material` caller since #4245 and also attaches no `MaterialTextureHandles`.

## Evidence
`rg 'classify_glass_into_material\b' docs/engine byroredux/src` finds the non-test sites above; the definition in `byroredux/src/helpers.rs` is `#[cfg(test)] fn classify_glass_into_material`.

## Impact
An auditor or contributor following the spec reads a function production never calls; the rustdoc link resolves only under `cfg(test)`.

## Related
#4873 (open — the `terrain_lod_btr.rs` Phase-2 omission and ground-cover wording in the same §3 section; could be folded together); #4246, #4855, #4283.

## Suggested Fix
- Rename the references at those sites to `classify_glass_into_material_with_provenance`.
- Add the provenance pair (`external_material_resolved`, `from_bgsm`) to §3 step 4.
- Add `object_lod.rs` to the Phase-2 caller and exemption list.

Validated at HEAD 9fcfdc3fc: `helpers.rs` has `#[cfg(test)]` on `classify_glass_into_material` and `pub(crate) fn classify_glass_into_material_with_provenance`; nifal.md, asset-pipeline.md, material-abstraction.md, `material_translate.rs` rustdoc and merge.rs comments still name the shim.

## Completeness Checks
- [ ] **SIBLING**: `byroredux/src/render/static_meshes.rs` comments naming the shim updated too
- [ ] **CANONICAL-BOUNDARY**: doc changes only; `translate_material` behaviour unchanged


## #5101 [OPEN] FO4-D1-01: merge_precombine_materials (the opaque-architecture blend restore) has no test on either route; the new drain-route guard passes mat_provider = None, so the merge never runs in it
labels: bug, import-pipeline, low, legacy-compat, game:fo4, test-gap

**Source report**: `docs/audits/AUDIT_FO4_2026-09-29.md`
**Severity**: LOW (test-gap)
**Dimension**: M49 precombines

## Location
- `byroredux/src/cell_loader/precombined.rs` (`merge_precombine_materials`, and its main-thread job call site).
- `byroredux/src/cell_loader/partial.rs` (`finish_partial_import` stream-drain call site, behind `if let Some(provider) = mat_provider`).
- `byroredux/src/cell_loader/finish_partial_tests.rs` (`finish_partial_import_builds_precombine_geometry_entry`).

## Description
e593770f0 factored the #1619-follow-up blend restore into `merge_precombine_materials`: keep the BGSM's two_sided / decal / alpha_test flags but restore the NIF-side `has_alpha` / `src_blend_mode` / `dst_blend_mode`, because FO4 authors the "Standard" blend identically on lab glass and opaque Institute metal. The same commit calls the helper from the new streaming-drain branch of `finish_partial_import`.

`grep merge_precombine_materials` finds only the function and its two callers — no test. The new drain-route guard calls `finish_partial_import(&mut world, None, key, partial, …)`, so the provider branch that applies the merge never executes in it.

## Evidence
- `finish_partial_tests.rs`: `finish_partial_import(&mut world, None, key, partial, &|_| false);` (every call in the file passes `None`).
- `partial.rs`: the merge sits behind `if let Some(provider) = mat_provider`.

Validated at HEAD 9fcfdc3fc: `rg merge_precombine_materials byroredux/src` → definition + 2 call sites only; all `finish_partial_import` test calls pass `None` for the provider.

## Impact
Nothing is wrong today. But if either route drifted back to a bare `merge_external_material` loop (the efd3c41b regression shape), every test would still pass, and precombined Institute / lab walls would render as `MATERIAL_KIND_GLASS` (see-through, mirror-hazy) on that route only. Streamed exteriors and synchronous interiors now take different routes, so a one-sided drift shows only as a visual difference between load paths.

## Related
- #1619, efd3c41b (the original regression); `/audit-fo4` SKILL Dim 1 "Unguarded" note.

## Suggested Fix
Add a unit test for the helper with an in-memory `MaterialProvider` holding a "Standard"-blend BGSM (function 1, src 6, dst 7, two_sided, alpha_test) and a mesh whose NIF-side `has_alpha = false`; assert the blend triple is restored and the BGSM's two_sided/alpha_test survive. Add a provider-backed variant of the drain-route test.

## Completeness Checks
- [ ] **SIBLING**: Both routes (main-thread `PrecombinedSpawnJob` and stream drain) covered
- [ ] **CANONICAL-BOUNDARY**: The restore stays at the import/merge boundary, never re-derived at render time
- [ ] **TESTS**: A regression test pins this specific fix


## #5102 [OPEN] NIFAL-D1-2026-09-29-02: mesh → FogVolume beam/fog replacement runs on the cell path only and is an unrecorded drawn-surface exemption
labels: bug, renderer, low, game:fnv, game:fo3, game:fo4, game:oblivion, terrain-exterior, nifal

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-29.md`
**Severity**: LOW
**Dimension**: Material (drawn-surface boundary) / Completeness
**Tier Violated**: single-boundary (load-path divergence); no-fabrication (weak sense)
**Game Affected**: Oblivion, FO3, FNV, FO4
**Location**:
- `byroredux/src/cell_loader/nif_import_registry.rs`: `CachedNifImport::beam_volumes`, which chains the asset-signature converters in `byroredux/src/fog.rs` (`fnv_nellis_hangar_beam_volumes_from_mesh`, `window_beam_volume_from_mesh`, `oblivion_dungeon_beam_volume_from_mesh`, `authored_cone_beam_volume_from_mesh`, `fnv_superwide_beam_volume_from_mesh`, `vault_window_beam_volume_from_mesh`);
- `byroredux/src/cell_loader/spawn/mesh_instance.rs` (`prepare_fog_mesh_instance` → `fog::fog_volume_from_mesh`, and the `FogGroup` path);
- `byroredux/src/scene/nif_loader.rs` calls none of them.

## Description
`0572bfd5a` added six asset-signature converters, each matching a file name plus exact vertex/index counts (e.g. `NVNellisHangarInteriorLightBeam.nif` 110 verts / 120 indices; FO4 `emergencylightbeam01.nif` 242/1080; Oblivion `lightbeam01.nif` 14/36). On a match the drawn submesh is **replaced** before upload: no `Material`, no raster entity, no BLAS; one or more canonical `FogVolume`s are emitted instead. The older fog-token converter (`fog_volume_from_mesh`, `733dff8f1`) does the same for `dst_blend == 7` fog/smoke quads.

This is a NIF-data → canonical-type translation with no declared boundary in `nifal.md` §2. `nifal.md` §3 says drawn-surface exemptions are "exactly three" (Cornell, save, ground cover); this would be a fourth. The media parameters are uncited: extinction 0.12 m⁻¹, albedo 0.9, `edge_softness` 0.35–0.65, an 80-BU beam half-width, extrusion of span × 0.3 or width × 0.75. (A painted card has no extinction to translate, so this is weak-sense no-fabrication, not invented data.)

## Evidence
`rg 'beam_volume_from_mesh|beam_volumes_from_mesh|fog_volume_from_mesh' byroredux/src` finds only `fog.rs`, its tests, `nif_import_registry.rs` and `mesh_instance.rs`.

## Impact
The same NIF renders two ways: `cargo run -- effects/ambient/windowlightbeam.nif` draws the painted card, while that NIF placed in a cell becomes a medium. The real game path (cells) is unaffected; the main cost is that the population is invisible to the §3 exemption ledger and the NIFAL boundary inventory.

## Related
EXT-D1-02 / EXT-D4-0x (`AUDIT_EXTERIOR_2026-09-27.md`, godray lighting side), #4809 (beam classification caching), `docs/engine/interior-godrays-status.md`. `fog.rs` is owned by exterior/renderer; this is the NIFAL facet only.

## Suggested Fix
- Record "mesh → participating medium (cell path)" in `nifal.md` §2/§3 as a declared boundary, citing the converters and the status doc.
- Either route the loose-NIF spawn through `beam_volumes` / `fog_volume_from_mesh`, or state the viewer divergence as deliberate.
- Cite or measure the media constants.

Validated at HEAD 9fcfdc3fc: `CachedNifImport::beam_volumes` chains the six beam converters; `nifal.md` §3 still says the exemption list "has exactly three"; `scene/nif_loader.rs` has no call to any beam/fog-from-mesh converter.

## Completeness Checks
- [ ] **SIBLING**: other cell-path-only mesh replacements (e.g. particle → medium `medium_from_particle`) checked for loose-NIF parity
- [ ] **CANONICAL-BOUNDARY**: any routing change keeps the translation at spawn/import, never at render time
- [ ] **TESTS**: a pin that both spawn paths agree (or that the divergence is documented)


## #5103 [OPEN] NIFAL-D5-2026-09-29-01: nifal.md §2 Particles still calls the legacy-data and #3329 sequence rate tiers "whole-scene" after #4560 and #4620 made both per-instance
labels: documentation, nif-parser, low, nifal, doc-rot

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-29.md`
**Severity**: LOW
**Dimension**: Particles · **Tier Violated**: doc / record-keeping · **Game Affected**: Oblivion, FO3, FNV, Skyrim
**Location**: `docs/engine/nifal.md` §2 Particles, Attribution paragraph: "Still whole-scene: the legacy `NiParticleSystemController` fallback and the #3329 sequence tier (a sequence's controlled block names an emitter controller, not an emitter instance)". Also `.claude/commands/audit-nifal/SKILL.md` Dim 5.

## Description
Both tiers now resolve through `find_own_emitter_ctlr_refs` (`crates/nif/src/import/walk/emitter.rs`):
- per #4560, the legacy data is `NiPSysEmitterCtlr`'s own `Data` ref, and an unlinked block is attributed to nobody;
- per #4620, a non-null `cb.controller_ref` must equal the system's own controller, and a null ref falls back to the system name.

The paragraph's stated reason ("names an emitter controller, not an emitter instance") is exactly what #4620 disproved. The only remaining whole-scene piece is the budget fallback when the own `data_ref` does not resolve. The audit-nifal skill's Dim 5 text carries the same stale claim. Introduced by `a6bcec6e2` + `b7491072f`, neither of which touched nifal.md.

## Impact
The next particle change is planned against a boundary that no longer exists — the same failure class as #4409.

## Related
#4560, #4620, #4409, #4261.

## Suggested Fix
Replace the sentence with "Still whole-scene: only the emitter-budget scan when the own `data_ref` does not resolve", citing #4560 and #4620, and sync the audit-nifal skill Dim 5 text.

Validated at HEAD 9fcfdc3fc: nifal.md still reads "Still whole-scene: the legacy `NiParticleSystemController` …" in §2 Particles.

## Completeness Checks
- [ ] **SIBLING**: audit-nifal SKILL.md Dim 5 synced in the same change


## #5104 [OPEN] FO4-D1-02: e593770f0 inserted a new test inside another test's doc comment — resolve_precombine_owner_follows_form_id_mod_index lost the first line of its doc
labels: documentation, low, legacy-compat, tech-debt, game:fo4

**Source report**: `docs/audits/AUDIT_FO4_2026-09-29.md`
**Severity**: LOW
**Dimension**: M49 precombines (doc hygiene)

## Location
`byroredux/src/cell_loader/precombined.rs` (test module: `oc_nif_paths_are_already_canonical_cache_keys` / `resolve_precombine_owner_follows_form_id_mod_index`).

## Description
e593770f0 inserted the new test `oc_nif_paths_are_already_canonical_cache_keys` inside another test's doc comment. The #1590 doc line now sits on top of the new test, and its continuation sits alone above the test it belongs to.

## Evidence
```
    /// #1590 (a) — the CSG + subdir follow the cell's owning plugin (form-id
    /// The streaming worker pre-parses precombines under the key the drain
    ...
    fn oc_nif_paths_are_already_canonical_cache_keys() {
    ...
    /// mod-index byte → load order), not the last-loaded `--esm`.
    #[test]
    fn resolve_precombine_owner_follows_form_id_mod_index() {
```

Validated at HEAD 9fcfdc3fc: the spliced comment is present exactly as above.

## Impact
Cosmetic. Both tests' rationale reads garbled, and the #1590 attribution sits on the wrong test.

## Suggested Fix
Move the `/// #1590 (a) — …` line down to rejoin its continuation above `resolve_precombine_owner_follows_form_id_mod_index`.

## Completeness Checks
- [ ] **SIBLING**: Other e593770f0 test insertions checked for the same splice


## #5105 [OPEN] TD3-2026-09-29-01: CLAUDE.md's Workspace Structure names two functions deleted months ago and a type that never existed
labels: documentation, low, tech-debt, doc-rot

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 3 · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Location**:
  - `CLAUDE.md:125` (`resources.rs  build_blas_for_mesh, register_ui_quad, …`)
  - `CLAUDE.md:128` (`mod.rs  … new()/destroy()/debug_assert_scratch_aligned()`)
  - `CLAUDE.md:66` (`archive.rs  GameArchive — wraps BSA … or BA2`)
  - `crates/facegen/src/eval.rs:116`
- **Evidence**:
  - `build_blas_for_mesh` was deleted by `999478ef4` (2026-08-15, #2914, "delete the dead single-shot
    BLAS path"). `resources.rs:323` now says "never-called single-shot `build_blas_for_mesh`".
  - `debug_assert_scratch_aligned` was deleted by `d6d0516f9` (2026-06-01).
  - `GameArchive` appears in no `.rs` file in the whole history; the type is
    `asset_provider::archive::Archive`.
  - `facegen/src/eval.rs:116` still says `out` "reaches the vertex SSBO and `build_blas_for_mesh`".
  - `_audit-validate.sh` covers skills and docs/engine only, not CLAUDE.md.
- **Impact**: CLAUDE.md is loaded into every agent session, and `_audit-common.md` names it as the
  authoritative tree.
- **Suggested Fix**:
  - Replace the three names with `build_blas_batched`, the round-up-at-use note, and `Archive`.
  - Fix the facegen comment.
  - Point the validator's symbol advisory at CLAUDE.md and AGENTS.md too.

**Validated at HEAD 9fcfdc3fc**: `CLAUDE.md:66/125/128` and `crates/facegen/src/eval.rs:116` still name `GameArchive` / `build_blas_for_mesh` / `debug_assert_scratch_aligned`; `git grep` finds no definition of any of the three in `*.rs`; the archive type is `pub(crate) struct Archive` (`byroredux/src/asset_provider/archive.rs:6`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files


## #5106 [OPEN] NIFAL-D8-2026-09-29-01: #4636's dead-path yield lets a BGSM win the greyscale-LUT texture while the #4402 enable-bit rule still treats the slot as NIF-won
labels: bug, import-pipeline, low, game:fo4, game:fo76, nifal

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-29.md`
**Severity**: LOW (latent; population not censused)
**Dimension**: Shader flags / texture roles (merge boundary)
**Tier Violated**: single-boundary (two precedence decisions for one slot disagree) · **Game Affected**: FO4, FO76
**Location**: `byroredux/src/asset_provider/material/merge.rs`:
- `nif_supplied_greyscale_lut` captured before the chain walk;
- the bit rule: if the slot is empty, assign (BGSM wins); else if the NIF supplied the slot, OR;
- `fill(&mut material.textures.greyscale_lut, &bgsm.greyscale_texture, …, texture_exists)` runs **after** the bit rule;
- the #4636 replacement branch of `fill` (dead NIF path + live sidecar path → replace).

## Description
Suppose the NIF's slot-3 LUT path resolves in no archive and the BGSM's does. The bit block sees a filled slot, takes the NIF-won branch, and ORs the BGSM bit in. `fill` then replaces the slot with the BGSM's LUT. The material ends up sampling the BGSM's texture under an enable that is NIF SLSF1 bit OR BGSM bit. That violates the #2108/#4402 contract documented in the same function ("this BGSM wins the slot → authoritative for both the texture and the enable bit (assignment, including OFF)"). Because `nif_supplied_greyscale_lut` stays true, every ancestor step in the chain also ORs. Introduced by the interaction with `78d079707` (#4636).

## Evidence
The outcome diverges only when the NIF bit is ON, the BGSM bit is OFF, the NIF LUT is dead and the BGSM LUT is live — then the remap stays on although the winning material authored it off. #4636's census covered the normal slot only (3 dead FO4 paths out of 637 disagreements), so vanilla incidence for the LUT slot is unmeasured (likely ~0); mods/retextures can reach it. The BGEM arm assigns the LUT without `fill`, so #4636 never applies there.

## Impact
Wrong palette remap on the affected mesh. Below the HIGH `translate_material` floor only because there is no known population.

## Related
#4636, #4402, #3898, #2108, #4286.

## Suggested Fix
Decide the winner once: run the LUT `fill` first, have it report whether this BGSM's path took the slot, and apply assign-vs-OR from that result.

Validated at HEAD 9fcfdc3fc: in `merge.rs` the `if material.textures.greyscale_lut.is_none() { assign } else if nif_supplied_greyscale_lut { OR }` block precedes `fill(&mut material.textures.greyscale_lut, …)`, and `fill` replaces a dead NIF path with a live sidecar path.

## Completeness Checks
- [ ] **SIBLING**: other slots whose enable bits are decided before `fill` (and the BGEM arm)
- [ ] **CANONICAL-BOUNDARY**: fix stays in the merge / `translate_material` boundary, never in shaders
- [ ] **TESTS**: A regression test pins the dead-NIF-LUT + live-BGSM-LUT + BGSM-bit-OFF case


## #5107 [OPEN] TD3-2026-09-29-02: AGENTS.md is a divergent fork of CLAUDE.md (84 differing lines) and still recommends the #3895 test trap
labels: documentation, low, tech-debt, doc-rot

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 3 · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Location**: `AGENTS.md` (added `db625997b` 2026-07-27; last edited `76c4521cf` 2026-09-23)
- **Evidence**:
  - `AGENTS.md:10` reads `cargo test -p byroredux-core    # Run ECS/core tests (162 tests)`. That is the
    exact command CLAUDE.md:16-25 warns silently drops the #486 inspect-gated guards (#3895).
  - It carries a "rustc ≥ 1.94 / distro rustc 1.93.1" toolchain section that CLAUDE.md dropped.
  - It repeats the `GameArchive` line.
  - Neither file refers to the other.
- **Suggested Fix**:
  - Fold any still-true fact unique to AGENTS.md into CLAUDE.md.
  - Replace AGENTS.md with a pointer, or a symlink, to CLAUDE.md.

**Validated at HEAD 9fcfdc3fc**: `AGENTS.md:10` reads `cargo test -p byroredux-core    # Run ECS/core tests (162 tests)` (no `--features inspect`); the rustc ≥ 1.94 section is at :16; `diff CLAUDE.md AGENTS.md` shows 84 differing lines; AGENTS.md:71/133/136 repeat the TD3-01 stale names.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files


## #5108 [OPEN] TD3-2026-09-29-03: docs/feature-matrix.md contradicts the player body and dialogue features shipped 09-28/09-29
labels: documentation, low, tech-debt, gameplay, dialogue, doc-rot

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 3 · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Location**:
  - `docs/feature-matrix.md:246`: "container/corpse transfer, visible player-mesh attachment, general HUD
    bars, and quest-objective presentation remain open".
  - `docs/feature-matrix.md:207`: "Dialogue tree + dialogue UI integration | ✗ M43 remainder".
- **Evidence**:
  - The player mesh shipped in `a070baaad` (body + view toggle), `db8351587` (third-person walk/idle) and
    `0182fc5e8` (mid-life gear import). `ROADMAP.md:239` lists it with `p3-player-body.sh`.
  - Dialogue shipped in `ab31cfefe` / `766e1746e` (NPC activation → topic selection, native response
    surface). `ROADMAP.md:240` says "live-verified in MarkarthWarrens".
  - The matrix was last touched 09-27.
  - The container-transfer clause should also be re-checked against `container_loot_system` (#4712).
- **Impact**: `_audit-common.md` names the matrix as the status floor audits re-check. These two rows
  would lead a gameplay or UI audit to scope shipped features as absent.
- **Related**: sibling matrix findings AUD-2026-09-29-D5-04 and CHAR-2026-09-29-D5-01 cover other rows.
- **Suggested Fix**:
  - Drop "visible player-mesh attachment" from row 246.
  - Mark row 207 as "~ single-level topic selection + native response surface (P4); tree/UI open".

**Validated at HEAD 9fcfdc3fc**: `docs/feature-matrix.md:207` still reads `Dialogue tree + dialogue UI integration | ✗ M43 remainder`; `:246` still lists "visible player-mesh attachment" as open.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files


