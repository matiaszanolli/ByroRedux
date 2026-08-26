# Legacy Compatibility Audit — 2026-07-02

**Scope:** Compatibility/mapping gaps between Gamebryo 2.3 / Creation-engine
behaviour and Redux's current implementation, framed by the three canonical
translation layers (NIFAL / EXAL / PHYSAL) plus coordinate-system correctness,
the per-game translation survey, and subsystem coverage vs. the legacy headers.

**Method:** Read the live layer specs (`nifal.md`, `exal.md`, `physal.md`,
`per-game-translation-survey.md`, `coordinate-system.md`), traced each claimed
`translate()` boundary to its callers to confirm single-producer, cross-checked
the survey's leak inventory against current source, and deduplicated every
candidate against the 21 open GitHub issues (`/tmp/audit/issues.json`) and the
per-layer leak inventories. Each finding was re-verified against live code and an
attempt made to disprove it before inclusion.

**Headline:** The codebase has moved substantially past the `per-game-translation-
survey.md` baseline (2026-05-28). All three canonical boundaries verify clean as
single-producers with no downstream per-game branch. Most survey-era gaps are
either **closed** (BSLightingShaderProperty monolith split into
`parse_skyrim`/`parse_fo4`/`parse_fo76_plus`; shader.rs `bsver()` sites 12+ → 5;
RACE Skyrim mis-decode gated under #1629; SCOL/PKIN/MOVS/MSWP game-gated; XCLL
size sanity gate; `PlacementLodProvider` for Oblivion/FO3/FNV now live) or
**tracked as open issues** (#337, #977, #1659, #1718, #1731, SK-D3-02). This
audit surfaces no new canonical-boundary leak; findings are (a) verification of
still-open tracked gaps and (b) two doc/tracking hygiene items.

---

## Boundary verification results (no findings — recorded for the trail)

| Layer | Claim verified | Result |
|---|---|---|
| **Coordinate system** | `(x,z,-y)` swap + `EXTERIOR_CELL_UNITS` single-source | **Clean.** No duplicated axis-swap or `4096.0` cell literal outside `math/coord.rs`. The lone `.z,-y` hit (`import/collision.rs:2337`) is a test assertion; `RENDER_ORIGIN_SNAP = 4096.0` is a distinct documented constant. |
| **NIFAL — material** | `translate_material` is sole populated-`Material` producer | **Clean.** Only callers: `scene/nif_loader.rs:828`, `cell_loader/spawn.rs` (via `cell_loader.rs`). `cornell.rs`/`helpers.rs` are test/reference scenes; `PrecombineMaterial` is a distinct type. No `metalness_override`/`classify_pbr` render-time reappearance. |
| **EXAL — env resources** | `env_translate.rs` sole `SkyParamsRes`/`WeatherDataRes`/`CellLightingRes` producer | **Clean.** All non-`env_translate` construction sites (`weather.rs:1171/1210/1305`, `render/lights.rs`, `audio.rs`, `cornell.rs`) are inside `#[cfg(test)]`. `default_water_for_worldspace` GameKind branch is the single exterior per-game decision. |
| **PHYSAL — ragdoll** | one translate (`template_from_imported`/`activate_ragdoll`), one sink (`build_ragdoll`) | **Clean.** `extract_ragdoll` has no `game ==` branch; `build_ragdoll` has one non-test caller; zero Rapier types leak outside `crates/physics`. |
| **EXAL — LOD placement** | `PlacementLodProvider` (Oblivion/FO3/FNV) live | **Live.** `stream_placement_lod_blocks` wired at bootstrap (`world_setup.rs:624`) and per-frame (`main.rs:1369`). |

---

## Findings

### LC0702-01: EXAL distant-object LOD full-model cull (VWD flag) still absent
- **Severity**: MEDIUM
- **Dimension**: EXAL — LOD distance rendering
- **Location**: `crates/plugin/src/esm/` (record-header flag parse), `byroredux/src/cell_loader/object_lod.rs:97,297`, `byroredux/src/cell_loader/placement_lod.rs:395`
- **Status**: Existing: #1731 (LC-D7-02)
- **Description**: The "Visible-When-Distant" / "Has Distant LOD" base-record-header flag (`0x00010000`) is the one runtime signal the real engine reads to **cull the full model** once its quad's `.bto` / `_far.nif` LOD is active. Redux does not parse it. Both the object-LOD (`.bto`) and placement-LOD (`_far.nif`) paths avoid the resulting full-mesh + LOD-mesh z-fight conservatively by loading distant geometry **only outside the full-detail ring** — a coarser rule than the flag would give.
- **Evidence**: `object_lod.rs:297` — "The full-model VWD cull is deferred; quads load only outside the full-detail ring." `placement_lod.rs:395` — "VWD object in a `.lod` renders its full mesh at distance." No `0x00010000` / `VWD` / `has_distant_lod` parse in `crates/plugin`.
- **Impact**: A full REFR that sits right at the full-detail/LOD boundary can render alongside its LOD proxy → z-fight / doubled draw at the ring seam. Blast radius is the transition band only; the conservative ring rule prevents it in the common case.
- **Related**: EXAL §5.2 VWD culling rule, §5.4; SKILL.md Dimension 5.
- **Suggested Fix**: Parse the record-header VWD flag; have the streaming ring suppress the full REFR beyond the full-detail radius when its quad's LOD is active. Tracked — no new issue needed.

### LC0702-02: FNV/FO3 ragdoll body + dependent constraints dropped silently on bone-name miss
- **Severity**: MEDIUM
- **Dimension**: PHYSAL — bone resolution (string interning / Dimension 7)
- **Location**: `byroredux/src/ragdoll.rs:81-120` (`template_from_imported`)
- **Status**: Existing: #1718 (FNV-D7-01)
- **Description**: `template_from_imported` resolves each Havok body's host-bone name against the loaded skeleton's `name → EntityId` map; a body whose bone name fails to resolve is `continue`d (dropped), and constraints referencing a dropped body are remapped/dropped. A single misspelled or case-variant bone name silently prunes part of the articulation with no diagnostic, and if the survivors fall below the ≥2-body/≥1-joint gate the whole ragdoll returns `None`.
- **Evidence**: `ragdoll.rs` — `let Some(&bone) = skel_map.get(&b.bone_name) else { continue };` then `... return None;` when the surviving graph is under-threshold.
- **Impact**: Partial or fully-missing ragdoll on any actor whose Havok bone names diverge from the loaded skeleton's interned names (the exact string-interning fidelity concern in Dimension 7 — bone-name → entity resolution is load-bearing for both skinning and PHYSAL binding). No user-facing error.
- **Related**: PHYSAL §3 translate; NIFAL §"String interning" note; SKILL Dimension 7.
- **Suggested Fix**: Emit a warning listing the unresolved bone names (case-normalised compare would catch the most likely miss class), so a silent drop becomes a diagnosable one. Tracked.

### LC0702-03: BSDismemberSkinInstance per-partition body-part flags parsed but discarded
- **Severity**: LOW
- **Dimension**: NIFAL — passthrough inventory (Skinning slice)
- **Location**: `crates/nif/src/blocks/skin.rs:372-398` (`BodyPartInfo`), `crates/nif/src/import/mesh/bs_tri_shape.rs:324`, `crates/nif/src/import/types.rs:292`
- **Status**: Existing: #1659 (SKY-D3-03)
- **Description**: `BSDismemberSkinInstance` per-partition `body_part` flags are decoded for byte-correctness but never surfaced onto the `Imported*` tier — the payload is invisible to the importer. This is a NIFAL passthrough parked on the raw tier (correct per the no-fabrication rule: the dismemberment / locational-damage consumer does not exist).
- **Evidence**: `types.rs:292` — "dismemberment / body-part segmentation payload invisible to [importer]." `bs_tri_shape.rs:324` — "dismemberment / body-part-segmentation system will consult [these later]."
- **Impact**: No visible-content loss today (Skyrim/FO4 render fine without the segmentation); blocks a future dismemberment/locational-damage feature. Bounded, documented.
- **Related**: NIFAL §"Passthroughs" (`bs_sub_index` sibling).
- **Suggested Fix**: When the dismemberment system lands, surface `body_part` per partition onto `ImportedSkin`. No action until then. Tracked.

### LC0702-04: SKILL.md Dimension 5 mis-states PlacementLodProvider as "still unimplemented"
- **Severity**: LOW
- **Dimension**: Doc-rot (audit-skill authoritative-path convention)
- **Location**: `.claude/commands/audit-legacy-compat/SKILL.md:173`
- **Status**: NEW
- **Description**: The audit skill's Dimension 5 text asserts: "The Oblivion/FO3/FNV placement scheme (`DistantLOD\*.lod` → `_far.nif`) is still **unimplemented**." This is stale — `byroredux/src/cell_loader/placement_lod.rs` exists, parses the `.lod` SoA format (#1726), and `stream_placement_lod_blocks` is wired at both stream sites. `exal.md` §2 is already corrected (it credits the live `PlacementLodProvider`); only the skill lags.
- **Evidence**: `placement_lod.rs:111` (`parse_placement_lod`), `world_setup.rs:624` + `main.rs:1369` (both stream call sites), `exal.md` §2 line ~148 (correct). Contradicts SKILL.md:173.
- **Impact**: A future `/audit-legacy-compat` run treats the skill's own factual claims as authoritative (per the `_audit-common.md` path-reference convention) and could re-file the placement scheme as an open gap — exactly the stale-premise class `feedback_audit_findings.md` warns about.
- **Related**: `exal.md` §5.2 / §Q3 (`DistantLOD\*.lod` RESOLVED #1726).
- **Suggested Fix**: Update SKILL.md Dimension 5 to reflect that the placement provider is live (Skyrim+/FO4 `.btr`/`.bto` and Oblivion/FO3/FNV `_far.nif` all consumed); the remaining LOD gaps are the VWD cull flag (#1731) and coarser LOD bands (8/16/32).

### LC0702-05: WRLD NAM3/NAM4 LOD-water + OFST cell-offset table skipped, untracked
- **Severity**: LOW
- **Dimension**: EXAL — exterior record coverage
- **Location**: `crates/plugin/src/esm/records/` (WRLD walker)
- **Status**: NEW
- **Description**: `exal.md` §5.4 records that the WRLD `NAM3`/`NAM4` LOD-water fields and the `OFST` cell-offset table are "currently skipped in `wrld.rs`" and feed the LOD ring rather than the full-detail scene. Unlike the VWD flag (#1731), no open issue tracks this skip. It is a bounded known gap (the LOD-water consumer does not exist yet, so surfacing the fields now would invent a resource nothing reads — the no-fabrication rule), but it is un-tracked, so it risks being re-derived from scratch in a later audit.
- **Evidence**: `exal.md` §5.4 — "What runtime LOD **does** need that we don't parse yet ... the WRLD `NAM3`/`NAM4` LOD-water fields + `OFST` cell-offset table currently skipped in `wrld.rs`." No matching open issue (grep of `issues.json` for `nam3`/`nam4`/`ofst`/`lod-water` → 0 hits).
- **Impact**: None at runtime today (distant LOD water is not modelled). Purely a tracking gap: the deferred parser work is documented in the spec but not in the issue tracker, so it can be forgotten or re-investigated.
- **Related**: #1731 (the sibling VWD parser gap, which *is* tracked); EXAL §5.4 / GameVariant §4 "Distant terrain source" row.
- **Suggested Fix**: File a low-priority tracking issue mirroring #1731's shape ("WRLD NAM3/NAM4 LOD-water + OFST skipped — parser gap for future LOD-water"), or fold it into the #1731 LOD-parser follow-up so the deferred work has a tracker.

---

## Documented limitations re-confirmed (NOT findings — do not re-file)

Per the SKILL's leak-inventory cross-check, these are bounded, spec-documented
limitations verified still-accurate this pass:

- **FO4/FO76/Starfield ragdolls** — blocked on the `BhkNPCollisionObject →
  BhkSystemBinary` blob decoder (multi-day RE); static collision falls back to a
  synthesised trimesh. (PHYSAL §5; NIFAL §"Collision".)
- **`BhkPCollisionObject` phantoms** — parked pending a `TriggerVolume` ECS path,
  not a rigid body. (PHYSAL §5.)
- **NIFAL parked passthroughs** — `bs_value_node`, `bs_ordered_node`, `tree_bones`,
  `range_kind`, `bs_lod_cutoffs`, `lod_group` (`NiLODNode` content-absent),
  `bs_sub_index`, furniture/inv markers, `NiSwitchNode` discriminator,
  `NiTextureEffect` (content-absent). Each blocked on a not-yet-built consumer.
  (NIFAL §"Nodes" / §"Passthroughs".)
- **NiStencilProperty stencil-test** (#337), **no_lighting_falloff** (SK-D3-02),
  **is_sky_object / water_shader_flags render dispatch** (#977 follow-up) — all
  captured at import, renderer-side dispatch tracked. (`import/material/mod.rs`.)
- **NiFogProperty** — intentionally not dispatched (#1224 / D4-NEW-02); 1 vanilla
  FO3 block; the fog path reads cell-scope `CellLighting`. (NIFAL §"Material".)
- **Emissive scale** — three `EmissiveSource` variants measured to share ~1.0
  scale; no normalization is correct. (NIFAL §4.)
- **Sun latitude** — no authored CLMT/WRLD latitude field exists; `SUN_SOUTH_TILT
  = 0.15` is an engine-defined constant, not a parse gap. (EXAL §9 Q1; #1019
  premise false.)

---

## Summary

- **Total findings**: 5
- **CRITICAL**: 0
- **HIGH**: 0
- **MEDIUM**: 2 (LC0702-01 #1731, LC0702-02 #1718)
- **LOW**: 3 (LC0702-03 #1659, LC0702-04 NEW doc-rot, LC0702-05 NEW tracking gap)

Two findings are NEW (both LOW: an audit-skill doc-rot and an untracked bounded
parser gap); three are verifications of already-open tracked issues. No new
canonical-boundary (NIFAL/EXAL/PHYSAL) leak was found — all four single-producer
boundaries and the coordinate-system single-source verified clean. The
absence of HIGH/CRITICAL reflects that the survey-era leaks have been closed or
tracked; the compat surface is in good shape.

---

*Next step:* `/audit-publish docs/audits/AUDIT_LEGACY-COMPAT_2026-07-02.md`
(only LC0702-04 and LC0702-05 are NEW; the three verifications should be skipped
or added as comments on their existing issues).
