# SpeedTree Subsystem Audit — 2026-08-07

**Scope**: `crates/spt/` (`byroredux-spt`) — the `.spt` TLV parameter-section
walker + placeholder-billboard import fallback (Session 33 Phase 1, "S1"),
plus its cross-cut wiring in `byroredux/src/cell_loader/references/mod.rs`,
`byroredux/src/cell_loader/references/import.rs`,
`byroredux/src/cell_loader/spawn.rs`, `byroredux/src/scene/nif_loader.rs`,
`crates/plugin/src/esm/records/tree.rs`, and `byroredux/src/systems/billboard.rs`.
Single-pass, all dimensions run inline per the skill's own architecture note
("small enough to run all dimensions inline rather than spawning Tasks") —
no sub-agents were dispatched for this cycle.

**Depth**: `deep` — corpus acceptance harness run live against on-disk
FNV / FO3 / Oblivion BSAs; full crate unit + integration suite run;
`git log --since=2026-08-03` walked file-by-file across every in-scope
path (4-day window since the last audit), with every touching commit's
diff read directly (not just commit messages) to confirm nothing relevant
regressed underneath the prior clean bill of health.

**Method**: Diffed direction against `AUDIT_SPEEDTREE_2026-08-03.md` rather
than re-deriving everything from scratch, per the skill's setup step — that
report reconfirmed all six dimensions clean across seven prior cycles; this
cycle re-confirmed the walker/tag/import/scene files are byte-identical
(zero commits touch them in the window) and focused fresh direct-read
effort on the commits that did touch in-scope wiring files, plus a live
re-run of the corpus gate and full test suite.

---

## Dedup pass (mandatory)

`gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json
number,title,state,labels --search "speedtree OR spt OR TREE"` returns
hits; the only SpeedTree-specific one is:

| Issue | State | Title |
|---|---|---|
| #1822 | OPEN | SPT-NEW-07: `MaybeStringElseBare` (tag 13005) can misparse a bare 13005 sitting immediately before the geometry tail as a length-prefixed string |

Unchanged since the last audit (`updatedAt: 2026-07-02`) — already filed,
no regression, no fix landed, correctly out of scope for re-reporting.

---

## Verification runs (this audit)

### Corpus acceptance gate — live run

```
BYROREDUX_FNV_DATA="/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data" \
BYROREDUX_FO3_DATA="/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data" \
BYROREDUX_OBL_DATA="/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data" \
  cargo test -p byroredux-spt --release --test parse_real_spt -- --ignored --nocapture
```

```
[FO3] 10 files  | 10 with entries | 0 hit unknown tag | 1800  entries | 100.00 % coverage
[FNV] 10 files  | 10 with entries | 0 hit unknown tag | 1800  entries | 100.00 % coverage
[OBL] 113 files | 113 with entries| 4 hit unknown tag | 20425 entries | 96.46 % coverage
  unknown-tag samples:
    trees\shrubms14boxwood.spt         | tag=768 (0x0300) at offset 4507
    trees\treecottonwoodsu.spt         | tag=768 (0x0300) at offset 5641
    trees\treems14willowoakyoungsu.spt | tag=768 (0x0300) at offset 5946
    trees\treems14canvasfreesu.spt     | tag=768 (0x0300) at offset 6211
```

Byte-for-byte identical to every prior audit run since 2026-07-03 (same
files, same offsets, same coverage rates). All three gates clear the
≥ 95 % floor.

### Unit + integration suite

`cargo test -p byroredux-spt --release` — 46 unit tests + 3 synthetic
integration tests (`parse_synthetic_spt.rs`), all pass, 0 failures.

---

## Change-window review (`git log --since=2026-08-03`)

Every file in scope was checked individually. Zero commits touch the
walker/tag/import/scene core; six commits touch cross-cut wiring files
for reasons unrelated to SpeedTree (physics/collision authoring, quest
lifecycle, streaming perf, material-scalar landing).

| File | Commits in window | SpeedTree-relevant? |
|---|---|---|
| `crates/spt/src/*` (`parser.rs`, `tag.rs`, `stream.rs`, `version.rs`, `scene.rs`, `import/mod.rs`, `lib.rs`) | **0** | Byte-identical to 2026-08-03 audit's verified state |
| `crates/plugin/src/esm/records/tree.rs` | **0** | Byte-identical |
| `byroredux/src/systems/billboard.rs` | **0** | Byte-identical |
| `crates/spt/docs/format-notes.md`, `crates/spt/tests/*` | **0** | Byte-identical |
| `byroredux/src/cell_loader/references/mod.rs` | `716b7ee9`, `8ee151e0`, `a844c26b`, `30d421cd`, `16039d97` | Read directly: none touch the `is_spt` extension check or the `record_index.trees.get(&child_form_id)` lookup (lines ~1403–1431). `a844c26b` (quest lifecycle) threads a new `record_index` parameter through several *other* spawn-helper signatures for alias resolution but does not alter the SPT dispatch block itself — confirmed by `grep -i spt` on the diff returning zero hits. |
| `byroredux/src/cell_loader/references/import.rs` | `8ee151e0` | **Mechanical, verified inert.** Adds `collision_authoring: byroredux_nif::import::collision::summarize_collision_authoring(&scene)` to `parse_and_import_nif`'s `CachedNifImport` construction, and the matching new struct field to `parse_and_import_spt`'s construction as `collision_authoring: Default::default()` (with a comment: "collision (tree-trunk collider) once the geometry tail is decoded — follow-up sub-phase"). Correct placeholder value — see Dimension 3 below for the downstream trace confirming this cannot fabricate a collider or corrupt telemetry for `.spt` placements. |
| `byroredux/src/cell_loader/spawn.rs` | `716b7ee9`, `8ee151e0`, `342ef84e`, `30d421cd`, `655ff18c`, `9e554089` | None touch `spawn_placement_root`'s `Billboard` insertion (line 661-662, the #994 guard) or the `translate_material(&mesh.material, …)` call site. `716b7ee9`/`8ee151e0` refactor `collisions_empty: bool` → `MissingCollisionFallback` enum for the missing-collision fallback selector (`missing_collision_fallback()`), used by packed-Havok proxy synthesis; traced downstream (below) and confirmed inert for SpeedTree billboards. |
| `byroredux/src/scene/nif_loader.rs` | `342ef84e`, `32ebfdec` | `is_spt` branch (lines 176-215) unchanged — `grep -i spt` on both diffs returns zero hits. Still calls `byroredux_spt::parse_spt` + `import_spt_scene(&scene, &SptImportParams::default(), …)`, still funnels through the same `translate_material` boundary. |
| `byroredux/src/material_translate.rs` | `4d350c4b`, `7dacef90`, `95e77897` | The `metalness: source.metalness_override.unwrap_or(f32::NAN)` / `roughness: …unwrap_or(f32::NAN)` lines (208-209) are untouched. `7dacef90` adds a new *test* harness fixture (unrelated glass/decal classification coverage) — no production change to the override passthrough SpeedTree relies on. |
| `crates/core/src/ecs/components/material.rs` | `4279c195`, `95e77897` | `resolve_pbr()` (line 813+) is byte-identical — both commits only add new struct fields (BSLightingShaderProperty shading scalars, `material.ior` doc) elsewhere in the file. Confirmed via `git show -p` grep on `resolve_pbr` returning zero diff hits. |

### Downstream trace: `collision_authoring: Default::default()` for `.spt` placements

The one substantively new piece of code touching an in-scope file this
window is the collision-authoring summary threaded into `CachedNifImport`.
Traced end-to-end to confirm it cannot synthesize a spurious physics proxy
or corrupt telemetry for SpeedTree billboards:

- `missing_collision_fallback(collisions_empty, authoring, base_layer)`
  (`spawn.rs:56-75`) returns `ArchitectureTriMesh` whenever
  `base_layer == RenderLayer::Architecture` **regardless of `authoring`**
  — that branch predates this window (same as the old
  `collisions_empty && base_layer == Architecture` form) and only inspects
  `authoring` for the separate `PackedAabbProxy` branch, gated to
  `RenderLayer::Clutter | RenderLayer::Actor`.
- `RecordType::TREE` classifies to `RenderLayer::Architecture`
  (`crates/plugin/src/record.rs:291`), so SpeedTree REFRs only ever reach
  the `ArchitectureTriMesh` arm — the `authoring.needs_packed_havok_fallback()`
  check (which would read the now-populated field) is never evaluated for
  `.spt` placements in the first place.
- Even so, the `ArchitectureTriMesh` fallback itself requires
  `!mesh.material.alpha_test` at its consumption site (`spawn.rs:1680-1687`).
  `placeholder_billboard_mesh` sets `alpha_test: true` unconditionally
  (the two-sided cutout billboard contract, #1000/#1001 lineage) — so the
  gate excludes every SpeedTree placeholder mesh from trimesh-collider
  synthesis, unchanged behavior.
- `CollisionAuthoringSummary::needs_packed_havok_fallback()` is
  `self.new_physics > 0`; `Default::default()` zeroes all three counters,
  so it evaluates `false` even on the code paths that do read it.
- The `unresolved_packed_collision` / `packed_collision_fallbacks` telemetry
  counters (`spawn.rs:609,616,619`) are both derived from
  `needs_packed_havok_fallback()` / `synthesized_collision_proxy`, neither
  of which can go true for a `.spt` placement — no bogus per-cell stats.

No behavioral change, no new risk.

---

## Dimension summary (this cycle)

| Dimension | Verdict | Basis this cycle |
|---|---|---|
| 1 — Walker Byte-Accounting | Clean | Zero commits to `parser.rs`/`stream.rs`/`tag.rs` since 08-03; live corpus gate byte-identical (same offsets, same coverage %) to eight prior runs |
| 2 — Placeholder Fallback | Clean | Zero commits to `import/mod.rs`; the alpha-test cutout gate that keeps `ArchitectureTriMesh` fallback synthesis from firing on billboards (Dimension 3 trace above) confirms the two-sided/alpha-test material contract is still load-bearing and intact |
| 3 — TREE → Billboard Wiring | Clean | `spawn_placement_root`'s `Billboard` insertion (#994 guard) and `parse_and_import_spt`'s graceful `None`-on-`Err` unchanged; new `collision_authoring` field threaded through `parse_and_import_spt` as `Default::default()`, traced downstream and confirmed inert (see above) |
| 4 — Per-Game Variants & Route Divergence | Clean | `version.rs` untouched; both routes (`nif_loader.rs` loose + `cell_loader` REFR) still call `parse_spt` + `import_spt_scene`, still funnel through the same `translate_material` boundary |
| 5 — Tag Dictionary | Clean | `tag.rs` untouched since 2026-05-09; live histogram byte-identical to eight prior audit runs |
| 6 — NIFAL Material Translation | Clean | `translate_material`'s override passthrough (lines 208-209) and `Material::resolve_pbr` (material.rs:813+) both byte-identical this window; `ImportedMaterial` defaults (`is_pbr: false`, `emissive_source: EmissiveSource::None`, etc.) unaffected by the two commits that touched `material_translate.rs`/`material.rs` (both additive, elsewhere in the files) |

---

## Findings

None. Zero new findings this cycle. The only tracked open item (#1822 /
SPT-NEW-07) is unchanged and correctly out of scope for re-reporting.

The subsystem absorbed one cross-cutting refactor this window (the
`CollisionAuthoringSummary` field landing on `CachedNifImport` for the
FO4+ packed-Havok compatibility-proxy feature) without any adaptation bug
— the author correctly set the SpeedTree placeholder's new field to an
inert `Default::default()`, and the downstream consumption path was
independently re-derived and confirmed to never reach a code branch that
would treat it as "authored collision data" for a TREE-classified REFR.

---

## Regression Guards (verified in place, NOT re-reported)

| Finding | Issue | Guard verified this cycle |
|---|---|---|
| SPT-D4-01 (cell placeholder loses `Billboard`) | #994 | `spawn.rs:661-662` inserts `Billboard` on the placement root when `placement_root_billboard.is_some()` — read directly, unaffected by this window's commits |
| SPT-D4-02 (`bs_bound` Z-up→Y-up) | #995 | `import/mod.rs` untouched this window |
| SPT-D5-01 (`wind` docstring) | #996 | `SptImportParams.wind` doc untouched |
| SPT-D2-01 ("first wins" leaf tex) | #997 | `import/mod.rs` untouched |
| SPT-D3-01 (pinned regression sample) | #998 | `tests/parse_synthetic_spt.rs` byte-pinned fixture passes |
| SPT-D1-01 (13005 bimodal) | #999 | `MaybeStringElseBare` untouched (residual tail edge tracked as #1822) |
| SPT-D4-03 (normal/winding) | #1000 | `-Z` normals + `[0,3,2,2,1,0]` winding untouched — and the `alpha_test: true` contract this relies on is the same gate that now also keeps the packed-collision refactor inert for billboards |
| SPT-D4-04 (default size / MODB) | #1001 | `compute_billboard_size` untouched |
| SPT-D5-02 (BNAM precedence) | #1002 | OBND-beats-BNAM untouched |
| BSXFlags dropped at spawn | #1214 | `bsx_flags = 0` synthetic default, `import.rs` construction site untouched by `8ee151e0` beyond the new field |
| SceneFlags / root_flags | #1235 | `root_flags = 0` synthetic default, untouched |
| SPT-NEW-02/03/04 doc/route | #1707/#1711/#1715 | All CLOSED; unchanged |
| SPT-NEW-05 (foliage keyword collision) | #1819 | `metalness_override: Some(0.0)` / `roughness_override: Some(0.85)` untouched; `translate_material`'s NaN-triggered classifier bypass re-verified this cycle (lines 208-209 byte-identical) |
| SPT-NEW-01 (`detect_variant` dead code) | #1820 | Two production (logging-only) callers, unchanged |
| SPT-NEW-06 (format-notes.md byte-align) | #1821 | Doc untouched this window |
| Mesh-level billboard field doesn't double-fire | #2206 (prior cycle) | `placeholder_billboard_mesh`'s `billboard_mode: None` unchanged; no commits this window touch `spawn_mesh_instance`'s per-mesh Billboard-attach or the `.spt` mesh-literal construction |
| NEW this cycle — `collision_authoring` field addition doesn't fabricate a collider for TREE placements | (no issue — verified clean, not a finding) | `parse_and_import_spt`'s `collision_authoring: Default::default()`, traced through `missing_collision_fallback` → `RenderLayer::Architecture` (TREE's classification) → `ArchitectureTriMesh` arm → `!mesh.material.alpha_test` gate (excludes SpeedTree's always-`alpha_test: true` billboard) |

The **14000-band Oblivion tail** (4 files bail at tag `768`) remains the
documented `format-notes.md` Phase-1 limitation, above the 95 % gate,
placeholder-covered — not re-reported.

---

## Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 0 |
| **Total** | **0** |

No new findings this audit. The subsystem has now shown a clean bill of
health across **eight** consecutive audit cycles (2026-06-23, 07-01,
07-02, 07-03, 07-16, 07-25, 08-03, 08-07). One cross-cutting engine
refactor landed in this window (the FO4+ packed-Havok collision-authoring
summary reaching `CachedNifImport`) and was traced commit-by-commit and
downstream-consumption-by-consumption to confirm it leaves the `.spt`
placeholder contract (graceful fallback, single `translate_material`
boundary, `Billboard` attach on the placement root, no fabricated
collision proxy) intact.

### Suggested next step

No new issues to file. `/audit-publish` is not needed this cycle — this
report only reconfirms clean status. #1822 (SPT-NEW-07) remains open and
tracked; no action needed on it from this audit.
