# NIFAL Audit — Canonical Translation Layer — 2026-07-16

Deep audit of **NIFAL** (the NIF Abstraction Layer; spec: `docs/engine/nifal.md`).
Scope: the *translate* side — does each parsed NIF data category reach one
canonical, game-agnostic representation through a single explicit `translate()`
boundary, with no leak / fabrication / render-time fallback? (Parse-side
correctness belongs to `/audit-nif`.)

All 9 dimensions re-verified from scratch against the live tree @ HEAD
(`c3e09bb5`). Dedup baseline: `/tmp/audit/issues.json` (28 OPEN issues) +
`docs/audits/AUDIT_NIFAL_2026-07-06.md` (HEAD `d59f40ac`, 0 findings) and the
seven prior NIFAL sweeps back to `AUDIT_NIFAL_2026-05-30.md`.

Delta since the 2026-07-06 sweep: **80 commits** (`d59f40ac..c3e09bb5`). Every
commit was triaged for NIFAL relevance; 7 touch the translate surface and are
detailed below — one lands a genuine finding, one lands a new (partially
converged) translated category, and five land clean.

---

## Executive Summary

**Severity tally: 0 CRITICAL · 1 HIGH · 1 MEDIUM · 0 LOW. 0 regressions. 2 NEW findings.**

This remains a mature, heavily-converged system — but this sweep's independent
re-derivation (as opposed to re-stating prior claims) surfaced a real defect
that six previous sweeps missed: **MAT-D1-01**, an unbounded substring
collision in the PBR keyword classifier's glass arm (`"ice"` matches `office`,
`notice`, `device`, …), which is the *same root-cause class* as the already-closed
`#1819` (SpeedTree `"genericelderberry"` → glass) — that fix patched the one
reported symptom (a SpeedTree bypass) without fixing the underlying
word-boundary gap in the shared classifier, leaving every other consumer
(every Oblivion/FO3/FNV legacy mesh) exposed.

The second finding, **D4-01**, is on a brand-new translated category:
commit `004b51c7` (2026-07-11) built a genuine `BSFurnitureMarker →
ImportedFurnitureMarker → Furniture` translate path, closing a passthrough gap
the spec's Nodes table had listed since 2026-05-28. The translate boundary
itself is clean (single construction site, honest parked legacy-`Orientation`
field, no fabrication), but the downstream M42 sandbox-seating consumer
re-resolves the canonical `Option<f32>` heading field with a per-era gameplay
heuristic — a textbook no-leak pattern, self-documented and opt-in
(`BYRO_SANDBOX_SIT`, off by default) but worth tracking as the feature matures.

All other tier invariants — **single-boundary**, **no-fabrication**,
**no-leak**, **no-render-time-fallback** — pass on every other category, with
re-derived evidence:

- `translate_material` has exactly **2** production callers
  (`byroredux/src/scene/nif_loader.rs:838`, `byroredux/src/cell_loader/spawn.rs:917`).
  No stray `Material {` construction site outside the boundary, `cornell.rs`
  (out-of-scope RT harness), and test files.
- `resolve_pbr` clamps `metalness ∈ [0,1]` / `roughness ∈ [0.04,1]`
  (`crates/core/src/ecs/components/material.rs:686-712`) and only overwrites
  individual `NaN`-sentinel fields — never an authored BGSM/BGEM override.
- Glass classified once, after `resolve_pbr`, inside `translate_material`
  (`material_translate.rs:160-168`).
- `resolve_shape_inner` carries **16** `downcast_ref::<Bhk*Shape>` arms
  (`crates/nif/src/import/collision/shape.rs`), cross-checked struct-by-struct
  against every parsed `Bhk*Shape` — no missing arm. `BhkPlaneShape` remains
  the one documented `None` exception.
- `hkMotionType` byte → canonical `MotionType` full-enum collapse intact
  (`crates/nif/src/import/collision/mod.rs:156-165`).
- `apply_emitter_overlays` has exactly **2** production callers; `initial_color`
  still intentionally unapplied; force fields converted Z-up→Y-up once, at
  overlay time, not per-frame.
- `convert_nif_clip` remains the sole NIF→core `AnimationClip` boundary (6
  legitimate callers, one function).
- `crates/renderer/shaders/` (incl. `include/*.glsl`) has **zero** `if game ==`
  occurrences.
- All pre-existing raw-tier-parked Dim-4 fields (`bs_value_node`,
  `bs_ordered_node`, `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs`,
  `bs_sub_index`) still have zero canonical consumers.
- FO4 `model_space_normals` / `alpha_test` still reach `MaterialInfo`; the
  companion `alpha_threshold` gap (`#1985`) is now also closed cleanly
  (verified import-time-only, citing the documented `nif.xml` `Threshold`
  default of 128, correctly gated on `!alpha_property_consumed`).
- `translation_completeness.rs` fill-rate floors unchanged (`>= 60.0`).

### Commits since the 2026-07-06 sweep (`d59f40ac..c3e09bb5`) — NIFAL-relevance triage

| Commit | Touches NIFAL surface? | Verdict |
|---|---|---|
| `e1b0294d` #1925 — narrow "scrap" PBR classifier keyword to "metalscrap" | **Yes** — Material (`crates/core/src/ecs/components/material.rs`) | **Fix itself lands correctly** (verified: bare `scrapmetal*` now reaches the conductive-metal arm, `metalscrap*` cladding still matte). **But its own "swept for other order-sensitive substring arms, none found" claim is disproven** — see MAT-D1-01 below, found in the same function. |
| `441186fb` #1985 — seed FO4 shader-flag-only alpha-test threshold | **Yes** — Material/Shader-flags (`crates/nif/src/import/material/walker.rs`) | **Clean fix.** Import-time only; `128.0/255.0` is `nif.xml`'s documented `NiAlphaProperty.Threshold` default (not fabricated) and matches the codebase's existing `/255.0` convention for the same field; correctly gated on `!info.alpha_property_consumed` (set earlier in the same function only when a real `NiAlphaProperty` was consumed) so it never clobbers an authored value. |
| `5ffb7638` #1580 — forward BGEM `grayscale_to_palette_alpha` into `EFFECT_PALETTE_ALPHA` | **Yes** — Shader-flags (`byroredux/src/asset_provider/material.rs`, `byroredux/src/cell_loader.rs`) | **Clean fix.** New `ImportedMesh::bgsm_greyscale_lut_is_alpha` field set at exactly one merge site, consumed at exactly one flag-packing site, routes through the existing `translate_material` boundary (via `pack_bgsm_material_flags`, one of the three OR'd `effect_shader_flags` contributors) — no second construction site. `EFFECT_PALETTE_ALPHA`/`EFFECT_PALETTE_COLOR` confirmed mutually exclusive. |
| `120c4635` #1659 — surface `BSDismemberSkinInstance` body-part flags onto `ImportedSkin` | **Yes** — Skinning (`crates/nif/src/import/mesh/skin.rs`, `crates/nif/src/import/types.rs`) | **Clean, new parked-not-leak field.** New `ImportedSkin::body_part_flags: Vec<BodyPartInfo>`, threaded symmetrically through both `NiTriShape`/`BsTriShape` extraction paths, honest empty-vec default for the three no-dismemberment paths. Whole-codebase grep confirms **zero** consumers outside `crates/nif/src/import/` (parser/import/tests) — legitimately parked, not a leak. Recommend adding to the documented-limitation ledger (see below). |
| `004b51c7` M41.5 — NPC idle variety + furniture-marker foundation | **Yes** — Nodes (`crates/nif/src/import/mod.rs`, `crates/nif/src/import/types.rs`, `crates/core/src/ecs/components/furniture.rs`) | **New translated category, partially converged.** Builds a genuine `BSFurnitureMarker → ImportedFurnitureMarker → Furniture` translate boundary (single construction site `furniture_component`, one call site) closing a passthrough gap the spec had documented as unconsumed. Clean on fabrication/coordinate-conversion/leak checks for the raw→canonical step itself — but see **D4-01** below for a downstream no-leak violation in the M42 sandbox-seating consumer, and a minor undocumented structural asymmetry (furniture only spawns on the cell-loader path, never the loose-NIF path — unlike the `Nodes`/`BSBound` asymmetries, this one isn't called out anywhere yet). The idle-variety half of the commit (`npc_spawn.rs`) was checked and confirmed to route through the existing `convert_nif_clip` pipeline with no second construction path. |
| `55105a2e` #1922/#1923/#1924 — correct three stale/misleading comments | Partial — one hunk touches `material.rs` (`crates/core/src/ecs/components/material.rs`) | **Doc-only.** Anchors the "metalscrap" arm's true commit-of-origin in a comment; no logic change. Out of scope otherwise (texture-registry and BLAS-scratch doc fixes are renderer-side, not NIFAL). |
| `977eb95a` — docs-only audit report addition | No | Adds an unrelated scripting-subsystem audit report; no code change. |
| remaining ~73 commits | No | M42 Wander/Travel/Follow/Escort/Guard/Patrol AI procedure runtimes, sandbox-seat PLDT decode, renderer/RT doc-rot fixes, audio subsystem audit, cross-cutting docs (save/load, streaming, NPC-spawn), SVGF/composite shader fixes, FFI safety-comment sweep, PEX/scripting decline-path fixes, CSG chunk validation — none touch `crates/nif/src/import/`, `material_translate.rs`, `anim_convert.rs`, `systems/particle.rs::apply_emitter_overlays`, or the collision/shader-flag translate surface beyond what's listed above. |

### Convergence status vs spec §2 leak inventory

| Category | Spec §2 status | Verified 2026-07-16 |
|---|---|---|
| Materials | converged | ⚠️ single boundary intact, plain-`f32` resolved PBR, glass-once ordering — but **MAT-D1-01 (HIGH, NEW)** found: the glass arm's unbounded `"ice"` substring match misclassifies common English words as glass. |
| Geometry / transform | converged | ✅ zero commits touched this category's entry points; all baseline claims re-derived and hold. |
| Skinning | converged | ✅ global bone indices, u16 guard intact. New `body_part_flags` field (#1659) is a clean parked-not-leak addition. |
| Lights | converged | ✅ renderer never inspects source block type (0 hits). |
| Nodes | triaged (parked-not-leak) | ✅ 7 pre-existing parked fields still zero-consumer. **`BSFurnitureMarker` graduates out of the passthrough table** — see Furniture below. |
| Furniture *(new sub-category, was a Nodes passthrough)* | n/a (new) | ⚠️ **Partially converged.** Translate boundary itself clean; downstream consumer has **D4-01 (MEDIUM, NEW)**. |
| Particles | converged (emitter overlay) | ✅ `apply_emitter_overlays` single boundary (2 callers); #1775's `start_size_variation` is a legitimate extension of the same boundary, not a new site. |
| Collision | audited/converged | ✅ 16 shape arms, `BhkPlaneShape` the one documented `None`; `hkMotionType` full-enum collapse intact. |
| Animation / controllers | converged | ✅ single `convert_nif_clip` boundary; text-key wiring intact; `AnimatedMorphWeights` now writes to a real canonical component every frame (a step past "parked on `Imported*`") but still has no renderer consumer — not a leak. |
| Shader flags / texture sets / effect shaders | converged | ✅ 0 shader `if game ==`; both delta commits (`#1985`, `#1580`) route cleanly through existing boundaries. |

---

## Per-Category Tier Matrix

| Category | Boundary fn | single-boundary | no-fabrication | no-leak | no-render-time-fallback |
|---|---|---|---|---|---|
| Material | `material_translate::translate_material` | PASS (2 callers) | **FAIL** (MAT-D1-01: classifier misfires, not an invented constant, but a wrong resolved value with no downstream correction) | PASS (plain `f32`) | PASS (0 shader branches) |
| Geometry/Transform | `import/mesh/*` + `import/coord.rs` | PASS | PASS | PASS | PASS |
| Skinning | `import/mesh/skin.rs` (extraction) | PASS | PASS | PASS (global indices; `body_part_flags` parked on raw tier) | N-A |
| Lights | `import/walk/mod.rs` → `LightKind` | PASS | PASS | PASS | PASS (no block-type match) |
| Nodes | (by design, no single boundary) | N-A (spec §2) | PASS | PASS (7 parked, 0 consumers) | N-A |
| Furniture | `cell_loader/references/attach.rs::furniture_component` | PASS (1 call site; loose-NIF path asymmetry noted, low-impact) | PASS (legacy `Orientation` honestly parked, no invented heading) | **FAIL** (D4-01: canonical `Option<f32>` re-resolved by `systems/sandbox.rs` heuristic) | N-A |
| Particles | `systems::apply_emitter_overlays` | PASS (2 callers) | PASS (initial_color unapplied) | PASS | N-A |
| Collision | `import/collision/shape.rs::resolve_shape_inner` | PASS | PASS (motion full-enum) | PASS (16 arms) | N-A |
| Animation | `anim_convert::convert_nif_clip` | PASS (6 callers, one fn) | PASS | PASS (quaternion keys; morph weights parked on real canonical component) | PASS |
| Shader flags | `shader_flags.rs` + `import/material/walker.rs` | PASS (block-type dispatch) | PASS | PASS (FO4 flags + #1580 alpha variant reach MaterialInfo) | PASS (0 shader `if game ==`) |

---

## Findings

### MAT-D1-01: `classify_pbr_keyword`'s unbounded substring match misfires the glass arm on common English words
- **Severity**: HIGH
- **Dimension**: Material
- **Tier Violated**: no-fabrication (a materially wrong value reaches the canonical `Material` at the single translate boundary with no downstream correction — matches the severity table's "wrong/divergent canonical Material out of translate_material → at least HIGH" rule)
- **Game Affected**: Oblivion, FO3, FNV primarily (any legacy inline-shader mesh reaching `classify_legacy_pbr` without a BGSM/BGEM override — i.e. every era without a resolved metalness/roughness); non-BGSM Skyrim/FO4 content shares the same exposure since `resolve_pbr`'s backstop calls the identical function.
- **Location**: `crates/core/src/ecs/components/material.rs:519-524` (the glass arm), `crates/core/src/ecs/components/material.rs:719-728` (`contains_any_ci`, the shared matcher — a pure sliding-window byte comparison with no word-boundary check, confirmed by reading the implementation directly)
- **Status**: NEW (no match in `/tmp/audit/issues.json` for `glass`/`ice`/`substring`/`word-boundary`/`classify_pbr`; closely related to CLOSED **#1819** but not the same finding — see below)
- **Description**: The alpha-unaware glass arm matches `&["glass", "crystal", "ice", "gem"]` via `contains_any_ci`, confirmed to be a raw ASCII case-insensitive **substring** match with zero word-boundary logic (`hs.windows(kb.len()).any(|w| w.eq_ignore_ascii_case(kb))` — a pure sliding window). `"ice"` is a 3-letter substring embedded in many ordinary English words that plausibly appear in Bethesda texture/mesh paths: `office`, `notice`, `device`, `justice`, `invoice`, `spice`, `voice`, `twice`, `advice`, `entice`, `artifice`, `sacrifice`, `practice`, `police`, `juice`, `dice`, `slice`. Any of these in a diffuse texture path routes the surface through the glass arm, forcing `roughness = 0.1, metalness = 0.0` — a dramatic swing from the ~0.85 matte default used for unmatched paths.

  This is not a hypothetical: the identical root-cause class was already caught and closed once in this exact function, for a different word. `docs/audits/AUDIT_NIFAL_2026-07-03.md` documents fix `1748e148` / issue **#1819** ("Foliage texture-path substring collisions in the PBR keyword classifier mis-tag vanilla trees as wood/glass", CLOSED, `high` label): a SpeedTree texture named `genericelderberry*.dds` was misclassified GLASS because `"generIC-Elderberry"` contains `"ice"` at a word seam. That fix gave SpeedTree's placeholder-billboard path an explicit bypass around the classifier (in `crates/spt/src/import/mod.rs`) — it never touched `classify_pbr_keyword`/`contains_any_ci` itself, so every other consumer of the shared classifier remains exposed to the same defect. Commit `e1b0294d` (2026-07-11, #1925) explicitly claimed to sweep this function for "other order-sensitive substring arms" and found none — that sweep did not catch this, despite the identical bug class already being a closed issue in the same function.

  A second, lower-impact instance of the same root cause: the fabric arm's `"fur"` keyword (`material.rs:540`) is a literal prefix of `"furniture"` — a common Bethesda clutter-asset directory/mesh token across every game. A plain `furniture\table01_d.dds` path with no `wood`/`stone`/`metal` token falls into the fabric/leather bucket (`roughness: 0.95`) instead of the intended matte-default fallback (`roughness: 0.85`). Noted for completeness but not independently finding-worthy — the visual delta between the two buckets is negligible (both are "fully rough, no highlight").
- **Evidence**:
  ```rust
  // material.rs:519 — alpha-unaware, unconditional
  if contains_any_ci(path, &["glass", "crystal", "ice", "gem"]) {
      return PbrMaterial { roughness: 0.1, metalness: 0.0 };
  }
  // material.rs:719 — contains_any_ci: pure substring window, no boundary check
  fn contains_any_ci(haystack: &str, keywords: &[&str]) -> bool {
      let hs = haystack.as_bytes();
      keywords.iter().any(|kw| {
          let kb = kw.as_bytes();
          if kb.is_empty() || kb.len() > hs.len() { return false; }
          hs.windows(kb.len()).any(|w| w.eq_ignore_ascii_case(kb))
      })
  }
  ```
  Call path: `classify_legacy_pbr` (`crates/nif/src/import/material/mod.rs`, called unconditionally for every extracted material at NIF-import time from `bs_tri_shape.rs`/`ni_tri_shape.rs`/`bs_geometry.rs`) → `metalness_override`/`roughness_override` `Some(...)` → `material_translate.rs:157-158` → `resolve_pbr` (non-`NaN`, clamps only, no re-classification) → canonical `Material.roughness = 0.1`.
- **Impact**: Any FO3/FNV/Oblivion (or non-BGSM Skyrim/FO4) surface whose diffuse texture path contains one of the colliding words renders with `roughness = 0.1`, below the RT reflection gate (`roughness < 0.6` in `triangle.frag`) — the surface becomes spuriously mirror-reflective ("wet floor"/chrome look) with no in-game workaround, since NIFAL's own no-render-time-fallback discipline means there is nowhere downstream to catch a wrong import-time classification.
- **Suggested Fix**: Add a word-boundary check to `contains_any_ci` (or a boundary-aware variant used specifically by short/common keywords like `"ice"`/`"fur"`/`"gem"`) so a match only counts when the preceding/following byte is not ASCII-alphanumeric (or is a path separator/string boundary). This closes the whole bug class instead of requiring a one-off keyword narrowing per incident (the `#1925` "metalscrap" fix and the `#1819` SpeedTree bypass are both prior instances of the same underlying gap being patched piecemeal, not at the root). Add a regression test asserting `office*.dds`/`notice*.dds`/`device*.dds` do not reach the glass arm.

---

### D4-01: Canonical `FurnitureMarker.heading_z_radians` Option is re-resolved by a per-era gameplay heuristic
- **Severity**: MEDIUM
- **Dimension**: Nodes (Furniture sub-category)
- **Tier Violated**: no-leak
- **Game Affected**: Oblivion, FO3, FNV (legacy `FurniturePositionData::Legacy` — no heading ever populated for these games)
- **Location**: `crates/core/src/ecs/components/furniture.rs:41` (canonical field); consumer `byroredux/src/systems/sandbox.rs:69-71` (`is_sit_marker`) and `:97-104` (`seat_world_transform`)
- **Status**: NEW (no match in `/tmp/audit/issues.json` for `furniture`/`heading`/`sandbox`/`seat`/`orientation`; the `Furniture` type postdates every prior NIFAL/NIF audit report)
- **Description**: Commit `004b51c7` (2026-07-11) built a genuine translate path — `BsFurnitureMarker` (raw parser, unchanged) → `extract_furniture_markers`/`imported_furniture_marker` (`crates/nif/src/import/mod.rs:324-364`, converts offset Z-up→Y-up via the shared `zup_to_yup_pos` primitive, honestly discards the legacy ushort `Orientation` field it cannot yet map to radians) → the single boundary `furniture_component` (`byroredux/src/cell_loader/references/attach.rs:44-63`, the only `Furniture {`/`FurnitureMarker {` construction site outside `crates/core` and its own tests) → canonical `Furniture`/`FurnitureMarker` ECS component. This raw→canonical step itself is clean on all four tier invariants.

  However, the canonical `FurnitureMarker.heading_z_radians: Option<f32>` — a legitimate representation of genuinely-missing legacy data, not itself a bug — is then **re-resolved per-source-era at the gameplay layer** by the one real consumer, the M42 sandbox-seating system, instead of at the translate boundary. `is_sit_marker` (`sandbox.rs:69-71`) uses `m.heading_z_radians.is_none()` as a proxy for "this is legacy content, treat as a sit marker," and `seat_world_transform` (`sandbox.rs:97-104`) branches `Some(h) => Quat::from_rotation_y(h)` vs. a geometric toward-furniture-centre heuristic for `None`. This is exactly the "`Option`/raw discriminator on a canonical type reaching a consumer that must re-resolve it" pattern the no-leak invariant exists to catch — the era-discriminant decision lives in `systems/sandbox.rs`, not in the translate step.
- **Evidence**:
  ```rust
  // sandbox.rs:69
  fn is_sit_marker(m: &FurnitureMarker) -> bool {
      m.animation_type == 1 || m.heading_z_radians.is_none()
  }
  // sandbox.rs:97-104
  let facing = match m.heading_z_radians {
      Some(h) => Quat::from_rotation_y(h),
      None if /* ... */ => Quat::from_rotation_y((-seat_local.x).atan2(-seat_local.z)),
      None => Quat::IDENTITY,
  };
  ```
  Both branches are doc-commented as intentional v0 approximations (`sandbox.rs:35-38,64-68,80-90`) and unit-tested (`is_sit_marker_modern_sit_and_legacy` at `:277-280`; `seat_world_places_root_at_marker_offset` and siblings at `:296-330`+) — a knowing, tested design choice, not an oversight.
- **Impact**: On FNV/FO3/Oblivion (the engine's primary reference/target games), every `BSFurnitureMarker` position is treated as a sit marker regardless of whether the source furniture is a bed (should be Sleep) or a lean-spot (should be Lean), because legacy content carries no `AnimationType` and the consumer conflates "no heading" with "assume sit." Self-acknowledged as a known v0 over-match, and the whole M42 sandbox-seating system is opt-in (`BYRO_SANDBOX_SIT` unset by default per `boot.rs:721`), so it does not affect default engine behavior today. The concern is architectural: as more per-era gameplay logic accretes onto `heading_z_radians.is_some()`/`is_none()` checks, the leak surface grows the way `metalness_override: Option<f32>` did pre-Material-convergence.
- **Suggested Fix**: Not urgent given the opt-in gate and honest self-documentation, but when the seating feature matures past v0: resolve the era discriminant once at the `furniture_component`/`imported_furniture_marker` translate boundary into an explicit canonical field (e.g. `pub kind: FurnitureMarkerKind` with variants `Sit`/`Sleep`/`Lean`/`Unknown`, defaulted the same way `is_sit_marker` does today) rather than leaving `heading_z_radians.is_none()` as an implicit "is legacy" flag for gameplay code to re-derive — mirroring how Materials resolved `metalness_override: Option<f32>` into a plain, pre-resolved `f32`.
- **Related (non-blocking, same commit)**: Furniture markers currently spawn only via the cell-loader path — `grep -n furniture byroredux/src/scene/nif_loader.rs` returns zero hits, so the loose-NIF viewer never attaches a `Furniture` component. Low practical impact today (the loose-NIF path has no actor spawning to consume it), and mirrors the existing `BSBound` loose-path/cell-path asymmetry the spec already documents in the other direction — but unlike that case, this asymmetry isn't called out anywhere yet. Recommend a one-line note in `nifal.md`'s Nodes section alongside the `BSBound` precedent; not filed as a separate finding.

---

### Premises checked and disproved (stale-premise guard)

- *"e1b0294d's (#1925) sweep for order-sensitive substring arms was complete."* — **Disproved.** The commit message claims no other arms were found; MAT-D1-01 is a live counter-example in the same function, one that shares its root cause with an already-closed issue (#1819) in the same file.
- *"004b51c7's furniture-marker work reads the parked `bs_value_node` billboard-mode hint."* — **Disproved** (checked because it's adjacent Nodes-table territory). `BsFurnitureMarker` and `BsValueNode` are distinct block types with distinct extraction paths; the furniture work does not touch `bs_value_node`, which remains parked exactly as documented.
- *"The `004b51c7` furniture translate step itself leaks the legacy ushort `Orientation` field."* — **Disproved.** Traced end-to-end: `imported_furniture_marker`'s `Legacy { .. }` arm discards `orientation`/`position_ref_1`/`position_ref_2` via `..` before they ever reach `ImportedFurnitureMarker`. The leak (D4-01) is one level downstream, in the *consumer's* handling of the resulting `Option`, not in the translate step's handling of the raw field.
- *"The `120c4635` (#1659) `body_part_flags` field is a no-leak violation because it's unresolved."* — **Disproved.** It lives on the raw `ImportedSkin` type, which the tier model explicitly permits to carry unresolved per-game data; a whole-codebase grep confirms zero consumers outside `crates/nif/src/import/` (parser/import/tests), so it never crosses into the canonical tier. Legitimately parked, not a leak.

---

## Documented-limitation ledger (parked-not-leak — do NOT re-report)

Re-verified zero canonical consumers at HEAD `c3e09bb5`, plus one new entry from this sweep:

- **Node passthroughs:** `bs_value_node` (BSValueNode LOD/billboard hint),
  `bs_ordered_node` (alpha-sort/draw-order), `tree_bones` (SpeedTree bone
  names), `range_kind` (destructible/blast/debris discriminator), `lod_group`
  (NiLODNode — content-absent foundation), `bs_lod_cutoffs` (BSLODTriShape
  mesh-level LOD, Skyrim ~43 meshes), `bs_sub_index` (BSSubIndexTriShape
  dismemberment segments).
- **NEW this sweep — Skinning:** `ImportedSkin::body_part_flags`
  (`crates/nif/src/import/types.rs:968`) — `BSDismemberSkinInstance`
  per-partition body-part/flag data (Oblivion/FO3/FNV slot-hiding metadata,
  #1659). Captured at extraction, zero consumers. Blocked on a
  dismemberment/locational-damage/armor-slot-hiding system that does not
  exist yet.
- **Mesh/scene passthroughs:** `ImportedTextureEffect` (NiTextureEffect —
  content-absent, dead extractor), `NiSwitchNode` identity (active-index
  walked, discriminator unsurfaced), `BSInvMarker`, `BSBound` (loose path
  only). `BSFurnitureMarker` is **removed** from this list — it now has a
  real translate path (see Furniture in the tier matrix above and D4-01).
- **Collision:** `BhkNPCollisionObject` (FO4+ Havok-serialised `BhkSystemBinary`
  blob — separate decoder project; cell loader falls back to synthesized
  static trimesh), `BhkPCollisionObject` phantoms (need a `TriggerVolume` ECS
  path), `BhkPlaneShape` (returns `None` — no half-space `CollisionShape`
  variant; trimesh fallback renders the ground surface).
- **Particles:** size-over-life *curve* (grow→steady→fade bell shape; only
  the authored magnitude is translated), per-emitter multi-emitter
  attribution.
- **Animation:** per-light **ambient** colour channels (matched but discarded,
  no per-light ambient slot yet) and **morph-weight** channels (written into
  a real canonical `AnimatedMorphWeights` component every frame, but with no
  renderer/GPU morph-blend consumer yet — a step past "parked on `Imported*`"
  but still not a leak, per the spec's own "captured, no renderer consumer
  yet" framing).
- **Emissive scale:** resolved **no-op** (spec §4) — all three `EmissiveSource`
  variants measured across Oblivion/FNV/Skyrim/FO4 share a ~1.0 scale; a
  future normalization constant would be a `no-fabrication` violation in
  reverse.

---

## Method notes

- 80-commit delta (`d59f40ac..c3e09bb5`) triaged commit-by-commit for NIFAL
  relevance via targeted `git log -- <entry-point files>` per dimension; 7
  translate-surface commits inspected in full (table above), each with an
  independent `git show` read rather than trusting the commit message.
- Ran as 3 parallel sub-audits (dims 1/8/9, dims 2/3/4, dims 5/6/7), each
  instructed to re-derive every checklist claim from the live tree rather
  than carry forward the prior report's assertions. Both findings (MAT-D1-01,
  D4-01) were independently re-verified against the live source by the
  orchestrating pass after the sub-audits reported them (glass-arm substring
  match confirmed by reading `contains_any_ci`'s implementation directly;
  `is_sit_marker`/`seat_world_transform`'s `Option` branching confirmed by
  reading `systems/sandbox.rs` directly).
- Dedup checked against `/tmp/audit/issues.json` (28 OPEN issues) and all
  eight prior `docs/audits/AUDIT_NIFAL_*.md` reports; neither finding
  duplicates an open issue. MAT-D1-01 is related to but distinct from the
  CLOSED `#1819` (same root cause, different manifestation, never
  root-caused).
- Game data present for Oblivion / FNV / Skyrim SE / FO4 / Starfield (per
  `_audit-common.md`); the `#[ignore]`-gated `translation_completeness.rs`
  harness was not driven this sweep (thresholds confirmed unchanged; no
  extractor under it was modified in the delta).
