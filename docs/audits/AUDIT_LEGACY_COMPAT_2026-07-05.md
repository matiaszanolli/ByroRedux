# Legacy Compatibility Audit — 2026-07-05

**HEAD:** a8d65d6c · **Type:** legacy-compat (canonical-translation boundary pass)

**Scope:** Compatibility/mapping gaps between Gamebryo 2.3 / Creation-engine
behaviour and Redux, framed by the three canonical translation layers
(NIFAL / EXAL / PHYSAL) plus coordinate-system correctness, the per-game
translation survey, and subsystem coverage vs. the legacy headers.

**Method:** **Delta pass** re-run at HEAD a8d65d6c over the same-day base report
(originally written at 8b50e238). Since 8b50e238, exactly **one commit** landed:
`a8d65d6c` (#1889 — materialise the VWD flag as a per-placement
`VisibleWhenDistant` marker). This is the commit that *closes* the sole carryover
item this report carried (LC0705-01, the VWD full-model-cull consumer that had no
issue filed): #1889 was opened from LC0705-01 and fixed in the same commit. I
re-ran every boundary single-producer grep at a8d65d6c, traced the #1889 diff
against all six audit dimensions to confirm it introduces no per-game branch and
no boundary violation, deduplicated against the 36 open issues
(`/tmp/audit/issues.json`, #1889 now **CLOSED**) and the per-layer specs, and
attempted to disprove each candidate before inclusion.

The earlier base-report method note (60-commit delta over the 2026-07-03 report at
8498e559, headline `450691e0` #1838/#1839 raw-BSVER gate restore) still stands as
the provenance of the 8b50e238 baseline; the paragraph below folds the single
a8d65d6c delta on top of it.

**Headline:** All four canonical boundaries (material / env / coord / ragdoll)
remain single-producer clean at a8d65d6c; the renderer still carries **zero**
`if game == …` branches, and the coordinate single-source (`(x,z,-y)` swap +
`EXTERIOR_CELL_UNITS`) has no new duplicate. The lone code delta — `a8d65d6c`
(#1889) — *closes* the one carryover tracking gap by materialising the
Visible-When-Distant record-header flag as a per-placement `VisibleWhenDistant`
ECS marker. It is a textbook clean parse→ECS-state materialisation: the flag read
(`RecordHeader::is_visible_when_distant`, a pure `flags & 0x00010000` bit test) is
game-agnostic, it carries onto `StaticObject::visible_when_distant` at every
construction site, and the cell-load insert (`if stat.visible_when_distant`) has
**no per-game branch and no render-time consumer** — a materialised hook for a
future full-model cull, not a coord/material/env/ragdoll boundary touch. **No NEW
code defect surfaced, and the sole prior carryover is now resolved.**

---

## Boundary verification results (no findings — recorded for the trail)

| Layer | Claim verified | Result |
|---|---|---|
| **Coordinate system** | `(x,z,-y)` swap + `EXTERIOR_CELL_UNITS` single-source, survives #1876/#1877 splits + #1889 | **Clean.** Re-grepped at a8d65d6c: no duplicated axis-swap or raw `4096.0` cell literal outside `crates/core/src/math/coord.rs`. Every `EXTERIOR_CELL_UNITS` hit (`streaming.rs`, `terrain_lod*.rs`, `cell_loader/{spawn,water,exterior}.rs`) is a *use* of the canonical constant. All `(x, z, -y)` hits are **doc comments** describing the canonical convention, not re-derivations. #1889 touched neither coord.rs nor any cell-math literal. |
| **NIFAL — material** | `translate_material` sole populated-`Material` producer | **Clean.** Only two non-test callers: `byroredux/src/cell_loader/spawn.rs:911` and `byroredux/src/scene/nif_loader.rs:838`, both delegating. `634873db` (#1873) tightened the PBR env-map lift **inside** `resolve_pbr`/classifier (gated on authored specular, not the struct default) — no new producer, no render-time `classify_pbr` reappearance. |
| **NIFAL — collision shape** | shape dispatch↔resolve parity preserved through the #1876 split | **Clean.** `crates/nif/src/import/collision/shape.rs::resolve_shape` retains every arm (Ball / MultiSphere / Cuboid / Capsule / Cylinder / ConvexHull / Compound / ConvexList / TriMesh). The split was a bit-for-bit no-op. |
| **EXAL — env resources** | `env_translate.rs` sole `SkyParamsRes`/`WeatherDataRes` producer | **Clean.** The two other construction sites (`systems/weather.rs:1210`, `render/lights.rs:296`) sit below `#[cfg(test)]` at 1153 / 227 respectively. The single sanctioned per-`GameKind` exterior branch remains `default_water_for_worldspace` (`env_translate.rs:65`, the documented EXAL GameVariant-table prototype). |
| **PHYSAL — ragdoll** | one translate, one sink; extract game-agnostic | **Clean.** `crates/nif/src/import/collision/ragdoll.rs::extract_ragdoll` switches on `BhkConstraintData` (Ragdoll / LimitedHinge / Other), never on game; `scale = scene.havok_scale` is precomputed via `havok_scale_for`. `ae083d69`'s zero-mass reclassify is gated on `mass<=0` (game-agnostic Havok→Rapier semantics), not `game ==`. Zero Rapier types leak outside `crates/physics`. |
| **Renderer per-game branch** | render side carries no `if game == …` | **Clean.** Re-grepped at a8d65d6c for `game ==` / `GameKind::` / `is_skyrim` / `is_fo4` / `is_starfield` across `byroredux/src/render` + `crates/renderer/src` → zero hits. All downstream `GameKind` branches live in the sanctioned upstream boundaries: EXAL LOD providers (asset-driven `.bto`/`.btr`/`_far.nif` selection, EXAL §5) and `env_translate.rs` water default. |
| **VWD marker (new, #1889)** | VWD flag materialisation adds no per-game branch, no boundary violation | **Clean.** `RecordHeader::is_visible_when_distant()` (`reader.rs:384`) is a pure `flags & 0x00010000` bit test — game-agnostic across the whole TES4+ record format. The value rides `StaticObject::visible_when_distant` through all four construction sites (MODL walker, grup_walker, SCOL/PKIN/MOVS) and the cell-load insert (`references/mod.rs:753`, `if stat.visible_when_distant { world.insert(root, VisibleWhenDistant) }`) has zero per-game logic. The marker has **no render-time consumer** by design (documented on the component + EXAL §5.2) — a materialised hook, not a coord/material/env/ragdoll touch. Not a leak. |

---

## Compat-relevant code deltas verified this pass (no findings)

- **`a8d65d6c` (#1889) — VWD flag → `VisibleWhenDistant` marker (the sole delta at
  this HEAD).** Materialises the base-record "Visible-When-Distant" / "Has Distant
  LOD" header flag (`0x00010000`, parsed under #1731) as a per-placement ECS
  marker, closing the LC0705-01 carryover this report previously tracked. Verified
  against every dimension: (1) the flag read `RecordHeader::is_visible_when_distant`
  is a pure bitflag test, game-agnostic — **no Pattern-A/B/C branch** introduced
  (Dimension 6); (2) it carries onto `StaticObject::visible_when_distant` at all
  four construction sites (`support.rs::build_static_object_from_subs` + the
  SCOL/PKIN/MOVS paths + `grup_walker.rs`), each threading
  `header.is_visible_when_distant()` — no site fabricates or hardcodes it; (3) the
  cell-load insert (`references/mod.rs:753`) is gated purely on the per-record bool,
  **not on game** — no EXAL/coord/material/ragdoll boundary is touched; (4) the
  marker has **no render-time consumer**, by explicit design (the conservative
  streaming ring already guarantees full model + LOD proxy never coexist, #1866), so
  there is no render-time fallback or per-draw heuristic. It is a clean
  parse→ECS-state materialisation — a hook, not a translation. Regression tests pin
  the flag flowing end-to-end (`tests/addn_stat.rs` — set when present, false
  otherwise). **Not a finding.**
- **`450691e0` (#1838/#1839) — raw-BSVER gate restore.** The #1277 Task-5 refactor
  had migrated four version-gated field reads (ni_tri_shape shader/alpha refs +
  material CRC; MOPP Build Type; BSMultiBoundNode Culling Mode) from raw
  `stream.bsver()` compares to `variant().has_*()` helpers. nif.xml gates all four
  purely on BSVER, so the helpers diverged from spec on hybrid `Unknown`-variant
  headers (under-read at `uv=11, bsver 35..=82`; over-read at `uv=12, bsver < gate`).
  This commit **restores the spec-correct raw-bsver reads** and adds four
  regression tests that trip red on revert. This is textbook **Pattern A** hygiene
  (§Dimension 6) resolved in the *correct* direction — a positive delta. Verified
  the restored gates match nif.xml (`> FO3_FNV`, `>= SKYRIM_LE`) and that the
  now-orphaned helpers still carry their own version.rs unit tests (removal
  tracked separately as NIF-D2-03). **Not a finding.**
- **`ae083d69` (#1832 partial) — zero-mass Dynamic reclassify.** Game-agnostic
  Havok→Rapier semantic translation (Havok special-cases zero-mass "dynamic"
  bodies as immovable world geometry; Rapier integrates them → free-fall). Gated
  on `mass<=0` + Dynamic-family motion type, at the import/collision boundary, no
  `game ==`. Within PHYSAL contract. Tracked under still-open **#1832** (door-
  threshold spawn gap explicitly left open). **Not a finding** (see also
  `tes_grounding_zero_mass_dynamic_fix.md` — "don't re-investigate the mass=0
  angle again"). |
- **`634873db` (#1873) — PBR env-map metalness gate.** Tightened the classifier so
  an authored-white specular is distinguished from the `[1,1,1]` struct default
  (`MaterialInfo::has_material_data`). Stays inside the single material boundary;
  no new producer, no render-time classifier. **Not a finding.**
- **`41152f13`/`9f12b2eb` (#1876/#1877) — module splits.** `import/collision.rs`
  (2587 LOC) → `collision/{mod,ragdoll,shape}.rs`; `cell_loader/references.rs`
  (2078 LOC) → submodule dir. Verified translate-arm parity (above) and that the
  coord/material/ragdoll single-producer greps still resolve correctly.
  **Not a finding.**

---

## Findings

**No NEW code defects, and no open carryover.** The single item this report
previously carried is now closed:

### LC0705-01: VWD full-model-cull consumer — RESOLVED as #1889 (`a8d65d6c`)
- **Severity**: LOW → **CLOSED**
- **Dimension**: EXAL — LOD distance rendering (Dimension 5)
- **Location**: `crates/plugin/src/esm/reader.rs:384` (`is_visible_when_distant`, producer), `byroredux/src/components.rs` (`VisibleWhenDistant` marker, NEW), `byroredux/src/cell_loader/references/mod.rs:753` (materialisation site, NEW)
- **Status**: **CLOSED** — the tracking issue LC0705-01 recommended was filed as **#1889** and fixed in the same commit `a8d65d6c` (verified CLOSED in `/tmp/audit/issues.json` / `gh issue view 1889`).
- **Resolution**: The prior pass flagged that the VWD flag (parsed under #1731) had zero production consumers and no follow-up issue. #1889 materialises the flag onto `StaticObject::visible_when_distant` at every construction site and inserts a per-placement `VisibleWhenDistant` ECS marker at cell load. This is the ECS-state hook LC0705-01 called for. An *active* render-time cull is still deferred (documented on the marker + EXAL §5.2 as design, not a gap — the conservative streaming ring, #1866, guarantees a full model and its LOD proxy never coexist, so there is no z-fight to cull today). The materialisation is verified clean against every dimension (see the code-delta entry above) — no per-game branch, no boundary violation.
- **Related**: #1731 (CLOSED, parse scope); #1889 (CLOSED, materialisation); #1866 (streaming ring rule); EXAL §5.2 / §5.4; SKILL Dimension 5.

---

## Still-open tracked gaps re-verified (Existing — do not re-file)

- **#1889** (LC0705-01) — VWD full-model-cull consumer. **CLOSED** this pass by `a8d65d6c` (flag materialised as `VisibleWhenDistant` marker; active cull still design-deferred per EXAL §5.2 / #1866). Do not re-file.
- **#1849** (LC0702-05) — WRLD NAM3/NAM4 LOD-water + OFST cell-offset table skipped. OPEN, correct.
- **#1850 / #1851** (FNV-D7-02/03) — **CLOSED** this session by `88d41600` (bhkBreakableConstraint dropped edges now surfaced; measured joint counts pinned). The underlying design (case-sensitive bone lookup matching Gamebryo `NiFixedString` exact-match interning) is correct, not a finding.
- **#1852** (FNV-D7-04) — ragdoll writeback uses live `gt.scale` in the inverse while the seed captured scale at activation. OPEN, PHYSAL-slice item owned by `/audit-fnv`.
- **#1659** (SKY-D3-03) — BSDismemberSkinInstance body-part flags parsed/discarded. OPEN, NIFAL parked passthrough (no dismemberment consumer).
- **#1856** (FO3-D1-01) — WaterShaderProperty flags dead-end at MaterialInfo. OPEN, NIFAL passthrough.
- **#1832** — TES grounding / door-threshold spawn gap. OPEN, partial fix `ae083d69`; the mass=0 angle is closed, the door-threshold seam is a separate open item.

## Documented limitations re-confirmed (NOT findings — do not re-file)

- **FO4/FO76/Starfield ragdolls** — blocked on `BhkNPCollisionObject → BhkSystemBinary` decoder (PHYSAL §5).
- **`BhkPCollisionObject` phantoms** — parked pending a `TriggerVolume` ECS path (PHYSAL §5).
- **NIFAL parked passthroughs** — `bs_value_node`, `bs_ordered_node`, `tree_bones`, `range_kind`, `bs_lod_cutoffs`, `lod_group`, `bs_sub_index`, `NiSwitchNode`/`NiTextureEffect` (content-absent).
- **NiFogProperty** — intentionally not dispatched (#1224); reads cell-scope `CellLighting`.
- **Emissive scale** — three `EmissiveSource` variants share ~1.0 scale; no normalization is correct (NIFAL §4).
- **Sun latitude** — no authored CLMT/WRLD latitude field exists; `SUN_SOUTH_TILT` is engine-defined (#1019 premise false; EXAL §9 Q1).
- **CHARAL rulesets** — the six `charal-*-ruleset.md` docs added this session are design specs for a PROPOSED layer (`charal.md` PROPOSED 2026-06-29). The implemented translation covers FNV/FO3 ActorValue population; the Oblivion/Skyrim/Starfield/FO76 rulesets are documented-ahead-of-code by design (`/audit-charal` territory), not a legacy-compat *translation regression*.

---

## Summary

- **Total NEW findings**: 0
- **CRITICAL / HIGH / MEDIUM / LOW**: 0
- **Carryover**: 0 open — the sole prior item (LC0705-01) is now CLOSED as #1889.

The compat surface is clean at a8d65d6c with **zero open findings**. All four
single-producer canonical boundaries (material / env / coord / ragdoll) and the
coordinate single-source re-verify clean; the renderer carries zero per-game
branches. The lone commit since the base report — `a8d65d6c` (#1889) — closes the
one carryover item by materialising the Visible-When-Distant record-header flag as
a per-placement `VisibleWhenDistant` ECS marker. That change was audited against
all six dimensions and is a textbook clean parse→ECS-state materialisation: the
flag read is a game-agnostic bit test, it threads through every `StaticObject`
construction site without fabrication, and the cell-load insert carries no
per-game branch and no render-time consumer (the active cull remains design-
deferred behind the conservative #1866 streaming ring, per EXAL §5.2). No new
per-game leak, boundary violation, or dropped-content gap surfaced.

---

*Next step:* none required — 0 open findings. #1889 (the prior carryover) is
already CLOSED; no issue to publish.
