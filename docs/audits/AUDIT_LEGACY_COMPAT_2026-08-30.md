# Legacy Compatibility Audit — 2026-08-30

**Base:** `64f64480` · **Type:** full `/audit-legacy-compat` sweep, all 7
dimensions, run solo in-process (no sub-agent fan-out), as part of a
`--preset comprehensive` audit-suite run.

## Scope

All seven dimensions: coordinate-system correctness (Z-up→Y-up), NIFAL
cross-layer mapping shape, the material translation boundary, PHYSAL's source
axis, EXAL/WATAL, per-game translation-survey patterns (A/B/C), and subsystem
coverage vs the legacy engines.

**Delta weighting.** 45 commits since the prior sweep
(`docs/audits/AUDIT_LEGACY_COMPAT_2026-08-27.md`, base `969d81c8`) — 466 files,
+43,361 / −2,279. The window is dominated by four multi-issue fix batches that
land directly on this audit's boundaries: `19813460` (Oblivion `APPLY_HILIGHT2`
parallax, `TREE.ICON`), `fa511bbf` (`TexDesc` clamp-nibble decode), `1ccf1abe`
(`bhkHinge` decode, posted float/colour channels, manager-blend emitter rate),
`d5a8c36c` (soft-lighting mask, glass pivots) and `d9d2d16a` (Starfield shader
enum). `crates/nif/src/import/` alone moved +1,100 lines across
`slot_role.rs` (+221), `types.rs` (+224), `walk/mod.rs` (+176),
`dedicated_shader.rs` (+153); `material_translate.rs` +169. Every claimed
single-producer contract was therefore re-traced from scratch against HEAD
rather than carried forward from the prior report — which is how this sweep
caught one of its own first-pass conclusions being wrong (see D2's correction
on ESM water planes).

**Source-availability statement.**

| Reference | Status |
|---|---|
| Gamebryo 2.3 source (`/media/matias/Respaldo 2TB/…/Gamebryo_2.3/`) | **UNMOUNTED** — `ls` returns "No such file or directory". Not consulted. Substitutions are stated at each finding. |
| `/mnt/data/src/reference/gamebryo-v32/Include/NiTransform.h` | **Consulted** — `m_Rotate` / `m_Translate` / `float m_fScale` (lines 25-27), settling the transform-fidelity question in D7. |
| `/mnt/data/src/reference/nifxml/nif.xml` | **Consulted** — `bhkHingeConstraintCInfo` (`:2447-2464`, both era layouts), `AlphaFlags` bitfield (`:1554-1563`), `NiTexturingProperty` field list (`:5229-5269`). |
| Vanilla mesh/texture archives (Oblivion, FO3, FNV, Skyrim SE, FO4, Starfield) | **UNMOUNTED this run** — `/media/matias` holds only `ROMS` and a `Videos` volume; a filesystem-wide `find` for `*.bsa` returns only synthetic `/tmp` test fixtures. **No corpus census was possible.** Every occupancy figure quoted below is re-quoted from a cited prior measurement, never re-measured, and each finding whose premise depends on unmeasured occupancy says so in its Confidence line. |

**Method.** Static analysis only, per dispatch (memory-constrained run; no
cargo invocation, no engine launch, no sub-agent delegation). Each dimension was
run in-process and written to `/tmp/audit/legacy/dim_N.md` before consolidation.
Deduplicated against the 159 open GitHub issues fetched live this run
(`gh issue list --state open --limit 400`), against the 15 sibling reports of
this same suite run (`docs/audits/AUDIT_*_2026-08-30.md`), and against
`docs/engine/{nifal,exal,physal,watal,per-game-translation-survey}.md`. No source
file, game file, or GitHub issue was modified; `git status` on `crates/` and
`byroredux/` is unchanged from the start of the run.

## Executive Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 2 |
| **Total (new, owned by this report)** | **3** |

Plus, not counted in the totals:

| Class | Count | Detail |
|---|---:|---|
| Independently derived, **deferred as duplicates** to the owning sibling audit | 2 | LC-D3-01 → `NIFAL-2026-08-30-D8-01`; LC-D4-01 → `PHYS-D4-2026-08-30-01` |
| **Existing** open issues re-verified at HEAD, not re-filed | 3 | #3534, #3536, #3537 |
| Prior sweep finding **verified FIXED** | 1 | LC-2026-08-27-D7-01 → #3530 (`19813460`) |
| Stale candidates investigated and **dropped** | 5 | enumerated below |

**The three layers are structurally intact.** NIFAL's material boundary, EXAL's
exterior boundary and PHYSAL's source axis all re-verified single-producer, with
no per-game branch downstream of any `translate()`, no bare numeric BSVER
comparison anywhere in production, and zero `game ==` in any shader.

**The one MEDIUM is not a code defect — it is an actionable wrong instruction in
the engine's own design docs, and this audit's own skill file points at it.**
`per-game-translation-survey.md` §5 "Pattern A" tells contributors that the
parser bypasses seven named `NifVariant` feature-flag helpers and that migrating
every raw `bsver()` comparison onto them is the "highest-leverage starter". All
seven helpers were **deliberately deleted** (#938 / #1511 / #1840 / #1897) and
`version.rs:699-718` records why, in terms: keeping them was *"an architectural
foot-gun"* that reintroduces a known mis-parse class, and *"no feature-flag
predicates remain on `NifVariant` — this doctrine is fully enforced now."* The
survey's prescription is the exact inverse of the enforced doctrine, and
Dimension 6 of `audit-legacy-compat/SKILL.md` instructs every future auditor to
audit by it.

That is the second consecutive sweep whose highest finding is about this audit's
own reference material rather than the engine — a signal worth acting on, since
the material is now demonstrably steering auditors wrong rather than merely
being out of date.

### Skill-vs-code discrepancies (reported per dispatch instruction)

Three places where the skill file's premise disagrees with HEAD. In each case
the code was trusted, the check run anyway, and the outcome recorded:

1. **Dimension 6, Pattern A** — *"hardcoded BSVER constants where a named helper
   already exists but call sites bypass it"*. Zero such sites exist; the helpers
   do not exist either. Investigated anyway → **LC-2026-08-30-D6-01 (MEDIUM)**.
2. **Dimension 7** — *"flag fidelity gaps (non-uniform scale is collapsed to
   uniform `f32`)"*. Gamebryo `NiTransform` carries `float m_fScale`
   (`gamebryo-v32/Include/NiTransform.h:27`); there is no non-uniform scale to
   collapse. The real (and already-filed, #3532) loss is scale baked into a
   `NiMatrix3`. No finding; the bullet should be reworded.
3. **Dimension 5, VWD** — *"has zero consumers"*. `VisibleWhenDistant` is
   stamped at spawn and read every reconcile by
   `streaming_helpers::resident_vwd_refr_cells`. The substance survives (the
   *cull* is still unwired) but the reason changed from "unwired" to
   "deliberately deferred pending live validation" (#3307). Already tracked by
   the still-open #3534.

### Per-dimension finding counts (every dimension enumerated)

| Dimension | CRIT | HIGH | MED | LOW | Findings |
|---|---:|---:|---:|---:|---|
| 1. Coordinate-system correctness (Z-up→Y-up) | 0 | 0 | 0 | 0 | **none — clean** |
| 2. NIFAL — canonical NIF→ECS mapping shape | 0 | 0 | 0 | 0 | **none — clean** (one first-pass conclusion corrected; deferred to `NIFAL-2026-08-30-D1-02`) |
| 3. Material translation boundary | 0 | 0 | 0 | 1 | LC-2026-08-30-D3-01 (+1 MEDIUM deferred to `/audit-nifal`) |
| 4. PHYSAL — per-game Havok → solver (source axis) | 0 | 0 | 0 | 0 | **none owned** (1 MEDIUM deferred to `/audit-physics`) |
| 5. EXAL / WATAL — exterior + water → renderer & solver | 0 | 0 | 0 | 0 | **none — clean** (2 Existing: #3534, #3536) |
| 6. Per-game translation-survey gaps (Pattern A/B/C) | 0 | 0 | 1 | 0 | LC-2026-08-30-D6-01 (1 Existing: #3537) |
| 7. Subsystem coverage vs legacy | 0 | 0 | 0 | 1 | LC-2026-08-30-D7-01 |

**Dimensions producing no owned findings: 1, 2, 4, 5** — four of seven.

---

## Dimension 1: Coordinate-system correctness (Z-up → Y-up)

Findings: 0.

Re-traced at HEAD 64f64480 (45 commits / 466 files since prior base 969d81c8):

- Single `(x, z, -y)` producer: `crates/core/src/math/coord.rs:73` (`zup_to_yup_pos`)
  and `:90` (`zup_to_yup_quat_wxyz`). Only other array-form hits in the tree are
  two throwaway `crates/nif/examples/_tmp_sf_d2_*.rs` probes (untracked scratch,
  not compiled into the engine) and a historical comment at
  `byroredux/src/cell_loader/references/import.rs:249`. No new swizzle site.
- `EXTERIOR_CELL_UNITS` remains the sole cell-size constant. Every production
  `4096.0` literal resolves to it, to `RENDER_ORIGIN_SNAP`'s explicit re-export
  (`crates/renderer/src/vulkan/scene_buffer/constants.rs:368-371`), or to an
  unrelated quantity. `crates/core/src/ecs/components/camera.rs:710` (flagged by
  a naive grep) is inside `#[cfg(test)]` (module opens at :481).
  DISMISSED CANDIDATE: `byroredux/src/systems/cinematic.rs:326` multiplies a
  cart-marker heading by a bare `4096.0`. It is a continuation *distance*
  ("far enough for downstream trigger volumes"), not cell-grid math, and does
  not participate in the Z-up→Y-up flip — magic-number hygiene at most, and
  `/audit-tech-debt`'s territory, not a coordinate mapping regression.
- Winding: `crates/nif/src/blocks/strip.rs::destrip` is the single de-strip
  implementation; odd triangles swap the LAST two (`strip[i-2], strip[i],
  strip[i-1]`) = OpenGL/Vulkan CCW. All three former copies delegate
  (`blocks/skin.rs:312`, `blocks/tri_shape/ni_tri_shape.rs:628`,
  `import/collision/shape.rs:655`). No D3D first-two variant anywhere.
- REFR Euler: `euler_zup_to_quat_yup_refr` (byroredux/src/cell_loader/euler.rs)
  is still the single dispatcher; the only non-test consumers are the REFR spawn
  path and `cell_loader/placement_lod.rs:173`, which calls the dispatcher rather
  than re-deriving. `crates/nif/src/anim/keys.rs:125` calls the canonical
  `euler_zup_to_quat_yup` directly (correct — animation keys are not REFRs and
  must not honour the diagnostic mode override). No caller hardcodes a mode.
- Ragdoll extract (`crates/nif/src/import/collision/ragdoll.rs`, -37/+37 this
  window) still emits Y-up via the shared helpers — see D4.

---

## Dimension 2: NIFAL — canonical NIF→ECS translation contract (mapping shape)

Findings: 0 (the one new NIFAL-slice finding is filed under D3, which owns the
texture/material boundary).

Re-traced against `docs/engine/nifal.md` §2 at HEAD.

- **Walker parity.** The window's biggest NIFAL delta is the particle slice
  (#2610 effect-shader payload, #3344 `max_particles`, #3329 sequence-derived
  emitter rate). Checked the historical #2206 failure shape — a field added to
  `walk_node_hierarchical` but not `walk_node_flat`: **both** walkers emit
  `effect_shader` and `max_particles` (`crates/nif/src/import/walk/mod.rs:618/626`
  hierarchical, `:1574/1579` flat), and both consumers apply them
  (`byroredux/src/scene/nif_loader.rs:619/628`,
  `byroredux/src/cell_loader/spawn.rs:1070/1077`) through the shared helpers
  `systems::particle::apply_emitter_params` and
  `cell_loader::pack_effect_shader_flags`. No parity gap.
- **`Material` single-producer.** Enumerated every production `MeshHandle`
  insertion in the tree (8 sites): `scene/nif_loader.rs:996`,
  `cell_loader/spawn/mesh_instance.rs:821`, `cell_loader/placement_lod.rs:589`
  (all three → `material_translate::translate_material`);
  `cell_loader/terrain.rs:665`, `terrain_lod.rs:870`, `terrain_lod_btr.rs:368`,
  `object_lod.rs:399` (all four → `translate_texture_only_material`, the
  declared second helper inside the same module, which owns no scalar literals);
  and `cell_loader/water.rs:481/813`, which insert `WaterPlane` + `WaterMaterial`
  but **no** canonical `Material`. The `every_exterior_spawner_inserts_a_
  boundary_material` harness enumerates five spawners (#3336 added the fifth) and
  does not list `water.rs`.
  **Correction, and deferral.** My first pass recorded the water sites as exempt
  on the reasoning that the water pipeline draws them. That is wrong:
  `byroredux/src/render/water.rs:111-138` looks up an **already-emitted**
  `DrawCommand` and flips `is_water`, so the ESM water planes do traverse
  `collect_static_mesh_draws` and land in the no-`Material` arm. The sibling
  `/audit-nifal` sweep of the same run found and owns this
  (**NIFAL-2026-08-30-D1-02**, LOW, `AUDIT_NIFAL_2026-08-30.md:127-140`), with
  the impact bounded to the RT/secondary path because `water.frag` never reads
  `mat.roughness`/`mat.metalness`. Not re-filed here; recorded so this report's
  own enumeration is not left standing as a contradicting clean verdict.
- **Collision source axis has zero game branches** — `grep` for
  `GameKind::|game ==|bs_version >=` across `crates/nif/src/import/collision/`
  returns nothing (see D4).
- **`ImportedTextureEffect` / `bs_lod_cutoffs` / `lod_group` / `bs_sub_index` /
  `BSInvMarker` / `NiSwitchNode` identity** remain the recorded parked set;
  none moved this window. Not re-filed.

DISMISSED CANDIDATE (stale premise): `pack_effect_shader_flags` is assigned at
two call sites rather than from inside `apply_emitter_params`. It is a single
shared translate function called twice, which the contract permits ("exactly one
`translate()` boundary", not "exactly one call site"). No drift possible.

---

## Dimension 3: Material translation boundary (NIFAL reference slice)

Findings: 1 (LOW) kept. 1 MEDIUM independently derived but **deferred as a duplicate** —
see the arbitration note under D3-01 below.

### Clean axes re-verified

- `translate_material` (`byroredux/src/material_translate.rs:456`) remains the
  sole populated-`Material` producer for shader-property/BGSM content; the
  declared sibling `translate_texture_only_material` (`:696`) owns the four
  no-source-record exterior populations and no scalar literals of its own.
  `byroredux/src/cornell.rs` is the synthetic RT test scene, out of the game
  translation path, and it routes its one real translation through
  `translate_material` (`:1994`).
- `metalness`/`roughness` are still plain resolved `f32` filled from a NaN
  sentinel by `Material::resolve_pbr`; the deleted `Option`-override +
  render-time `classify_pbr` path has not reappeared (only the explanatory
  comment block at `render/static_meshes.rs:410-445` survives).
- **Prior sweep's LC-2026-08-27-D7-01 (MEDIUM) verified FIXED** by #3530
  (`19813460`). Traced end-to-end: `legacy_properties.rs:275`
  (`APPLY_HILIGHT2` → `parallax_map = normal`, `parallax_height_in_alpha`) →
  `import/types.rs:581` → `material_translate.rs:586` →
  `core/.../material.rs:425` → `render/static_meshes.rs:306-310`
  (`PARALLAX_ALPHA_HEIGHT_BIT`) → `GpuMaterial.parallax_map_index` (hashed at
  `vulkan/material.rs:1040`) → both shader consumers mask the bit before
  sampling (`triangle.frag:228/1569`, `include/ray_hit.glsl:296-298`,
  `include/material_sampling.glsl:49-50`). No unmasked read anywhere.

### Deferred duplicate → `NIFAL-2026-08-30-D8-01` (derived here first as "D3-01"; the ID is reused below for the finding this report owns)

Derived independently this sweep, before the sibling report was read; the two
analyses agree on the mechanism, the location and the severity (MEDIUM). Per the
skill's own rule — *"when a finding is about one layer's internals, file it there
and keep the mapping-shape observation here"* — this belongs to `/audit-nifal`,
which owns the texture-role slice. **It is not counted in this report's totals.**
The independent derivation is corroboration, and the reasoning is kept below
because it reaches the same conclusion by a different route (from the #2695
two-table history rather than from the tier matrix).

The mapping-shape observation this report does keep: `slot_role.rs`'s module
header still asserts *"Both sites now call `slot_to_role`"* as the statement of
the #2695 fix. That sentence is what makes the gap invisible — it is true of the
primary role and false of the colocated one, and the header is where an auditor
checking the two-table invariant stops reading.

<details><summary>Independent derivation (retained for corroboration)</summary>


`crates/nif/src/import/material/slot_role.rs` exists because the slot→role
table used to live twice (importer + REFR texture overlay) and disagreed on
four slots (#2695). Its module header states the fix as *"Both sites now call
`slot_to_role`."*

#3458 (`d5a8c36c`) then added a **second** role for one slot:
`slot_to_colocated_role` (`slot_role.rs:264-277`) returns
`TextureRole::LightingMask` for Skyrim/Starfield slot 2 when the shader type is
the tint family (FaceTint/SkinTint/HairTint) **and** SLSF2 `Soft_Lighting` or
`Rim_Lighting` is set — because `slot_to_role` returns `Tint` first and stops,
so the soft-lighting gate crossed the boundary while its mask did not
(measured: 4,054 of 8,058 vanilla soft-lighting properties).

That second binding was wired into **one** consumer only:

- importer — `crates/nif/src/import/material/dedicated_shader.rs:252-263`
  calls `slot_to_colocated_role` after the primary `dest` fill. ✅
- REFR overlay — `byroredux/src/cell_loader/spawn/mesh_instance.rs:197-256`
  builds the same `TextureSlotContext` (including `soft_lighting` /
  `rim_lighting`, `:193-194`) but its gate is a bare primary-role equality:

  ```rust
  let pick = |slot: u32, raw: Option<FixedString>, role: TextureRole| {
      raw.filter(|_| slot_to_role(slot_context, slot) == Some(role))
  };
  …
  (textures.lighting_mask, sources.lighting_mask) = resolve_effective(
      ov.and_then(|o| pick(2, o.glow, TextureRole::LightingMask)),
      mesh.material.textures.lighting_mask,
  );
  ```

  On the tint family `slot_to_role(ctx, 2)` is `Some(Tint)` (`slot_role.rs:297-301`),
  never `Some(LightingMask)`, so `pick` yields `None` and `resolve_effective`
  falls back to the mesh's own texture. There is no
  `slot_to_colocated_role` call anywhere under `byroredux/`.

**Consequence.** For a Skyrim/Starfield tint-family mesh with `Soft_Lighting`
or `Rim_Lighting` set, placed on a REFR carrying an XATO / XTNM / XTXR slot-2
override: `MaterialTextureSet.tint` takes the *overridden* texture while
`MaterialTextureSet.lighting_mask` keeps the *base mesh's* texture — two
canonical roles that #3458 established are, by construction, one and the same
texture now diverge. `triangle.frag`'s soft-lighting lobe is then modulated by
a mask that belongs to a different placement variant. Non-tint shader types are
unaffected (their slot 2 resolves to `LightingMask` as the primary role, so
`pick` works).

Structurally this is the exact class #2695 closed: a role decision made at the
boundary that a second consumer re-derives incompletely, where a fix to one
side silently fails to reach the other.

- **Severity**: MEDIUM. Per `_audit-severity.md`, a translatable input silently
  dropped at the NIFAL boundary is MEDIUM; it does not remove visible content
  (the mask degrades to the base variant's, not to nothing), so no escalation.
- **Confidence**: CERTAIN on the code path; the *content* population (tint-family
  meshes on REFRs carrying slot-2 overrides) is unmeasured — no game archive is
  mounted this run.
- **Suggested fix**: give `pick` a colocation-aware form, e.g. gate on
  `slot_to_role(ctx, slot) == Some(role) || slot_to_colocated_role(ctx, slot) == Some(role)`,
  so the overlay honours the same two-role contract the importer does. A pin in
  `mesh_instance.rs`'s test module asserting slot-2 overlay routing on a
  `SKIN_TINT` + `soft_lighting` context would keep the two sites in lockstep.

</details>

### LC-2026-08-30-D3-01 (LOW) — `slot_role.rs`'s module header and its `canonical_shader_type` doc give opposite answers on whether Starfield content reaches this table

Two passages 130 lines apart in the same file:

- header, `slot_role.rs:17-23`: *"Starfield and FO76 `BSGeometry` materials
  **deliberately do not enter this table** … A **zero Starfield hit** here is
  therefore an explicit format boundary, not an unmeasured routing gap."*
- `canonical_shader_type` doc, `slot_role.rs:141-153` (added by #3364,
  `d9d2d16a`): *"a Starfield FaceTint (3) **reached the slot table** as Skyrim
  Parallax and bound the head's detail map as a POM height field — the exact
  failure #2694 fixed for Skyrim."*

Both cannot be true. The upstream issue (#3364) was filed **LOW / PLAUSIBLE,
"code-read only — no Starfield install on this machine to census"**, and its
consequence paragraph is explicitly conditional (*"for a Starfield type-3
property **with a `BSShaderTextureSet`**"*) — the very thing the header asserts
does not exist. The fix's doc dropped that conditional and states the misroute
as observed fact.

The file now carries nine live `TextureSlotLayout::Starfield` match arms
(`:180, :269, :297, :334, :350, :354, :389, :403` …). Read from the header they
are unreachable code a cleanup pass could legitimately delete; read from
`canonical_shader_type` they are a shipped rendering fix. Nothing in the tree
resolves it — `record_unrouted_texture_slot`'s counters are runtime-only and
there is no checked-in Starfield `BSShaderTextureSet` occupancy figure
comparable to the Skyrim ones the rest of the file cites (3158/3158,
1616/1664, …).

- **Severity**: LOW (doc correctness / audit-reference integrity; no runtime
  behaviour is wrong either way).
- **Confidence**: CERTAIN — both passages quoted verbatim from HEAD.
- **Suggested fix**: settle it with the same kind of census the rest of the file
  uses (count Starfield `BSShaderTextureSet` blocks with a populated slot in
  `MeshesPatch.ba2` / `Meshes01.ba2`) and rewrite whichever passage the number
  falsifies. If the count really is zero, keep the arms but label them
  forward-compat, and downgrade #3364's narration to the conditional it was
  filed as.

---

## Dimension 4: PHYSAL — per-game Havok articulation → solver (source axis)

Findings: 0 kept. 1 MEDIUM independently derived but **deferred as a duplicate** —
see the arbitration note below.

### Clean axes re-verified

- **The seam is still only the CInfo decode.** `grep` for
  `GameKind::|game ==|bs_version|bsver` across `crates/nif/src/import/collision/`
  returns **zero** hits — `extract_ragdoll` switches on `BhkConstraintData`, never
  on game. The one era test in the whole path is
  `constraints.rs:434` (`stream.bsver() <= bsver::NI_BS_LTE_16`), documented as a
  bsver test matching the sibling `rigid_body.rs` gate (#1608).
- **#3330 widened the seam correctly, not sideways.** The new
  `LimitedHingeCInfo::parse_hinge_fo3` / `parse_hinge_oblivion` are a *third*
  pair of era arms on the existing typed decoder, dispatched from the same four
  places (bare Oblivion, Oblivion malleable inner, bare FO3+, FO3+ malleable
  inner) — no new joint kind, no solver change. Verified both layouts field-for-
  field against `/mnt/data/src/reference/nifxml/nif.xml:2447-2464`: Oblivion
  (`until="20.0.0.5"`) is `Pivot A, Perp A1, Perp A2, Pivot B, Axis B` = 5×Vec4
  = 80 B; FO3+ (`since="20.2.0.7"`) is `Axis A, Perp A1, Perp A2, Pivot A, Axis B,
  Perp B1, Perp B2, Pivot B` = 8×Vec4 = 128 B. Both match the code exactly, and
  both byte sizes match the skip-table entries they replaced.
  The Oblivion `Axis A` reconstruction is derived, not guessed: nif.xml states
  `Perp A2 = Axis A × Perp A1`, so `Perp A1 × Perp A2 = Axis A` for an orthonormal
  pair — the identity the code applies.
- **One translate, one build.** `template_from_imported` / `activate_ragdoll`
  (`byroredux/src/ragdoll.rs:83/290`) → `RagdollSpec` → `build_ragdoll`
  (`crates/physics/src/ragdoll.rs:224`). `rapier3d` appears in
  `crates/physics/src/ragdoll.rs` only.
- Documented limitations NOT re-filed: FO4+ `BhkSystemBinary`, phantoms,
  cone+2-plane → per-axis limit approximation, captured-but-unused motors.

### Deferred duplicate → `PHYS-D4-2026-08-30-01` (derived here first as "D4-01")

Derived independently this sweep from the source axis ("does each game's
authoring reach the canonical spec intact?"), before the sibling report was read.
The two analyses agree on mechanism, location, evidence and severity (MEDIUM).
`/audit-physics` owns PHYSAL's articulation slice as of 2026-08-13, so the finding
is theirs. **Not counted in this report's totals.**

One observation this report adds that the sibling does not carry, and that is
mapping-shape rather than solver: `docs/engine/physal.md` — the spec whose leak
inventory this audit is instructed to use as its dedup baseline — contains **zero**
occurrences of "prismatic", "ball and socket" or "stiff spring". The gap has never
been in the inventory, which is why two consecutive suite runs had to rediscover it
from the code. Whoever fixes PHYS-D4-2026-08-30-01 should also add a `bhk*` →
canonical-joint coverage table to `physal.md` §5, mirroring the shape-coverage
table `import/collision/mod.rs` already carries.

<details><summary>Independent derivation (retained for corroboration)</summary>


#3330 (*"undecoded `bhkHinge` / `bhkPrismatic` / breakable edges fragment three
FNV creature ragdolls into disconnected components"*, MEDIUM) shipped a
union-find census over the surfaced joint graph:

```
creatures\protectron\skeleton.nif        12 authored -> 9 surfaced  (2x bhkPrismatic + 1x breakable)
  connected components: 4   [main body] | ["Bip01 Head"] | ["Bip01 Head Dome"] | ["Bip01 Spine Brain"]
creatures\sentryturret\skeleton.nif       3 authored -> 2 surfaced  (1x bhkHingeConstraint)
creatures\minisentryturret\skeleton.nif   3 authored -> 2 surfaced  (1x bhkHingeConstraint)
```

Commit `1ccf1abe` closed it with `Fix #3330` after implementing **only** the
hinge decode. The closing change's own comment says so:

> `crates/nif/src/import/collision/ragdoll.rs:206-211` — *"What remains reaching
> here on vanilla FNV is `creatures\protectron\skeleton.nif`'s two
> `bhkPrismaticConstraint` edges, which need a canonical prismatic joint kind
> that does not exist yet."*

The breakable third is also still dropped: `ragdoll.rs:142-155` warns and
`continue`s, because `BhkBreakableConstraint`'s wrapped CInfo is still
`stream.skip`-ed at parse (#1850, also CLOSED). So the Protectron — the *largest*
of the three cases, and the only one that was 4-way rather than 2-way severed —
is unchanged: its head, head dome and spine-brain each become an independent
free-falling multibody the moment the ragdoll activates.

State of the tracking, checked this run against the live issue list (159 open):
- #3330 CLOSED, #1850 CLOSED, #1539 CLOSED.
- **No open issue** mentions prismatic, ball-and-socket, stiff-spring, breakable
  constraints or Protectron.
- `docs/engine/physal.md` — the spec whose leak inventory this audit is told to
  use as its baseline — contains **zero** occurrences of "prismatic",
  "ball and socket" or "stiff spring". The gap is not in the inventory at all.

This is not "re-filing a documented limitation": the three limitations `physal.md`
does document (FO4+ packed Havok, phantoms, the cone/plane approximation, unused
motors) are each recorded there with a rationale. This one exists only as a code
comment inside the function that drops it, behind a closed issue whose title
still claims it fixed the case.

- **Severity**: MEDIUM. `_audit-severity.md`: a translatable block silently
  dropped by the translation layer is MEDIUM, escalated to HIGH only if it
  removes visible game content. The content is not removed — the limb renders,
  it detaches — and the blast radius is one shipped skeleton, so MEDIUM holds.
- **Confidence**: CERTAIN on the code state (drop sites read at HEAD; the
  hinge-only scope of `1ccf1abe` confirmed from the diff and its own comment).
  The census figures are #3330's, re-quoted, not re-measured — no FNV archive is
  mounted this run.
- **Suggested fix**: two independent pieces, and they should be tracked
  separately rather than under one re-opened umbrella. (a) Add
  `ImportedJointKind::Prismatic` + a `bhkPrismaticConstraintCInfo` decode
  (nif.xml `:2474`, both eras) and a Rapier prismatic arm in `build_ragdoll`.
  (b) Retain `BhkBreakableConstraint`'s wrapped CInfo at parse instead of
  skipping it, then route the inner 7/2/1 through the existing arms. Either way,
  record both in `physal.md` §5's per-concern inventory so the next sweep has a
  baseline, and add a `bhk*` → canonical-joint coverage table there the way
  `import/collision/mod.rs` already carries one for shapes.

</details>

---

## Dimension 5: EXAL / WATAL — per-game exterior environment → renderer & solver

Findings: 0 new. 2 Existing (both already filed by the prior sweep, both still open).

### Clean axes re-verified

- **Single exterior boundary.** `byroredux/src/env_translate.rs` is still the
  only production construction site for `SkyParamsRes` (`:1068`, `:1286`),
  `WeatherDataRes` (`:1199`, `:1333`) and the ESM `WaterMaterial`
  (`resolve_water_material`). Every other `SkyParamsRes {` / `WeatherDataRes {` /
  `WaterMaterial {` literal in the tree is inside a `#[cfg(test)]` module —
  verified by comparing each hit's line against its file's `#[cfg(test)]` offset
  (`systems/character.rs:1115`, `systems/water.rs:606`, `systems/weather.rs:860`,
  `scene/world_setup.rs`). Mesh water is the declared NIFAL/WATAL seam in
  `material_translate.rs:166` and is documented as such.
- **No render-time fallback.** The plugin-less case routes through the canonical
  `procedural_fallback_{sky,weather,cell_lighting}` constructors, funnelled by
  the single `insert_procedural_fallback_resources`
  (`scene/world_setup.rs:712-722`); even `cornell.rs` (the synthetic RT scene)
  consumes them rather than inlining literals (`cornell.rs:1403-1404`).
  `systems/weather.rs:283` re-seeds through the same constructor.
- **Exterior GameKind branches.** One `game == GameKind::Skyrim` outside the
  named tables (`scene/world_setup.rs:962`) — that is the Existing finding below.
  `cell_loader/exterior.rs`, `streaming*.rs`, `cell_loader/water.rs` and
  `cell_loader/terrain.rs` contain **zero**.
- **DISMISSED CANDIDATE (premise checked, fails).** The six per-game LOD
  predicates spread across four `cell_loader` files initially read as
  "scattered `if game == …` exterior logic". They are not: `combined_lod_supported`
  and `legacy_landscape_lod_supported` (`lod_support.rs:67/78`) both *derive*
  from `env_translate::terrain_lod_layout`, the EXAL table; `object_lod_scheme`
  (`object_lod.rs:519`), `placement_lod_supported` (`placement_lod.rs:314`) and
  `LodBandLadder::for_game/for_terrain_game/for_object_game`
  (`lod_bands.rs:141/155/177`) are each one named GameVariant table answering a
  distinct question, each carrying its archive evidence and each pinned by
  tests. Cross-checked for mutual consistency (Oblivion: OblivionLegacy terrain
  / no object scheme / placement scheme yes / no band ladder → the fixed
  synthesized ring, asserted at `terrain_lod.rs:1122-1127`). No drift.

### Existing (deduped, not re-filed)

- **#3534** — LC-2026-08-27-D5-01: the skill's own Dimension 5 text. Re-checked:
  the FO3/FNV distant-object-LOD half **has** been corrected in the skill (it now
  carries the #3321 re-derivation in full). The **VWD half is still stale**: the
  skill says the flag "has zero consumers", but `VisibleWhenDistant` is stamped
  at spawn (`cell_loader/references/synth_child.rs:691/753`) and read every
  reconcile by `streaming_helpers::resident_vwd_refr_cells` (`:183/313`) feeding
  `LodCoverageStats::vwd_full_model_overlaps`. The *substance* of the skill's
  claim survives — the **cull** is still not wired — but the reason has changed
  from "unwired" to "deliberately deferred pending live validation", tracked by
  #3307 and documented at `components.rs:225-241`. Reported here per the "trust
  the code over the skill" rule; the issue is already open.
- **#3536** — LC-2026-08-27-D5-02: the `game == GameKind::Skyrim` branch with two
  hardcoded vanilla FormIDs. Confirmed unchanged at HEAD,
  `byroredux/src/scene/world_setup.rs:962-969`
  (`materialize_scene_actor_alias_stubs(..., 0x0003_372B, 0x000B_ECD4)`).

---

## Dimension 6: Per-game translation-survey gaps (Pattern A/B/C)

Findings: 1 (MEDIUM). 1 Existing (#3537).

### Measured state of the three patterns at HEAD

- **Pattern A — clean, and clean in a way the survey does not describe.**
  A grep for a *numeric* BSVER comparison in production
  (`^[^/]*\b(bs_version|bsver)\b *(>=|>|<=|<|==|!=) *[0-9]+` across `crates/` +
  `byroredux/`, excluding tests/examples) returns **two hits, both inside
  `#[cfg(test)]` assertion messages**. Every production gate reads a named
  constant out of `crate::version::bsver` with an nif.xml citation beside it —
  spot-verified at `shader.rs:83` (`FO3_REFRACTION`), `shader.rs:91`
  (`FO3_PARALLAX`), `node.rs:111` (`FALLOUT4`), `node.rs:894/936`
  (`SF_FORM_ID` / `SF_WEAK_REF_GAP`), `particle.rs:370/391/415`
  (`NI_BS_LTE_16` / `FO3_FNV` / `FO76`), `extra_data.rs:1133` (`FO3_FNV`),
  `constraints.rs:434` (`NI_BS_LTE_16`), `collision_object.rs:53`.
- **Pattern B — realised as namespaced constants + block-type dispatch**, per
  `nifal.md`'s shader-flags entry; `triangle.frag` and every other shader carry
  zero `game ==`, re-confirmed by grep over `crates/renderer/shaders/`.
- **Pattern C — the variant-enum shapes exist** where the survey asked for them
  (`ShaderFlags` three-variant view, XCLL game-size gate, `BsLightingShader`
  three-variant parse).

### LC-2026-08-30-D6-01 (MEDIUM) — `per-game-translation-survey.md`'s Pattern A prescription is the exact inverse of the settled raw-bsver doctrine, and this audit's own Dimension 6 points auditors at it

`docs/engine/per-game-translation-survey.md` §5 "Pattern A: hardcoded BSVER
constants where a helper exists" states:

> *"`NifVariant` exposes `has_effects_list`, `has_properties_list`,
> `has_material_crc`, `has_shader_alpha_refs`, `uses_bs_tri_shape`,
> `uses_fo4_shader_flags`, `uses_fo76_shader_flags` — and the parser calls
> `stream.bsver() < 130` or `stream.bsver() > 34` directly instead. Fix is
> mechanical: every raw `bsver()` comparison gets rewritten to call the named
> helper … **Highest-leverage starter** … a clippy lint or custom test can
> enforce 'no raw `bsver()` comparison outside `version.rs`'."*

Against HEAD, every clause of that is false or inverted:

1. **Those helpers do not exist.** `uses_fo4_shader_flags`,
   `uses_fo76_shader_flags` and `has_dynamic_effect_fields` (cited in §4.1)
   have **zero** occurrences anywhere in the tree. The other four survive only
   as *prose in comments explaining why the call site does not use them*
   (`node.rs:107`, `base.rs:101`, `ni_tri_shape.rs:129/368`,
   `shape_compound_tests.rs:29`).
2. **They were removed on purpose, and the reasoning is recorded in the file
   the survey points at.** `crates/nif/src/version.rs:699-718`:
   *"#938 … removed three predicates; #1511 removed six more; #1840 removed
   seven more (`has_material_crc`, `has_properties_list`, `avobject_flags_u32`,
   `has_shader_alpha_refs`, `has_effects_list`, `uses_bs_tri_shape`,
   `has_culling_mode`); #1897 removed the last survivor … Keeping a
   call-site-less predicate as an 'approved helper' alongside the raw-bsver path
   was an **architectural foot-gun**: a contributor adopting one … reintroduces
   the one-bsver-step transitional-export mis-parse those call sites were fixed
   to avoid. **No feature-flag predicates remain on `NifVariant` — this doctrine
   is fully enforced now.**"*
3. **The bare-comparison problem it describes is already solved** — by the
   opposite move (named *constants*, not named *predicates*), as measured above.
4. **The premise cascades into three more sections.**
   - §4.1's 14-row table is headed *"Hardcoded threshold constants scattered
     across 30+ sites"* with a *"Helper available? Yes — bypassed"* column.
     Every cited site now reads a named constant; the column names APIs that no
     longer exist.
   - §4.1's closing paragraph calls `BSLightingShaderProperty::parse` *"the
     textbook candidate for splitting into `BsLightingShaderVariant::{Skyrim,
     Fo4, Fo76Plus}`"*. That split **landed**: `shader.rs:903 parse_skyrim`,
     `:1009 parse_fo4`, `:1159 parse_fo76_plus`, plus
     `parse_shader_type_data_fo4` / `_fo76`.
   - §8 task 5 ("Migrate raw `bsver()` comparisons to `NifVariant` helpers …
     add a clippy lint to prevent regression") and §9's progress row for it
     (*"landed `2bd447d5` — 6 sites migrated, 3 new helpers added"*) are a
     record of work that #1840 / #1897 then deliberately reverted, with no note.
     §9 also still records task 2 as *"deferred to a dedicated session"* —
     it shipped.

**Why this is not merely doc rot.** This audit's own skill file
(`.claude/commands/audit-legacy-compat/SKILL.md`, Dimension 6) instructs the
auditor to *"audit by the survey's three leak patterns: Pattern A — hardcoded
BSVER constants where a named helper already exists but call sites bypass it
(`per-game-translation-survey.md` §5 Pattern A)"*. An auditor following that
literally searches for a class of leak that was designed out, and — worse —
the survey's stated "highest-leverage starter" is an instruction to reintroduce
an abstraction the tree records as a foot-gun that causes a mis-parse. That is
an actionable wrong instruction sitting in the engine's own design docs, not a
stale number.

- **Severity**: MEDIUM. The `_audit-severity.md` LOW bucket is dead code /
  missing docs / naming / test-coverage gaps; an architectural prescription that
  is the documented inverse of the enforced doctrine is not in it, and the
  decision tree's terminal rule is "Otherwise → MEDIUM". No runtime behaviour is
  wrong today, which is why it is not HIGH.
- **Confidence**: CERTAIN. Every claim above is a grep or a verbatim quote from
  HEAD; the removal rationale is in the codebase, signed by four issue numbers.
- **Suggested fix**: rewrite §5 Pattern A to state the doctrine the tree
  actually enforces — *raw `stream.bsver()` compared against a named
  `version::bsver::*` constant, with the nif.xml `vercond` quoted at the site;
  no `NifVariant` feature-flag predicates* — and cite `version.rs:699-718` for
  why. Strike the seven dead helper names from §4.1's "Helper available?"
  column and re-title the table (the constants are named, the *thresholds* are
  what is scattered). Mark §9's task-5 row REVERTED (#1840/#1897) and its
  task-2 row LANDED. Then update the skill's Dimension 6 Pattern A bullet to
  match, or it will keep re-seeding the same misdirection every sweep.

### Existing (deduped, not re-filed)

- **#3537** — LC-2026-08-27-D6-01: §7 item 7 still restates, unmarked, the
  `classify_pbr_keyword`-collapses-everything claim that §2 retracts eight
  lines into itself. Confirmed unchanged (`survey.md:56-68` vs `:427-430`).
  The fix for D6-01 above should be batched with it — they are the same
  document and the same class of failure.

---

## Dimension 7: Subsystem coverage vs legacy

Findings: 1 (LOW). Prior sweep's D7-01 (MEDIUM) verified FIXED — see D3.

### Property → pipeline mapping, re-walked field by field

This is the bullet that produced last sweep's MEDIUM, so it was redone from the
struct definitions rather than from the prior report.

| Legacy property | Authored fields | Reaches the engine? |
|---|---|---|
| `NiZBufferProperty` | `z_test_enabled`, `z_write_enabled`, `z_function` | **all three** — `legacy_properties.rs:127-133` → `MaterialInfo` → `material_translate.rs:496-498` → `Material` → `render/static_meshes.rs:388` → `DrawCommand.z_test/z_write/z_function` (`vulkan/context/mod.rs:257-264`) |
| `NiStencilProperty` | `draw_mode` (+ 6 stencil-test fields) | `draw_mode` → `is_two_sided()` → `TwoSided`. The stencil test itself is unmapped — **not a finding**: Oblivion's only use was stencil shadow volumes, which the RT shadow path replaces, so it is a reference-only technique per the standing rule |
| `NiVertexColorProperty` | `vertex_mode`, `lighting_mode` | both — `material/mod.rs:378 from_property` decodes the pair into a `VertexColorMode` |
| `NiFlagProperty` ×4 | `NiSpecularProperty` / `NiWireframeProperty` / `NiShadeProperty` / `NiDitherProperty` bit 0 | all four have arms at `legacy_properties.rs:759-789`; wireframe + flat-shading reach `Material` and the render path (`static_meshes.rs:674/679`) |
| `NiTexturingProperty` | apply mode, 8 slots + 4 decals, per-slot clamp/filter/UV-transform | now complete for the render-affecting set — `apply_mode` was closed by #3530 and `clamp_mode` by #3516 this window |
| `NiFogProperty` | — | deliberate documented skip (#1224); not re-filed |
| `NiAlphaProperty` | blend, src/dst blend, test, threshold, test func, **No Sorter**, Clone Unique, Editor Threshold | six of eight — see the finding below |

DISMISSED CANDIDATES (premise checked, dropped as reference-only):
`Bump Map Luma Scale` / `Luma Offset` / `Bump Map Matrix` are byte-consumed and
discarded at `properties.rs:294-302`. They parameterise the fixed-function
DX7 emboss-bump path; Oblivion's `bump_texture` slot actually carries a
tangent-space `_n.dds`, which the importer routes to `normal_map`
(`legacy_properties.rs:190`), so the emboss scalars have no meaning under
normal mapping. Same verdict for the `Num Shader Textures` / `ShaderTexDesc`
array (`properties.rs:403-435`) — custom-shader map bindings for a shader
system Redux does not implement.

### Other D7 axes

- **Transform fidelity.** `NiTransform` in Gamebryo is
  `NiMatrix3 m_Rotate; NiPoint3 m_Translate; float m_fScale;` — verified at
  `/mnt/data/src/reference/gamebryo-v32/Include/NiTransform.h:25-27`. Redux's
  `Transform` (Quat + Vec3 + `f32`) is therefore a *faithful* mapping, not a
  lossy one. **Skill discrepancy, reported per instruction:** this audit's own
  Dimension 7 bullet says *"flag fidelity gaps (non-uniform scale is collapsed
  to uniform `f32`)"* — the legacy type has no non-uniform scale to collapse.
  The only real loss is scale baked into a `NiMatrix3`, and that is already
  filed (#3532, 1 hit in 642,589 vanilla matrices) — the skill bullet should be
  reworded to point there instead of implying a per-node scale gap.
- **String interning.** `StringPool::intern` folds to ASCII lowercase
  (`crates/core/src/string/mod.rs:56/86`) and every bone-name → entity lookup
  routes through `crate::name_lookup::get_case_insensitive`
  (`scene/nif_loader.rs:1196/1229`, `ragdoll.rs:101/105`), pinned by
  `case_mismatched_bone_name_still_resolves` (`ragdoll.rs:1614`). No interning
  gap can break skinning or PHYSAL bone binding.
- **Animation model.** Parked set unchanged: per-light ambient channels
  (no consumer) and morph-weight GPU blending (#2221, sink exists). Not
  re-filed.

### LC-2026-08-30-D7-01 (LOW) — `AlphaFlags` bit 13 `No Sorter` is parsed into the flags word and never decoded, so the engine back-to-front-sorts every alpha-over draw including the ones the author opted out of

nif.xml `AlphaFlags` (`nif.xml:1554-1563`) defines eight members. `apply_alpha_flags`
(`crates/nif/src/import/material/mod.rs:1517-1542`) decodes five of them
(`Alpha Blend` 0x0001, `Source Blend Mode` 0x001E, `Destination Blend Mode`
0x01E0, `Alpha Test` 0x0200, `Test Func` 0x1C00) plus the `Threshold` byte.

`No Sorter` (bit 13, mask 0x2000) is never read — a grep for `0x2000` /
`no_sorter` / `NoSorter` across `crates/nif`, `crates/core` and `byroredux`
returns only unrelated shader-flag constants. It is Gamebryo's per-property
instruction to `NiAlphaAccumulator` to draw the shape in accumulation order
rather than depth-sorted.

Redux implements exactly the ordering this flag opts out of, unconditionally:
`byroredux/src/render/mod.rs:508` puts `!cmd.sort_depth` in **slot 3** of the
alpha-over sort key — ahead of render layer, two-sidedness, blend factors,
depth state and mesh — i.e. a global back-to-front order, with the module doc
(`:382-387`, `:490-503`) recording that this ordering was chosen deliberately
for correctness at a measured batching cost (FNV `FreesideAtomicWrangler`,
25 → 8 GPU calls). There is no per-draw exemption, so a shape whose author
disabled the sorter is sorted anyway.

This is not a "reference-only legacy shading param": draw ordering for
alpha-over is a behaviour Redux implements on purpose, and this is the one
authored control over it that the mapping ignores.

- **Severity**: LOW as filed. The decision tree would put a visual-ordering
  artifact at MEDIUM, but the premise that any shipped content sets the bit is
  **unverified** — no game archive is mounted this run (`/media/matias` holds
  only `ROMS` and a `Videos` volume), so no occupancy census was possible. If a
  census finds authored occupancy, this escalates to MEDIUM.
- **Confidence**: CERTAIN that the bit is unread and that the sort is
  unconditional; PLAUSIBLE that it changes any vanilla frame.
- **Suggested fix (in order)**: (1) census `flags & 0x2000` across the Oblivion
  / FO3 / FNV mesh archives with a throwaway `crates/nif/examples` probe — the
  same method #3530 used for `APPLY_HILIGHT2` (1,433 hits / 741 meshes) and
  #3516 used for `clamp_mode` (2236/2258); (2) if non-zero, surface it as
  `MaterialInfo.no_sorter` → `Material` → `DrawCommand`, and make slot 3 of the
  alpha-over key `(!no_sorter, !sort_depth)` so opted-out draws keep their
  state-clustered order; (3) if zero, record the measurement beside
  `apply_alpha_flags` the way `properties.rs` records its other
  deliberate-skip decisions, so the next sweep does not re-derive it.
  `Clone Unique` (0x4000) and `Editor Alpha Threshold` (0x8000) are
  editor/instancing hints with no render-state meaning and need no such note.

---

## Deduplication

### Against open GitHub issues (159 open, fetched live this run)

| Issue | Status at HEAD | Action |
|---|---|---|
| #3534 (LC-2026-08-27-D5-01, skill Dimension 5 text) | **Half fixed.** The FO3/FNV distant-object-LOD passage now carries the #3321 re-derivation in full. The VWD passage is still stale — see the skill-discrepancy list above. | Not re-filed; the open issue covers it. |
| #3536 (LC-2026-08-27-D5-02, `game == Skyrim` + two hardcoded FormIDs) | **Unchanged** — `scene/world_setup.rs:962-969`. | Not re-filed. |
| #3537 (LC-2026-08-27-D6-01, survey §7 restates the retracted claim) | **Unchanged** — `survey.md:427-430` vs the §2 retraction at `:56-68`. | Not re-filed; **should be batched with LC-2026-08-30-D6-01** — same document, same failure class. |
| #3532 (LC-2026-08-27-D1-01, `#2456` SVD classifier) | Unchanged; owns the only real transform-fidelity loss. | Referenced from D7, not re-filed. |
| #3187 (`apply_slot_swap` is a third slot table) | Unchanged. Distinct from LC-D3-01: #3187 is about the ESM-side XTXR index table, D3-01 is about the spawn-side `pick` gate. | Not merged. |
| #3307 (per-cell object-LOD segments / the VWD cull) | Unchanged; the documented reason the VWD cull is deferred. | Referenced, not re-filed. |
| #2221 (morph-weight GPU consumer), #2697 (`supplemental_texture_indices`) | Unchanged parked items. | Not re-filed. |

### Against the 15 sibling reports of this same suite run

| Sibling finding | Relationship |
|---|---|
| `NIFAL-2026-08-30-D8-01` | **Duplicate of my D3-01.** Independently derived; deferred to `/audit-nifal`, which owns the texture-role slice. |
| `PHYS-D4-2026-08-30-01` | **Duplicate of my D4-01.** Independently derived; deferred to `/audit-physics`, which owns PHYSAL's articulation slice. |
| `NIFAL-2026-08-30-D1-02` (ESM water planes outside the #2444 guard) | **Corrects my D2 first pass.** I had recorded the water spawners as exempt; they are not. Their finding, their fix. Recorded in D2 so this report does not leave a contradicting clean verdict standing. |
| `NIF-2026-08-30-D5-01` (four constraint types on the `is_havok_constraint_stub` list) | Adjacent to D4's seam check, not overlapping — theirs is about the drift-detector's stub list, mine was about the decode seam's width. |
| `NIF-2026-08-30-D3-03`, `NIF-2026-08-30-D2-02` | Doc-rot / dead-constant findings in `nif-parser.md` and `version.rs`. Same *class* as D6-01 but different documents; D6-01's subject (`per-game-translation-survey.md`) appears in no sibling report. |
| All others (`AUDIT_{AUDIO,CHARACTER,CONCURRENCY,ECS,ESM,PERFORMANCE,RENDERER,SAFETY,SAVE,SCRIPTING,SPEEDTREE,UI}_2026-08-30.md`) | Checked for `No Sorter` / `0x2000`, `per-game-translation-survey` §5 Pattern A, and the `slot_role.rs` Starfield-scope contradiction. No overlap with D3-01, D6-01 or D7-01. |

### Against the layer specs

`nifal.md` §2's parked inventory (`ImportedTextureEffect`, `bs_lod_cutoffs`,
`lod_group`, `bs_sub_index`, `BSInvMarker`, `NiSwitchNode` identity, the four
`ImportedNode` fields), its emissive-scale and `NiFogProperty` regression guards,
`exal.md` §5's LOD coverage table and its sun-model guard, and `physal.md`'s four
documented limitations were all treated as baseline and none was re-filed.
`physal.md`'s **absence** of a `bhk*` → canonical-joint coverage inventory is
recorded in D4 as a spec-currency observation attached to the deferred finding,
not as a separate finding.

## Stale candidates investigated and dropped (5)

1. **`byroredux/src/systems/cinematic.rs:326` — bare `4096.0`.** It is a
   continuation *distance* for a cart-route heading, not cell-grid math, and does
   not participate in the Z-up→Y-up flip. Magic-number hygiene at most, and
   `/audit-tech-debt`'s territory. (D1)
2. **`crates/core/src/ecs/components/camera.rs:710` — `const CELL_UNITS = 4096.0`.**
   Inside `#[cfg(test)]` (module opens at `:481`). Naive-grep artefact. (D1)
3. **`pack_effect_shader_flags` called at two sites rather than from inside
   `apply_emitter_params`.** It is one shared translate function called twice,
   which the contract permits; no drift is possible. (D2)
4. **Six per-game LOD predicates spread across four `cell_loader` files.** Read
   initially as "scattered `if game == …` exterior logic". They are not:
   `combined_lod_supported` / `legacy_landscape_lod_supported` both *derive* from
   the EXAL table `env_translate::terrain_lod_layout`, and the other three are
   named GameVariant tables each answering a distinct question, each carrying its
   archive evidence and each pinned by tests. Cross-checked for mutual
   consistency — no drift. (D5)
5. **`NiTexturingProperty`'s `Bump Map Luma Scale` / `Luma Offset` /
   `Bump Map Matrix`, its `ShaderTexDesc` array, and `NiStencilProperty`'s six
   stencil-test fields.** All byte-consumed and discarded, all reference-only:
   the first three parameterise fixed-function emboss bump (Oblivion's bump slot
   actually carries a tangent-space `_n.dds`, which the importer routes to
   `normal_map`), the fourth binds a custom-shader system Redux does not
   implement, and the last drove stencil shadow volumes that the RT shadow path
   replaces. Per the standing rule, an unmapped legacy shading param is not
   automatically a gap. (D7)

## Verification

- **Every finding's premise was re-checked against HEAD**, not inherited. The
  prior sweep's MEDIUM (D7-01) was traced end-to-end through eight files and
  three shaders before being declared fixed; the two duplicates were derived from
  the code before the sibling reports were opened.
- **One of this report's own first-pass conclusions was wrong and is corrected
  in place** (D2, ESM water planes). It is left visible rather than silently
  edited, because the failure mode — reasoning from a module's stated design
  ("the water pipeline draws these") instead of from the call graph — is the same
  one that let #2206 and #2440 survive four sweeps each.
- **No corpus census was possible** (no archives mounted). Two findings
  (LC-D7-01, and the reachability half of the deferred LC-D3-01) have
  occupancy-dependent premises and say so explicitly in their Confidence lines;
  neither is rated above LOW/MEDIUM on unmeasured content.
- No source file, game file, shader, or GitHub issue was modified. No cargo
  command was run and the engine was not launched, per the run's memory
  constraint.

## Summary

Three new findings, all in documentation or in a second consumer of a
one-boundary contract — none in the boundaries themselves. The abstraction
layers held under a 466-file delta that touched all three of them.

The recurring signal across the last three sweeps is that this audit's own
reference material is the weakest link in the area it audits: two of the three
findings here (D6-01, D3-01's retained observation) and two of the three
skill-vs-code discrepancies are cases where a doc or a module header states a
contract that the code no longer honours — and in the D6-01 case, states an
action that the code explicitly documents as harmful. Fixing
`per-game-translation-survey.md` §4.1/§5/§8/§9 and the matching Dimension 6
bullet in the skill should be batched with #3537 and treated as maintenance of
the audit infrastructure, not as documentation polish.
