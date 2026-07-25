# Oblivion (TES4) Compatibility Audit — 2026-07-25

**Scope**: NIF v20.0.0.5 + the v10.x NetImmerse tail, BSA v103, the live ESM
path, rendering/material translation, NIFAL canonical translation for
Oblivion, real-data validation, and the exterior blocker chain. Run as one
leg of a `comprehensive` audit-suite sweep. All 7 dimensions worked directly
in this session (no sub-agent delegation) — every checklist item verified
against live source, live `cargo test` runs, and live data pulled from
`/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/`.

## Executive Summary

Oblivion remains the most mature per-game slice of the compat matrix short
of exterior rendering, and this sweep found the surface essentially
unchanged and healthy since the 2026-07-16 audit — every regression-guard
item checked out, the full workspace test suite (4300+ tests across the
tail of the log, 0 failures) passed, and three real on-disk meshes traced
cleanly end-to-end through `import_nif_scene`. The one substantive finding
this cycle is procedural, not code-level: a real, previously-investigated,
still-open Oblivion-specific gameplay bug (`is_grounded` stuck false at an
interior spawn, blocking jump) had its own GitHub follow-up **recommended
at issue-closure time and never filed** — it now exists only as a paragraph
in `ROADMAP.md`'s Known Issues, untracked as a distinct issue.

- **NIF parse (incl. v10.x tail)**: 99.93% clean (8 026 / 8 032,
  `Oblivion - Meshes.bsa`, verified live this session via `nif_stats`) — byte
  identical to the checked-in `oblivion_truncations.tsv` baseline (6 residual
  NetImmerse markers: `marker_arrow`/`divine`/`map`/`radius`/`temple`/`travel`,
  0 hard failures). Matches `ROADMAP.md`'s cited row exactly — no drift.
- **BSA v103 archive**: regression guard intact — version/folder-size/hash
  logic unchanged and correct; `byroredux-bsa` unit suite green.
- **ESM parse**: live path (not a stub), all game-specific branches
  (16-byte ACBS, CONT 4-byte DATA, CLMT 8-byte WLST, XCLL 3-size band,
  single-byte DIAL/INFO DATA) verified correct; both previously-ignored
  real-data parity tests (`clas_oblivion_knight_against_vanilla`,
  `race_oblivion_data_and_subs_against_vanilla`) pass live against vanilla
  `Oblivion.esm`.
- **Render / NIFAL**: Disney-BSDF gate confirmed to stay unreachable for the
  all-legacy Oblivion material universe; `EmissiveSource::Material` tagging,
  `resolve_pbr` NaN-sentinel resolve-once path, and the #869 wireframe/
  flat-shading guards all hold.
- **Cell loading**: Interior renders end-to-end (unchanged). Exterior:
  TES4 worldspace + LAND parse/load is implemented and game-agnostic; only
  the on-device exterior render bench remains pending (same shape FO3 was —
  *not* a BSA v103 problem, that framing stays dead per #699).
- **Top blocker in priority order**: (1) on-device Oblivion exterior render
  bench (infrastructure exists, unexercised) — unchanged from prior sweeps;
  (2) the untracked `is_grounded`/jump regression at Oblivion interior
  spawns (this audit's new finding, OBL-2026-07-25-01).

## Dimension Findings

### Dimension 1 — NIF Version Handling (v20.0.0.5 + v10.x NetImmerse Tail)
**0 new findings — all checklist items confirmed as intact regression
guards.**

- `header.rs`: `user_version` gate at `V10_0_1_8`, and the BSStreamHeader
  dual-band guard (`version == V10_0_1_2 || (user_version >= 3 && (version
  ∈ {V20_2_0_7, V20_0_0_5} || (V10_1_0_0 <= version <= V20_0_0_4 &&
  user_version <= 11)))`) match nif.xml exactly; the #170 regression test
  (`bs_stream_header_not_read_for_off_spec_version`) still passes.
- All v10.x sub-version constants (`V10_0_1_2`, `V10_1_0_0`..`V10_1_0_114`,
  `V10_2_0_0`, `V20_0_0_4`, `V20_0_0_5`) present in `version.rs` and used as
  gate boundaries.
- `#1509` guard (`crates/nif/src/blocks/controller/morph.rs:103`):
  `NiGeomMorpherController` still gates its trailing field on `bsver > 9`
  (not the old `bsver != 0 && bsver <= 11`).
- `#1506`/`#1507`/`#1508` stride-drift family: `NiInterpController`/
  `NiQuatTransform` (`interpolator_tests.rs`), `NiPSysData` + emitter
  (`particle_tests.rs`), `NiBlendInterpolator` + `ControlledBlock`
  (`interpolator.rs`) all still land on the correct block boundary.
- `NiTexturingProperty` (`properties.rs:211`) reads `texture_count` as a raw
  `u32`, no leading bool gate; slots 6/7 correctly excluded pre-20.2.0.5.
- Pre-Gamebryo inline block-type-name handling (`lib.rs:378-412`) intact;
  the truncation-failure path logs at `warn!`, the normal detection path at
  `debug!` (no sweep-spam risk — see Dimension 7).
- u16-vs-u32 `NiAVObject.flags` gate (`base.rs:82`, `bsver >
  FLAGS_U32_THRESHOLD`) and the #1331 fix (using raw `bsver()` rather than
  the variant helper) both hold.
- Legacy/Oblivion-only block dispatch (`NiKeyframeController`,
  `NiSequenceStreamHelper`, `NiBillboardNode` + `NiNode` subclasses,
  `NiLight` hierarchy, `NiUVController`, `NiCamera`, `NiTextureEffect`, the
  legacy particle stack, `BSShader*Property` aliases) all present in
  `blocks/mod.rs`'s dispatch table.
- Collision: `BhkMultiSphereShape` → `Compound`/`Ball` and
  `BhkConvexListShape` → `Compound`/`ConvexHull` both resolve correctly in
  `import/collision/shape.rs` (verified via
  `multi_sphere_shape_resolves_to_compound`, `convex_list_shape_resolves_to_compound`
  and siblings).
- `havok_motion_type` (`import/collision/mod.rs:156`) maps the full Havok
  enum per #1652 — `1..=5|8 → Dynamic`, `6 → Keyframed`, `7 → Static`,
  `9 → CharacterKinematic`, else `Static`; the pre-fix `4 => Keyframed` /
  `_ => Static` collapse is not present.
- **Verification**: `cargo test -p byroredux-nif --lib` → **886 passed, 0
  failed, 0 ignored**.

### Dimension 2 — BSA v103 Archive
**0 findings — regression guard confirmed.**

- `BSA_V_OBLIVION = 103` recognised in `open.rs:40`; rejection only outside
  `{103, 104, 105}`.
- Folder-record size: `if version == BSA_V_SKYRIM_SE { 24 } else { 16 }`
  (`open.rs:100`) — v103/v104 both 16 bytes, only v105 is 24.
- `embed_file_names` gated on `version >= BSA_V_FO3_SKYRIM` (`open.rs:75`).
- Folder/file hash functions (`hash.rs`) unchanged.
- **Verification**: `cargo test -p byroredux-bsa --lib` → **53 passed, 0
  failed, 11 ignored**. Live `nif_stats` sweep over `Oblivion - Meshes.bsa`
  (8 032 files) round-tripped with 0 archive-level errors — end-to-end v103
  extraction still works exactly as #699 closed it.

### Dimension 3 — ESM Record Coverage (live path, not a stub)
**0 new findings.**

- TES4 header/GRUP walking unchanged; `EsmVariant::detect`
  (`reader.rs:56-62`) is purely structural (byte-offset-20 `HEDR` probe for
  the 20-byte-vs-24-byte record header split) — it does **not** depend on
  the HEDR version float, so the "1.0 vs 0.94" checklist phrasing is a
  non-issue: Oblivion is identified by header shape, never misroutes.
- 16-byte ACBS guard (`actor/mod.rs:723`, `GameKind::Oblivion` arm gated
  `len >= 16`) still precedes the FNV/FO3 24-byte arm (`:753`) in match
  order. Both `oblivion_16byte_acbs_parses_level_and_gender` and
  `fnv_ignores_16byte_acbs` pass.
- MGEF-by-code map, CONT 4-byte payload guard
  (`cont_data_handles_oblivion_4byte_payload_without_overrun` passes), CLMT
  8-byte-vs-12-byte WLST dispatch (`game`-gated, not autodetected —
  `parse_clmt_oblivion_three_entry_wlst_decodes_as_three` passes) all hold.
- The two previously-ignored real-data parity tests
  (`clas_oblivion_knight_against_vanilla`,
  `race_oblivion_data_and_subs_against_vanilla`) **re-run green against
  vanilla `Oblivion.esm`** this session.
- CELL walker: `XCLL` canonical-size table (`walkers.rs:47`,
  `XCLL_SIZES_OBLIVION = [28, 32, 36]`) and `RCLR` (interior `walkers.rs:523`
  + exterior `wrld.rs:348`) both handled.
- DIAL/INFO: Oblivion's single-byte `DATA` dialogue-type byte handled
  distinctly from FO3+'s wider layout (`misc/dialogue.rs:118`); all 10
  dialogue tests pass.
- Minimum exterior-REFR record set: no Oblivion-specific gap found in the
  cell loader beyond what's already covered by the shared FNV-aligned CELL
  walker — see Dimension 7.
- **Verification**: `cargo test -p byroredux-plugin --lib` (targeted:
  actor, climate, container, dialogue, cell suites) all green;
  `cargo test -p byroredux-plugin --test parse_real_esm -- --ignored
  clas_oblivion_knight_against_vanilla race_oblivion_data_and_subs_against_vanilla`
  → **2 passed**.

### Dimension 4 — Rendering Path for Oblivion Shaders
**0 findings.**

- `NiTexturingProperty` → `MaterialInfo` pipeline (base/dark/detail/
  gloss/glow/bump) intact, decal-slot loop still correctly excludes
  post-20.2.0.5-only normal/parallax slots.
- `NiMaterialProperty` legacy color path — no `srgb_to_linear` applied
  (unchanged, per `0e8efc6`).
- `NiVertexColorProperty` / `NiStencilProperty` / `NiZBufferProperty` all
  parse and are consumed; #869 guards confirmed: `NiWireframeProperty`
  (`legacy_properties.rs:530`) still routes to `flat_shading` /
  `vk::PolygonMode::LINE` (`static_meshes.rs:589-597`), and
  `NiShadeProperty.flat_shading`'s BSVER-gated flags field
  (`properties.rs:568`) is correct.
- `#1239` `NiPSysEmitter` version gate (`particle.rs:81-89`) still routes
  Oblivion's pre-BS202 emitter layout correctly.
- Emitter runtime hookup verified end-to-end: `extract_emitter_params` /
  `extract_emitter_rate` (`import/walk/mod.rs`) feed both
  `scene/nif_loader.rs:547` (loose-NIF path) and `cell_loader/spawn.rs:615`
  (cell-load path), both calling `apply_emitter_params`
  (`systems/particle.rs`) — an Oblivion emitter that parses reaches the ECS
  and animates, not just parses-then-drops.
- Disney-BSDF gate (`MAT_FLAG_PBR_BSDF`): `pack_bgsm_material_flags`
  (`cell_loader.rs:221-223`) only sets it from `mesh.is_pbr`, which is only
  ever flipped `true` by the FO4 BGSM-authored-`pbr`-flag arm
  (`asset_provider/material.rs:937-940`) or the Starfield `.mat`+CDB-present
  arm (`:691-704`) — neither path is reachable from any Oblivion
  `NiTexturingProperty`/`NiMaterialProperty` content. Gate confirmed
  unreachable for the entire Oblivion material universe.
- **Verification**: `cargo test -p byroredux-nif --lib import::material` →
  **149 passed, 0 failed**.

### Dimension 5 — NIFAL Canonical Material Translation for Oblivion
**0 findings.**

- `translate_material` (`byroredux/src/material_translate.rs`) remains the
  single boundary; `static_meshes.rs:317-326` reads `m.roughness`/
  `m.metalness` directly with the explicit "no per-draw keyword scan /
  classify_pbr fallback" comment intact.
- `classify_pbr_keyword` (`crates/core/src/ecs/components/material.rs:491`)
  is called from exactly one site — `Material::resolve_pbr` (`:743`) —
  confirmed not reachable from any render-loop call site.
- `EmissiveSource::Material` tagging for `NiMaterialProperty`
  (`import/material/legacy_properties.rs:105`) confirmed distinct from the
  `BSLightingShaderProperty`→`Lighting` and `BSEffectShaderProperty`→
  `Effect` arms; all 5 `emissive_source_tests` pass.
- Disney-gate-stays-0 cross-referenced with Dimension 4 — same conclusion.
- **Verification**: `cargo test -p byroredux-core --lib resolve_pbr` → 5
  passed; `cargo test -p byroredux-nif --lib emissive_source` → 5 passed.

### Dimension 6 — Real-Data Validation
**0 findings — baseline matches exactly, no drift.**

- Live `nif_stats` sweep, `Oblivion - Meshes.bsa` (8 032 files): **8 026
  clean (99.93%), 6 truncated (38 blocks dropped total), 0 failures, 0
  recovered/unknown types** (81 distinct block types, all `unknown = 0`).
  Byte-identical to `crates/nif/tests/data/block_coverage_baselines/oblivion_truncations.tsv`.
- `per_block_baseline_oblivion` integration test (opt-in, `--ignored`)
  passes.
- `recovery_trace` run against all 6 residual truncated files individually
  (`marker_arrow`, `marker_divine`, `marker_radius` shown in detail; the
  remaining 3 share the same NiNode-claims-absurd-element-count signature)
  — confirmed the expected NetImmerse-tail stride-drift signature (not new
  drift), matching #1611's checked-in baseline.
- Three representative interior meshes traced through `import_nif_scene`
  end-to-end via `import_probe`:
  - `meshes\lights\chandelier04.nif` (bsver 11 / v20.0.0.5): 72 blocks → 9
    nodes, 8 meshes, 1 collision node.
  - `meshes\clutter\books\octavo04.nif` (bsver 11): 18 blocks → 1 node, 2
    meshes, 1 collision node.
  - `meshes\creatures\goblin\goblinhead.nif` (bsver 5, NetImmerse v10.x
    family): 26 blocks → 1 node, 1 mesh, 0 collision (expected — creature
    heads are skinned, not rigid-collision).
  All three parsed and imported with plausible node/mesh/collision counts;
  no unexpected drops.
- **Verification**: full workspace `cargo test --workspace` this session
  → **0 failures** across every crate (nif 886, plugin ~700+ across lib +
  doctests, bsa 53, core 546+, renderer 593+, save 428, scripting 187, and
  the remaining smaller crates — see raw log for the full per-crate
  breakdown; grep for `FAILED`/`panicked` across the full log returned
  nothing).

### Dimension 7 — Exterior Blocker Chain & Game-Specific Quirks
**1 finding — see below.** All other checklist items confirmed clean.

- Confirmed (again) that the real Oblivion exterior blocker is TES4
  worldspace + LAND wiring, already implemented game-agnostically
  (`byroredux/src/cell_loader/exterior.rs` has zero `GameKind::Oblivion`
  special-casing beyond what the shared CELL/WRLD walkers already do) — not
  BSA v103, which stays closed per #699.
- `_far.nif` distant-object LOD (#1726/#1745): `placement_lod_supported`
  (`placement_lod.rs:306`) still gates the whole scheme to
  `GameKind::Oblivion` only (FO3/FNV ship zero `.lod` files — #2086); all 10
  `placement_lod` unit tests pass, including
  `parses_real_single_placement_file` against real vanilla data.
- Pre-v3.3.0.13 fallback logs at `debug!` (`lib.rs:380`), not `warn!` — no
  sweep-spam risk. `warn!` is reserved for the actual truncation-failure
  path (`lib.rs:404`), which is a distinct, rarer event.
- No Oblivion-specific record type missing from the cell loader's REFR-
  placement surface beyond the FNV-aligned baseline.
- Two previously-flagged Dimension-7 LOW findings from the 2026-07-16
  report were checked and are now **closed**:
  - `OBL-D7-01` (legacy_particle.rs module doc overclaiming Oblivion
    dependency) — doc comment now correctly softened
    (`crates/nif/src/blocks/legacy_particle.rs:8-19`), matches the suggested
    fix verbatim. **Fixed, not regressed.**
  - `DIM3-OBL-01` (XESP doc mislabeled "Skyrim+") — fixed via #2088,
    confirmed closed; current comment at `walkers.rs:861-863` correctly
    reads "present since Oblivion... NOT Skyrim+, per #2088".
  - `DIM3-OBL-02` (`flags_oblivion` parsed but unconsumed) — still true,
    still LOW, still intentional (CHARAL sequencing per the prior report's
    own conclusion); no GitHub issue was ever filed for it. Carried forward,
    not re-minted as new.

#### OBL-2026-07-25-01: Untracked residual — `is_grounded` stays false at Oblivion interior spawn, blocking jump
- **Severity**: HIGH
- **Dimension**: Exterior Blocker Chain & Game-Specific Quirks (surfaced via
  `ROADMAP.md`'s Known Issues while cross-checking Dimension 7's blocker
  chain against the ticket tracker)
- **Location**: `crates/nif/src/import/collision/shape.rs:354`
  (`resolve_tri_strips_data_refs`, the suspected root per the closing
  investigation), consumed at `byroredux/src/systems/character.rs:195,219,335`
  (`c.is_grounded` gates `jump_fired` and `desired_vertical`),
  `crates/physics/src/components.rs:107`
- **Status**: NEW (the underlying symptom was investigated and closed as
  part of `#2013`, but the closing comment explicitly recommended filing a
  **separate** follow-up issue for this residual, which a `gh issue list
  --state all --search "grounded"` / `"is_grounded"` / `"ICMarketDistrict"`
  sweep in this session confirms was never done — it exists only as a
  paragraph in `ROADMAP.md`'s Known Issues, line ~701)
- **Description**: `#2013` ("TES-family player rig never grounds at
  cell-load spawn — infinite freefall") was fixed in `e2f75456`
  (2026-07-18) by adding a capsule-shaped ground probe
  (`PhysicsWorld::cast_capsule_down`) to the door-spawn nudge. That fix
  resolved the reported symptom on both Skyrim SE and Oblivion — spawn is
  now stable on both, and the issue was closed. But the closing comment
  documents a **second, distinct** bug found while verifying the fix on
  Oblivion (`ICMarketDistrictTheGildedCarafe`): the character no longer
  falls through the floor, but `is_grounded` itself never flips `true`. A
  one-off diagnostic probe attributed this to the resting contact's surface
  normal reading inverted (`dot(normal, +Y) ≈ -0.99`) — i.e. a
  wrong-winding collision triangle, "likely somewhere in the NiTriStrips-
  based Oblivion collision import path (`resolve_tri_strips_data_refs` in
  `crates/nif/src/import/collision/shape.rs`)" per the investigator's own
  words. The comment explicitly recommends filing this as its own issue
  "since it's a different bug... than the spawn-positioning bug this issue
  tracked" — that recommendation was never carried out.
- **Evidence**: `gh issue view 2013 --comments` (this session) shows the
  full investigation trail across three comments culminating in the
  e2f75456 fix note. `is_grounded` is read at
  `byroredux/src/systems/character.rs:195` (`let jump_fired = want_jump_now
  && controller.is_grounded && !controller.wants_jump;`) and `:219`
  (`if controller.is_grounded && !jump_fired`) — both gate real player
  control, not cosmetic state. `gh issue list --repo matiaszanolli/ByroRedux
  --state all --search "grounded"` returns only `#2013` (closed) and
  `#1832` (closed, the earlier zero-mass-Dynamic reclassification fix); no
  distinct tracking issue exists for the inverted-normal residual.
- **Impact**: On any Oblivion interior spawn reproducing the same floor
  geometry class as `ICMarketDistrictTheGildedCarafe` (verified live with a
  real Vulkan device + Oblivion game data, per the closing comment), the
  player character cannot jump (`jump_fired` requires `is_grounded == true`)
  and the vertical-velocity resolution path in `character.rs:219` takes the
  non-grounded branch every frame despite resting on solid ground — a
  correctness gap in core player control, not merely cosmetic, and specific
  to Oblivion content (the shared FO3/FNV collision path grounds correctly,
  per the investigation's own elimination). Graded HIGH per the "fails
  under realistic conditions, no workaround" bar — jump is unconditionally
  unavailable in the affected cell(s), not a rare or gated failure mode.
- **Related**: `#2013` (closed — spawn-positioning symptom fixed),
  `#1832` (closed — zero-mass-Dynamic reclassification, a prerequisite fix
  sharing the same `extract_from_classic` path)
- **Suggested Fix**: File the follow-up issue the `#2013` closing comment
  already recommends, scoped narrowly to the inverted collision-normal
  hypothesis in the Oblivion NiTriStrips collision-import path
  (`resolve_tri_strips_data_refs` / `merge_tri_strips_shape`,
  `crates/nif/src/import/collision/shape.rs:340-`). Needs a live Vulkan
  device + real Oblivion data (already available in this environment) to
  reproduce and isolate — same tooling used to close `#2013`.

## Blocker Chain

Interiors already render end-to-end (Anvil Heinrich Oaken Halls, unchanged).
The real remaining chain to "Oblivion exterior renders":

1. **TES4 worldspace + LAND wiring** — implemented and game-agnostic (verified
   this session: `exterior.rs` carries no Oblivion-specific branching beyond
   the shared CELL/WRLD walker work already audited in Dimension 3).
2. **CELL exterior REFR placement** — no Oblivion-specific record gap found;
   shares the FNV-aligned baseline.
3. **On-device exterior render bench** — infrastructure exists (same shape as
   the completed FO3/Skyrim/FO4 benches), execution still pending. This is
   the only remaining item in the chain and is unchanged from the prior
   sweep — **do not** regenerate the stale "BSA v103 is broken" framing
   (dead since #699).

Orthogonal to this chain (does not block exterior rendering, but blocks
core player-character correctness in interiors on at least one verified
cell): **OBL-2026-07-25-01** above.

## Regression Guard List

All items below were checked against live code + live test runs this
session and confirmed still intact:

- v10.x stride-drift family: `#1506` (`NiInterpController`/`NiQuatTransform`),
  `#1507` (`NiPSysData` + emitter), `#1508` (`NiBlendInterpolator` +
  `ControlledBlock`), `#1509` (`NiGeomMorpherController` `bsver > 9` gate).
- `NiTexturingProperty` raw `u32` texture-count read (no `Has Shader
  Textures: bool` gate).
- BSStreamHeader dual-band guard (`#170`).
- `user_version` threshold at `V10_0_1_8`.
- BSA v103 extraction end-to-end (`#699`).
- 16-byte Oblivion ACBS guard ahead of the FNV/FO3 24-byte arm (`#1650`).
- `havok_motion_type` full-enum mapping, no `4 => Keyframed`/`_ => Static`
  collapse (`#1652`).
- `BhkMultiSphereShape` / `BhkConvexListShape` → `CollisionShape` resolution.
- Disney-BSDF gate (`MAT_FLAG_PBR_BSDF`) stays 0 across the entire Oblivion
  material universe.
- `#869` `NiWireframeProperty`/`NiShadeProperty.flat_shading` guards.
- `#1239` `NiPSysEmitter` version gating + full emitter-to-ECS runtime
  hookup (parses **and** animates, not parse-then-drop).
- `#1611` residual truncation baseline (6 NetImmerse markers, 0 hard
  failures) — byte-identical to the checked-in TSV, confirmed via a fresh
  live sweep, not re-derived from memory.
- Pre-v3.3.0.13 inline-type-name fallback logs at `debug!`, not `warn!`.
- `placement_lod_supported` stays `GameKind::Oblivion`-only (`_far.nif` LOD,
  `#1726`/`#1745`/`#2086`).
- Two previously-fixed Dimension-7/3 doc findings from the 2026-07-16 audit
  (`OBL-D7-01`, `DIM3-OBL-01`) — both now fixed, confirmed not regressed.

## Finding Count Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0     |
| HIGH     | 1     |
| MEDIUM   | 0     |
| LOW      | 0 new (1 carried forward: `DIM3-OBL-02`, still LOW, no GH issue, intentional per CHARAL sequencing) |

**Total new findings requiring action: 1** (`OBL-2026-07-25-01`).

Suggest: `/audit-publish docs/audits/AUDIT_OBLIVION_2026-07-25.md`
