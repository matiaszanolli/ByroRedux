# Skyrim SE Compatibility Audit — 2026-08-03

**Repo**: `/mnt/data/src/gamebyro-redux` · HEAD `1ae86f62`
**Scope**: 7 dimensions per `.claude/commands/audit-skyrim/SKILL.md` — BSTriShape
packed geometry + SSE skinned reconstruction, `BSLightingShaderProperty` /
`BSEffectShaderProperty` shader-type dispatch, NPC equip + FaceGen (M41),
multi-master load order + TES5 cell-load, BSA v105 (LZ4), specialty NIF
blocks + real-data rendering, NIFAL canonical material translation.
**Prior audit**: `docs/audits/AUDIT_SKYRIM_2026-07-25.md` (9 days earlier,
all-clean). This pass independently re-verified every checklist item against
current code and live data rather than trusting the prior report's citations,
and found one new HIGH-severity regression the prior pass's methodology could
not have caught (see Dimension 1).

## Executive Summary

Skyrim SE remains the renderer's control bench — cell loading, exterior/
interior streaming, multi-master DLC load order, BSA v105 extraction, and the
M41 NPC-equip pipeline are all live and load-bearing on this game's content.
Six of seven dimensions confirm the 2026-07-25 clean bill of health holds,
with only LOW-severity hardening gaps and documentation drift found.

**Dimension 1 is the exception and the headline finding of this audit.**
Every `BSDynamicTriShape` block on Skyrim SE — the block type that carries
**all** NPC FaceGen head/eye/brow/mouth geometry, both the shared
`femalehead.nif`/`malehead.nif` and every per-NPC
`facegendata\facegeom\<plugin>\<formid>.nif` — imports to **zero meshes**.
This was confirmed against real vanilla data (`Skyrim - Meshes0.bsa`): both
`femalehead.nif` and a real NPC head record (`00096559.nif`, 6 shapes) import
0 meshes each, despite parsing cleanly (0 truncated, 0 failures) and despite
all the geometry data (positions, indices, UVs, normals, tangents, skin
weights) already sitting in memory after parse. The M41 equip smoke test
passes because it asserts on entity/draw/`tex.missing` counts, not head
presence, so this has been invisible to the existing regression gate through
at least three prior audits (#571/#621/#946 only added diagnostics, never a
fix). **Net effect: on the renderer's own control-bench cell
(WhiterunBanneredMare), every rendered NPC is headless.** See Dimension 1 for
the full three-site root cause and a concrete three-edit fix.

Everything else — shader-type dispatch, the M41 equip/leveled-list pipeline
(aside from a cheap efficiency regression in the new resumable-assembly
refactor), multi-master FormID remap, BSA v105 extraction, specialty NIF
block dispatch, and the NIFAL material-translation boundary — holds up under
live re-verification (test suites re-run, not cited; two archive/ESM sweeps
re-run against real data; two throwaway live-data probes written and
discarded during this audit).

## Total Findings: 9 (0 CRITICAL / 1 HIGH / 3 MEDIUM / 5 LOW)

## Dimension Findings

### Dimension 1 — BSTriShape Packed Geometry + SSE Skinned Reconstruction

**Method**: every checklist item read against `nif.xml`, then the one
suspicious path confirmed against real vanilla data extracted from
`Skyrim - Meshes0.bsa`.

#### SK-D1-01: Every Skyrim SE `BSDynamicTriShape` imports to zero meshes — all NPC FaceGen head/eye/brow/mouth geometry is dropped
- **Severity**: HIGH
- **Location**: `crates/nif/src/import/mesh/bs_tri_shape.rs:26-37` (reconstruction gate), `crates/nif/src/import/mesh/sse_recon.rs:206-208` (`VF_VERTEX` bail), `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:515-566` (`parse_dynamic`)
- **Status**: NEW (root cause of the diagnostic added under closed #1225; closed #571/#946 only added log lines, never a fix)
- **Description**: `BSDynamicTriShape` ships all Skyrim SE head geometry.
  On this block type, `VF_VERTEX` (bit 0) is **clear** in `vertex_desc` —
  positions live in a trailing `Vector4[]` dynamic array instead of the
  packed vertex buffer, and the triangle list lives only on the sister
  `NiSkinPartition` (`num_triangles == 0` on the block body). Three sites
  interact to drop the whole shape:
  1. `extract_bs_tri_shape` only attempts SSE reconstruction when **both**
     `shape.vertices` and `shape.triangles` are empty; `parse_dynamic` has
     already filled `shape.vertices` from the `Vector4[]` array, so the
     reconstruction path is never reached, and the shape is then dropped for
     having no triangles.
  2. Even if reached, `decode_sse_packed_buffer` bails outright when
     `VF_VERTEX` is clear — exactly the `BSDynamicTriShape` shape — which
     also kills the #638 skin-payload fallback.
  3. `parse_dynamic` discards the `Vector4`'s `w` lane
     (`let _w = ...; // bitangent-x or unused`), which per `nif.xml` is the
     **only** source of `bitangent_x` once `VF_VERTEX` is clear — so even a
     fixed (1)+(2) can't reassemble the tangent basis without this.
- **Evidence** (real vanilla data): `femalehead.nif` — `BSDynamicTriShape
  verts=996 tris=0 desc=0x0046200021000045` → **imported meshes: 0**.
  `facegendata\facegeom\skyrim.esm\00096559.nif` (real NPC head, 6 shapes,
  2797 verts/4234 tris across the whole record) → **imported meshes: 0**.
  Decoded descriptors confirm `VF_VERTEX` is genuinely clear by stride
  arithmetic (declared stride == exactly the non-position field sum) — not a
  misparse. The parser-side breadcrumb already in-tree even says *"mesh will
  silently fail to render at the import boundary"* (fires on all 21,140/
  21,140 vanilla `BSDynamicTriShape` blocks per the live Meshes0 sweep run
  this session — see Dimension 6).
- **Impact**: Skyrim SE NPCs render **headless** — no face, eyes, brows,
  mouth, or hair-base geometry on any actor, on the renderer's own
  control-bench cell (6 named NPCs, WhiterunBanneredMare). Nothing is
  missing from the parser — everything needed is already in memory — only
  the import-boundary plumbing needs to route it.
- **Related**: #559, #157/#341/#571/#621/#946, #638, #1225 (this is its root
  cause), M41.0 (closed only for the FNV kf-era `NiTriShape` path).
- **Suggested Fix**: (1) keep the `Vector4` `w` lane instead of discarding
  it; (2) make `decode_sse_packed_buffer` handle "positions supplied
  externally" (consume 0 bytes for the position quad when `VF_VERTEX` is
  clear) instead of bailing; (3) widen `extract_bs_tri_shape`'s
  reconstruction gate to fire on `shape.triangles.is_empty()` alone, keeping
  the already-parsed `shape.vertices` as the position source. Add a
  real-data regression test pinning `femalehead.nif → 1 mesh, 996 positions,
  5118 indices`.

#### SK-D1-02: `#621`'s `VF_FULL_PRECISION` back-write rests on a false premise and is a no-op on all vanilla content
- **Severity**: LOW
- **Location**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:529-543`
- **Status**: NEW
- **Description**: The comment justifying `shape.vertex_desc |=
  (VF_FULL_PRECISION as u64) << 44` claims the dynamic array "overwrote"
  packed half-precision positions. Measured against real data, a
  `BSDynamicTriShape`'s packed buffer has **no position field at all**
  (`VF_VERTEX` clear) and `VF_FULL_PRECISION` is **already set** in every
  observed descriptor — the `|=` is a no-op on every vanilla block, and the
  rationale is wrong. It also actively misleads a future reader into
  thinking the packed buffer holds positions.
- **Impact**: No runtime effect today; stale/misleading rationale that let
  SK-D1-01 survive three audits.
- **Suggested Fix**: Fold into the SK-D1-01 fix; drop the `|=` (or correct
  the comment) and record the real invariant instead.

**Verified clean** (no findings): `VF_*` flag-bit table vs `nif.xml`
(1:1 match); `half_to_f32` IEEE-754 binary16 decode (hand-traced zero/
subnormal/normal/Inf-NaN classes); `extract_bs_tri_shape` flag-combination
handling, index stride, skin-weight renormalization and bone remap
(apart from the `BSDynamicTriShape` hole above); SSE reconstruction's
Z-up→Y-up conversion and bitangent-as-∂P/∂U tangent routing (confirmed
**not** a magenta/chrome risk for non-dynamic bodies); the
`alpha_property_consumed` cascade (exactly two gate sites, no double- or
missed-application path for skinned Skyrim geometry). One test-coverage gap
noted (not filed as a finding): `alpha_flag_tests.rs` has no case for a
BSLightingShader + `alpha_property_ref` shape or for the
gate-ordering-regresses-loudly scenario.

### Dimension 2 — BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch

**Result: no findings.** Read directly (not cited from the prior report):
Skyrim dispatch (`parse_shader_type_data`) correctly routes all 7
with-trailing-data types (1/5/6/7/11/14/16) and falls through cleanly to
`None` for the other 14 numeric values; FO4's dispatch adds correctly
version-gated deltas (env-map-scale dead-band #1552 already fixed,
SSR bools, skin-tint alpha); FO76's distinct `BSShaderType155` numbering
(4=SkinTint Color4, 5=HairTint Color3) lives in its own function with no
shared match arms — no cross-contamination possible by construction.
`BSEffectShaderProperty` field layout unchanged since the byte-for-byte
`nif.xml` trace in the prior audit. **`MAT_FLAG_PBR_BSDF` regression guard
independently re-derived from scratch**: `ImportedMaterial::is_pbr` defaults
`false` on both the struct default and the inline-NIF-shader-property path
(the only path any Skyrim SE material takes, since Skyrim ships no external
BGSM/BGEM/.mat sidecars); the only sites that ever set it `true` are inside
`merge_external_material` (gated on a resolved external material file
existing on disk) — so the Disney BRDF lobe stays structurally unreachable
for vanilla Skyrim content, confirmed by code-path analysis, not sampling.
18/18 `shader_type_data_tests`, 5/5 `emissive_source_tests` re-run live this
session, both green.

### Dimension 3 — NPC Equip + FaceGen (M41)

**Context**: the 2026-07-25 pass was clean, but two days later (`9bf4c493`)
the whole NPC-assembly path was refactored into a cooperative/resumable
state machine (`byroredux/src/npc_spawn/resumable.rs`, 1158 new lines) so
exterior streaming can spawn one actor-part per frame budget. This pass
re-verified against that new code, including a live run of
`docs/smoke-tests/m41-equip.sh skyrim` against the real WhiterunBanneredMare
cell: **PASS** — the 6 named NPCs (saadia, brenuin, mikael, sinmir,
amaundmotierreend, hulda) each carry `EquipmentSlots`; 46 entities carry
`Inventory`; `tex.missing=0`. `resolve_armor_mesh`, `expand_leveled_form_id`
(LVLI single/multi-pick + recursion cap), FaceGen parse, and
`BSDismemberSkinInstance` partition→bone-palette remap are all unchanged and
correct post-refactor.

#### SKY-D3-2026-08-03-01: Cooperative NPC assembly re-walks and re-tags the whole growing actor subtree on every attached part
- **Severity**: MEDIUM
- **Location**: `byroredux/src/npc_spawn/resumable.rs:1126-1133` (`parent_part`)
- **Status**: NEW (introduced by `9bf4c493`, 2026-07-27 — postdates the last Dimension-3 pass)
- **Description**: `parent_part` calls `tag_descendants_as_actor(world,
  placement_root)` after attaching **every** part (skeleton, each body
  piece, head, hair, brow, each eye, each armor piece), doing a full BFS
  from the placement root over the entire actor subtree assembled so far —
  including parts already tagged by the previous call. Pre-refactor this ran
  exactly once per NPC, at the very end. Post-refactor a fully-equipped
  actor re-walks its own (monotonically growing) subtree `N+6` times instead
  of once, where `N` is its part count.
- **Impact**: Purely a CPU-efficiency regression — idempotent, so no NPC
  renders wrong — but it directly undercuts the refactor's own stated goal
  of bounding per-frame NPC-assembly cost during exterior streaming; the
  redundant work is silently folded into each unit's wall-clock cost rather
  than skipped or amortized.
- **Suggested Fix**: Tag from `part_root` (the new subtree, already in
  scope) instead of re-walking from `placement_root`; the two
  `Finalize`-phase calls then become redundant and can be removed.

#### LOW-1: `audit-skyrim` skill's own Dimension-3 checklist still misstates the `upperbody.nif` pre-scan as applying to "Skyrim+"
- **Severity**: LOW
- **Location**: `.claude/commands/audit-skyrim/SKILL.md:137`
- **Status**: NEW (the underlying fact was noted narratively in the
  2026-07-16 report, but the skill file's checklist line was never
  corrected, so every subsequent Dimension-3 pass re-derives the same
  clarification from scratch)
- **Description**: `humanoid_body_paths` returns `&[]` for
  `GameKind::Skyrim | Fallout4 | Fallout76 | Starfield` — the `upperbody.nif`
  pre-scan mechanism the checklist describes is real, but exclusively for
  the kf-era `Oblivion | Fallout3NV` arm. Skyrim's actual body-coverage
  mechanism is the race-default-skin fallback + post-loop occupancy filter
  (#2093/#2094).
- **Suggested Fix**: Reword the checklist line to point at the actual
  Skyrim+ mechanism.

Edge case attempted and disproven: a body-covering armor item displaced off
its slot by a later item whose mesh fails to resolve cannot create a
rendering gap in practice — displacement is per-bit and a worn-mesh NIF
renders whole regardless of nominal bit ownership; only total displacement
combined with the displacer's own resolve failure would matter, and that
needs malformed ARMO/ARMA data no vanilla record exhibits.

### Dimension 4 — Multi-Master Load Order + TES5 Cell-Load Regression

**Result: all clean, one new LOW finding.** Live-verified this session (not
cited): `parse_real_skyrim_esm` passes against the real `Skyrim.esm`
(SolitudeWinkingSkeever, 981 refs, 590/590 cells with XCLL); full
`byroredux-plugin` suite — **631 passed, 0 failed**. Went beyond the prior
pass by writing a throwaway two-master live-data test against real
`Skyrim.esm` + `Update.esm` + `Dawnguard.esm` (not covered by the repo's
synthetic single-master fixtures): 21,624/26,881 REFRs in shared Fort cells
correctly resolved cross-plugin, **zero** unresolved refs. `.STRINGS` loader
wiring (#1553), ESL bit-packing (#1554), and deleted-REFR tombstone handling
(#1660) are all confirmed intact by direct code read, not citation.

#### SK-D4-NEW-01: No overflow guard on regular/light-master slot counters in the load-order assignment loop
- **Severity**: LOW
- **Location**: `byroredux/src/cell_loader/load_order.rs:158-191`
- **Status**: NEW
- **Description**: `next_regular: u8` / `next_light: u16` are incremented
  unconditionally with no ceiling check, unlike every other failure mode in
  this function (duplicate plugin, missing master, misordered master), all
  of which error loudly. Past 254 regular or 4096 ESL `--master` plugins,
  this silently wraps/aliases two plugins' FormID spaces together.
- **Impact**: Only reachable past 254 regular or 4096 ESL plugins — not
  realistic for any current Skyrim SE load order (~7-10 plugins including
  all official DLC), and matches the real engine's own hard ceiling.
  LOW/hardening, not a compat bug.
- **Suggested Fix**: Replace the increments with `checked_add`, erroring in
  the same style as the function's other guards.

Control-bench note: the ROADMAP figure for WhiterunBanneredMare (335.0 FPS,
3237 entities, R6a-stale-15) and this session's live Dimension-3 run (5110
entities) disagree — already tracked as **Existing: #2216** ("stale on
entities_total... benign drift"), not re-filed here.

### Dimension 5 — BSA v105 (LZ4)

**Result: all clean, one new LOW finding.** `crates/bsa/src/archive/` is
already heavily hardened from prior sessions. Live-verified this session:
`cargo test -p byroredux-bsa --lib` (53 passed) + the real-data `--ignored`
suite (11 passed) against actual Skyrim SE archives; the existing
`skyrim_bsa_sweep_audit` example against all 11 real v105 archives —
**65,637 files, 0 errors, 0 magic mismatches** across Meshes0/1 +
Textures0-8. A debug-build rerun (activating debug-only hash/offset
assertions) found zero hash or offset mismatches across the same 65,637
files. Zero-based sibling auto-load confirmed end-to-end against the real
install: `Textures0.bsa` → 9 archives, `Meshes0.bsa` → 2 archives, missing
siblings silently skipped, corrupt-but-present siblings warned-and-skipped
without aborting the primary load. Clarified two checklist framings that
don't match the format: v105 uses `lz4_flex::frame::FrameDecoder` (not the
`block` API #2097/LZ4-01 flags for the separate BA2 reader), and the
"compression flag priority" is actually an XOR/toggle, not an
override relationship — both already correctly implemented and tested.

#### SK-D5-BSA-NEW-01: Stale "256 MB" cap comments — actual enforced limit is 1 GB
- **Severity**: LOW
- **Location**: `crates/bsa/src/archive/extract.rs:97,118,166`, `crates/bsa/src/ba2.rs:491`
- **Status**: NEW
- **Description**: Commit `4a2b8200` bumped `MAX_CHUNK_BYTES` from 256 MB to
  1 GB (to fit FO76 content) without updating four call-site comments that
  still describe the old 256 MB figure.
- **Impact**: Cosmetic only — the code correctly uses the constant
  everywhere; a future reader trusting the comment over the constant could
  misjudge the actual safety margin.
- **Suggested Fix**: Update the four comments to say "1 GB" or reference
  `MAX_CHUNK_BYTES` by name instead of hardcoding the number in prose.

### Dimension 6 — Specialty Blocks + Real-Data Rendering

**Result: no new findings.** `BSLODTriShape` routes to `NiLodTriShape`
(NOT `BsTriShape` — #838 regression guard intact, confirmed by direct read
of the dispatch table and its documenting comment); `BSMeshLODTriShape` and
`BSSubIndexTriShape` have their own distinct dispatch arms, no confusion.
`BsLagBoneController` + `BsProceduralLightningController` (#837) live-tested
this session: 2/2 and 1/1 pass respectively. M35 `.btr` distant-terrain LOD:
5/5 tests pass. `.bto` object LOD: 4/4 tests pass, including the
hysteresis-band ring-exclusion regression test; VWD full-model culling
(#1731) reconfirmed as intentional forward scope, not re-filed. **Meshes0
sweep baseline live-run this session**: 18,862/18,862 clean (100.00%), 0
truncated, 0 failures, 0 recovered — matches the ROADMAP baseline exactly,
not just cited. The block-type histogram from this same run counts 21,140
`BSDynamicTriShape` blocks as "clean" — the exact blocks Dimension 1's
SK-D1-01 finds import to zero meshes, which is the key methodological point
tying these two dimensions together: **a clean parse-rate sweep cannot catch
this class of bug**, because it only measures whether bytes were consumed
correctly, not whether the import boundary produced usable geometry from
them.

### Dimension 7 — NIFAL Canonical Material Translation (Skyrim slice)

**Result: 0 CRITICAL, 0 HIGH, 2 MEDIUM, 1 LOW.** Checklist items 1-4
(single boundary, deleted per-draw classifier, resolve-before-glass
ordering, `EmissiveSource` routing) all independently re-verified as
**CLEAN** against live code (not re-cited): `translate_material` has
exactly two per-game callers (cell-spawn + loose-NIF-load), both
spawn-time; `classify_pbr` is confirmed **deleted** (zero matches
repo-wide, not merely deprecated); `resolve_pbr()` runs immediately before
`classify_glass_into_material` so forced-glass roughness wins; Skyrim's
`BSLightingShaderProperty` routes to `EmissiveSource::Lighting` and
`BSEffectShaderProperty` to `Effect`, with a clobber analysis confirming
neither can downgrade/relabel the other via a mixed-property mesh.

#### SKY-D7-01: Skyrim `lighting_effect_1`/`lighting_effect_2` captured at import, dropped at the canonical boundary
- **Severity**: MEDIUM
- **Location**: captured `crates/nif/src/import/material/dedicated_shader.rs:308-314`; carried `crates/nif/src/import/types.rs:484-491`; never read by `byroredux/src/material_translate.rs:95-181`
- **Status**: Existing — same-day sibling audit `docs/audits/AUDIT_NIFAL_2026-08-03.md` finding MAT-D1-NEW-04 is the superset of this (**not yet filed as a GitHub issue** — absent from the 47 currently-open issues); confirmed independently here for the Skyrim slice.
- **Description**: `BSLightingShaderProperty` on Skyrim LE/SE authors
  `lighting_effect_1` (subsurface roll-off) and `lighting_effect_2`
  (rim/backlight power); `ImportedMaterial` carries both, but canonical
  `Material` has no corresponding fields and `translate_material`'s literal
  never mentions them. `GpuMaterial` has no such field either — the data
  ends at `ImportedMaterial`.
- **Impact**: Skin/hair/cloth/ice materials authoring non-default subsurface
  or rim-lighting response render with the engine's fixed Disney BSDF
  response instead of the authored curve. Shading-fidelity loss only —
  stays below the HIGH "wrong/divergent Material" bar — but is genuine
  authored-data loss at the boundary; `docs/engine/nifal.md`'s "Materials —
  converged" verdict overstates completeness.
- **Suggested Fix**: File MAT-D1-NEW-04 as an issue if not already queued;
  add the two scalars to `Material`/`GpuMaterial` end-to-end (mirroring
  #1147's `translucency_*` rollout), or at minimum record them in
  `nifal.md` as "captured, not yet shaded."

#### SKY-D7-02: Authored `refraction_strength` discarded for every Skyrim material that isn't fire-refraction
- **Severity**: MEDIUM
- **Location**: `byroredux/src/material_translate.rs:34-44` (`material_optical_scalar`) + `:180`; producer `crates/nif/src/import/material/dedicated_shader.rs:307,333-350`
- **Status**: NEW (adjacent to but distinct from **#2232**, which is about the `ior` overload being undocumented, not this discard)
- **Description**: `material_optical_scalar` only returns the authored
  `refraction_strength` when `material_kind ==
  MATERIAL_KIND_FIRE_REFRACTION` (synthesized only when both SLSF1
  `REFRACTION` and `FIRE_REFRACTION` bits are set); every other kind gets a
  constant `DEFAULT_DIELECTRIC_IOR` (1.5), silently discarding the authored
  scalar. Ordinary Skyrim refractive-glass/ice/crystal authoring (SLSF1
  `Refraction` alone) hits this — the flag isn't packed into any
  `material_flag::*` bit either.
- **Impact**: Skyrim refractive surfaces render as ordinary dielectrics
  (IOR 1.5, no authored distortion) or, if a glass texture-keyword happens
  to fire, at the engine's fixed glass IOR (1.45) regardless of what the
  artist authored. Shading fidelity only — no wrong material *kind*, no
  fabrication — hence MEDIUM not HIGH.
- **Suggested Fix**: Either pack a `MAT_FLAG_REFRACTION` bit and let the
  scalar ride an un-overloaded canonical field, or explicitly document the
  non-103 discard as deliberate in both `material_translate.rs` and
  `nifal.md` — today it reads as an oversight at the call site.

#### SKY-D7-03: Canonical PBR `roughness` is written at a second spawn-time site outside `translate_material`
- **Severity**: LOW
- **Location**: `byroredux/src/material_translate.rs:300-338` (`resolve_normal_alpha_spec_roughness`), called from both spawn paths
- **Status**: NEW — documentation precision only, **not a defect**
- **Description**: Both spawn paths call `resolve_normal_alpha_spec_roughness`
  after texture handles are attached, re-deriving `roughness` from
  `glossiness`/`specular_strength` plus resolved normal/gloss textures. This
  is the dominant path for Skyrim specifically (no dedicated gloss map;
  spec mask lives in the normal-map alpha), so most Skyrim architecture's
  shipped roughness comes from this second write, not `translate_material`'s
  literal.
- **Impact**: None functionally — the helper is idempotent and NaN-guarded.
  The cost is purely that `material_translate.rs`'s own "the single site"
  doc claim and `nifal.md` describe a one-shot boundary where the real
  implementation is a documented two-phase one.
- **Suggested Fix**: Amend the module doc and `nifal.md`'s Materials row to
  describe the boundary as two-phase.

## Shader-Type Coverage Matrix

| `ShaderTypeData` variant | Numeric type(s) | Parse | Import | Render |
|---|---|---|---|---|
| `None` | 0,2,3,4,8,9,10,12,13,15,17,18,19,20 (Sky/FO4); 0,2,3,12,17 (FO76) | complete | complete (no-op) | complete (default material path) |
| `EnvironmentMap` | 1 | complete | complete | complete (env cubemap reflection) |
| `SkinTint` | 5 | complete (Color3 Skyrim, +alpha FO4 130-139) | complete | complete |
| `HairTint` | 6 (Sky/FO4), 5 (FO76) | complete | complete | complete |
| `ParallaxOcc` | 7 | complete | complete | complete (POM in `triangle.frag`) |
| `MultiLayerParallax` | 11 | complete | complete | render-complete; pre-existing open caustic-source/TLAS-mask mismatch (REN-D14-01/02, not Skyrim-specific, owned by `/audit-renderer`) |
| `SparkleSnow` | 14 | complete | complete | not independently re-verified this pass |
| `EyeEnvmap` | 16 | complete | complete | complete (NPC eye cubemap) |
| `Fo76SkinTint` | 4 (FO76 only) | complete | complete | N/A to Skyrim |

## Cell-Load Regression Status

TES5 cells parse cleanly through the unified `esm/cell/` walker; compressed
record groups decompress without error. `parse_real_skyrim_esm` passes live
against the real `Skyrim.esm`. Multi-master DLC load order (Dawnguard +
Update.esm + Skyrim.esm) resolves cross-plugin REFRs correctly with zero
unresolved refs on a live throwaway test against real DLC content.
BSA v105 extraction is 100% clean across all 11 real archives (65,637
files). The Meshes0 NIF-block-dispatch sweep is 100% clean (18,862/18,862,
0 truncated). **The one caveat to "cell-load is clean": a clean parse does
not imply a complete render** — SK-D1-01 shows 21,140 correctly-parsed
`BSDynamicTriShape` blocks per Meshes0 alone produce zero imported meshes,
so "parses clean" and "renders complete" are measuring different things and
should not be conflated when reading future sweep results.

Control-bench entity-count figures are internally inconsistent across this
session's own dimensions (3237 per ROADMAP vs 5110 live-measured in
Dimension 3/4) — already tracked as **Existing: #2216**, not re-filed.

## Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 3 |
| LOW | 5 |
| **Total** | **9** |

The single HIGH finding (SK-D1-01) is the audit's headline result: a
three-site, small-diff bug that makes every Skyrim SE NPC render headless,
undetected through at least three prior audit passes because the existing
regression gate (M41 equip smoke test) checks entity/component/texture
counts, not head-mesh presence. The three MEDIUM findings are all
shading-fidelity or CPU-efficiency issues with no crash/data-loss risk. The
five LOW findings are documentation drift and hardening gaps with no
realistic trigger under current content.

---

Suggested next step: `/audit-publish docs/audits/AUDIT_SKYRIM_2026-08-03.md`
