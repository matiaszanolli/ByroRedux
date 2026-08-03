# SpeedTree Subsystem Audit — 2026-08-03

**Scope**: `crates/spt/` (`byroredux-spt`) — the `.spt` TLV parameter-section
walker + placeholder-billboard import fallback (Session 33 Phase 1, "S1"),
plus its cross-cut wiring in `byroredux/src/cell_loader/references/mod.rs`,
`byroredux/src/cell_loader/references/import.rs`,
`byroredux/src/cell_loader/spawn.rs`, `byroredux/src/scene/nif_loader.rs`,
`crates/plugin/src/esm/records/tree.rs`, and `byroredux/src/systems/billboard.rs`.
Run as one leg of a `comprehensive` audit-suite sweep.

**Depth**: `deep` — corpus acceptance harness run live against on-disk
FNV / FO3 / Oblivion BSAs; full crate unit + integration suite run;
`git log --since=2026-07-25` walked file-by-file across every in-scope
path (9-day window since the last audit), with both touching commits'
diffs read directly (not just commit messages) to confirm nothing
relevant regressed underneath the prior clean bill of health.

**Method**: Diffed direction against `AUDIT_SPEEDTREE_2026-07-25.md`
rather than re-deriving everything from scratch, per the skill's setup
step — that report did full direct reads of `parser.rs`/`stream.rs`/
`import/mod.rs`/`version.rs` with hand-verified cross products and
byte-count checks; this cycle re-confirmed those files are byte-identical
(zero commits touch them in the window) and focused fresh direct-read
effort on the two commits that *did* touch in-scope files, plus a live
re-run of the corpus gate and full test suite.

---

## Dedup pass (mandatory)

`gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json
number,title,state,labels --search "speedtree OR spt OR TREE"` returns
four hits, only one SpeedTree-specific:

| Issue | State | Title |
|---|---|---|
| #1822 | OPEN | SPT-NEW-07: `MaybeStringElseBare` (tag 13005) can misparse a bare 13005 sitting immediately before the geometry tail as a length-prefixed string |
| #2264 | OPEN | TD6-001: ROADMAP wording for unrelated record types (not SpeedTree) |
| #2155 | OPEN | CONC-D4-NEW-03: ABBA detector coverage (unrelated, concurrency) |
| #1576 | OPEN | SF-D4-03: Starfield BFCB component-block gap (unrelated) |

Only #1822 is SpeedTree-specific and it is unchanged since the last
audit (already filed, no regression, no fix landed) — correctly out of
scope for re-reporting.

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
[FO3] 10 files  | 10 with entries | 0 hit unknown tag | 1800  entries  | 100.00 % coverage
[FNV] 10 files  | 10 with entries | 0 hit unknown tag | 1800  entries  | 100.00 % coverage
[OBL] 113 files | 113 with entries| 4 hit unknown tag | 20425 entries  | 96.46 % coverage
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

## Change-window review (`git log --since=2026-07-25`)

Every file in scope was checked individually. Two commits touch in-scope
files; every other file (`parser.rs`, `tag.rs`, `stream.rs`, `version.rs`,
`scene.rs`, `crates/plugin/src/esm/records/tree.rs`,
`byroredux/src/systems/billboard.rs`, `crates/spt/src/lib.rs`) has **zero**
commits in the window — byte-identical to the 2026-07-25 audit's verified
state.

| File | Commit | What changed | SpeedTree-relevant? |
|---|---|---|---|
| `crates/spt/src/import/mod.rs` | `4fd214aa` (Fix #2206: propagate `NiBillboardNode` mode on the cell-loader NIF path) | Added `billboard_mode: None` to the `placeholder_billboard_mesh` struct literal — a new `ImportedMesh` field (the flat-walk's per-mesh billboard-mode sibling to `ImportedNode::billboard_mode`) forced by an unrelated fix for the *general* NIF `walk_node_flat` path. The commit message and the added code comment both state the `.spt` path is untouched and uses a different mechanism (`placement_root_billboard` on the node, not the mesh) | Mechanical only, verified correct: `spawn_mesh_instance`'s new `if let Some(raw) = mesh.billboard_mode { … }` (spawn.rs:1270) is unreachable for SpeedTree meshes since the field is hardcoded `None`; the pre-existing `spawn_placement_root`'s `if let Some(mode) = cached.placement_root_billboard` (spawn.rs:453, the #994 guard) is untouched by this commit — confirmed no double-attach or precedence conflict between the two Billboard-insertion sites |
| `crates/spt/src/import/mod.rs` (tests only) | `05d68926` (Refactor material handling in NIF import pipeline) | Mechanical test-only field-access-path updates (`mesh.alpha_test` → `mesh.material.alpha_test`, etc.) following the engine-wide `ImportedMesh.material: ImportedMaterial` encapsulation (`c8c8a834`-era NIFAL split noted in project memory) | Re-verified the *production* code (not just tests): `placeholder_billboard_mesh` now constructs `material: ImportedMaterial { textures, alpha_test: true, alpha_threshold: 0.5, alpha_test_func: 6, two_sided: true, metalness_override: Some(0.0), roughness_override: Some(0.85), ..Default::default() }` — every field the S1 acceptance contract requires is still set explicitly; `ImportedMaterial::default()` fills the rest with `is_pbr: false`, `from_bgsm: false`, `bgem_glass: false`, `thin_glass: false`, `emissive_source: EmissiveSource::None` (`#[default]` on the enum) — no drift |
| `byroredux/src/cell_loader/spawn.rs` | `4fd214aa`, `01f198e7`, `05d68926`, `bca0f127`, `3b922734`, `8a15b064`, `733dff8f`, `a8b0cf64`, `1d94eb24`, `24e5cb6a` | Billboard-mode-on-mesh (above), shadow-projection canonicalization, material refactor, fog volumes, particle-emitter/fog cleanup, light-anim, PBR plumbing unification, fire-refraction material kind — none touch `spawn_placement_root`'s `cached.placement_root_billboard` block (spawn.rs:453) or the `translate_material` call site's `&mesh.material` argument beyond the mechanical rename | Confirmed by direct read: both Billboard-insertion sites and the `translate_material(&mesh.material, …)` call (spawn.rs:1303) are intact and correct post-refactor |
| `byroredux/src/scene/nif_loader.rs` | `bca0f127`, `8a15b064`, `733dff8f`, `a8b0cf64`, `05d68926`, `1d94eb24`, `24e5cb6a` | Same cross-cutting renderer/material work | `is_spt` branch (lines 176-215) unchanged: still calls `byroredux_spt::parse_spt` + `import_spt_scene(&scene, &SptImportParams::default(), …)`, returns `Some(imported)` which flows through the *same* `load_nif_bytes_with_skeleton` → `translate_material` call (line 879) as every other loose-NIF mesh — single-boundary contract intact |
| `byroredux/src/cell_loader/references/mod.rs`, `references/import.rs` | `01f198e7`, `cd6a8338`, `6df3bad8`, `733dff8f`, `9bf4c493`, `9926fa50`, `0dcb71b7` (mod.rs); `05d68926` (import.rs) | Shadow-projection canonicalization, renderer audit fixes, SCEN scripting, fog volumes, resumable NPC assembly / cell application, seat-clear fix (mod.rs); material refactor mechanical rename (import.rs) | None touch the `is_spt` dispatch block or `parse_and_import_spt`'s control flow (`None`-on-`Err`, `record_index.trees` lookup) — confirmed by direct read |
| `byroredux/src/material_translate.rs`, `crates/core/src/ecs/components/material.rs` | `05d68926`, `0a3e0da5`, `bca0f127` | Material refactor (the `ImportedMaterial` encapsulation itself), runtime-telemetry-audit doc commit, texture/fog handling | `translate_material`'s `metalness: source.metalness_override.unwrap_or(f32::NAN)` / `roughness: source.roughness_override.unwrap_or(f32::NAN)` lines (174-175) unchanged in effect — `source` is now `&ImportedMaterial` (was `&ImportedMesh`-flat) but the field names and resolve-PBR-then-clamp contract are identical; SpeedTree's `Some(...)` overrides still bypass the NaN-triggered keyword-classifier arm on both routes |

**Current state, confirmed by direct read (not carried forward)**:

- `byroredux/src/cell_loader/spawn.rs:453` — `Billboard::new(mode)` still
  inserted on the placement root exactly when
  `cached.placement_root_billboard.is_some()` — the #994 guard, byte-level
  unchanged by the #2206 fix.
- `crates/spt/src/import/mod.rs`'s `placeholder_billboard_mesh` still sets
  `billboard_mode: None` on the `ImportedMesh` — the two billboard
  mechanisms (mesh-level for general NIF, node-level for SpeedTree) cannot
  double-fire for a `.spt` scene.
- `byroredux/src/scene/nif_loader.rs:176-215` — the `--tree` loose route
  still calls `import_spt_scene(&scene, &SptImportParams::default(), …)`,
  i.e. still no TREE metadata (documented route divergence, not a bug),
  and still funnels through the shared `translate_material` boundary via
  `load_nif_bytes_with_skeleton`.

No functional regression found in the change window.

---

## Dimension summary (this cycle)

| Dimension | Verdict | Basis this cycle |
|---|---|---|
| 1 — Walker Byte-Accounting | Clean | Zero commits to `parser.rs`/`stream.rs` since 07-25; live corpus gate byte-identical (same offsets, same coverage %) to six prior runs — the walker's behavior on all 133 vanilla files is unchanged |
| 2 — Placeholder Fallback | Clean | `placeholder_billboard_mesh`/`compute_billboard_size` production logic re-read directly this cycle post-material-refactor: precedence chain (OBND→BNAM→MODB→default), clamps, Z-up→Y-up, `-Z` winding, alpha-test cutout fields all present and correctly typed under the new `ImportedMesh.material: ImportedMaterial` shape |
| 3 — TREE → Billboard Wiring | Clean | `spawn_placement_root`'s `Billboard` insertion (#994 guard) and `parse_and_import_spt`'s graceful `None`-on-`Err` re-verified unchanged; new mesh-level billboard field from #2206 confirmed inert (`None`) for SpeedTree, no double-attach |
| 4 — Per-Game Variants & Route Divergence | Clean | `version.rs` untouched; both routes (`nif_loader.rs` loose + `cell_loader` REFR) still call `parse_spt` + `import_spt_scene`, still funnel through the same `translate_material` boundary |
| 5 — Tag Dictionary | Clean | `tag.rs` untouched since 2026-05-09; live histogram byte-identical to seven prior audit runs |
| 6 — NIFAL Material Translation | Clean | Re-verified against the post-refactor `ImportedMaterial` struct: `is_pbr: false`, `from_bgsm: false`, `bgem_glass: false`, `thin_glass: false`, `emissive_source: EmissiveSource::None` (via `#[default]`) all still hold via `..Default::default()`; explicit `metalness_override`/`roughness_override` `Some(...)` still bypass the keyword classifier on both routes |

---

## Findings

None. Zero new findings this cycle. The only tracked open item (#1822 /
SPT-NEW-07) is unchanged and correctly out of scope for re-reporting.

The subsystem absorbed two cross-cutting refactors this window (#2206's
`NiBillboardNode` mesh-level propagation, and the engine-wide
`ImportedMesh.material: ImportedMaterial` encapsulation) without any
adaptation bug — both commits' authors correctly treated the `.spt`
placeholder path as a distinct mechanism from the general NIF path and
left it functionally untouched, with only the required mechanical
field/path updates.

---

## Regression Guards (verified in place, NOT re-reported)

| Finding | Issue | Guard verified this cycle |
|---|---|---|
| SPT-D4-01 (cell placeholder loses `Billboard`) | #994 | `spawn.rs:453` inserts `Billboard` on the placement root when `placement_root_billboard.is_some()` — read directly, unaffected by #2206 |
| SPT-D4-02 (`bs_bound` Z-up→Y-up) | #995 | `import/mod.rs` routes center via `zup_to_yup_pos`, half-extents `(hx,hz,hy)` — unchanged |
| SPT-D5-01 (`wind` docstring) | #996 | `SptImportParams.wind` doc still says CNAM, not BNAM |
| SPT-D2-01 ("first wins" leaf tex) | #997 | `import/mod.rs` `.first()` — unchanged |
| SPT-D3-01 (pinned regression sample) | #998 | `tests/parse_synthetic_spt.rs` byte-pinned fixture passes |
| SPT-D1-01 (13005 bimodal) | #999 | `MaybeStringElseBare` — unchanged (residual tail edge tracked as #1822) |
| SPT-D4-03 (normal/winding) | #1000 | `-Z` normals + `[0,3,2,2,1,0]` winding — unchanged |
| SPT-D4-04 (default size / MODB) | #1001 | `compute_billboard_size` OBND→BNAM→MODB→default — unchanged |
| SPT-D5-02 (BNAM precedence) | #1002 | OBND-beats-BNAM — unchanged |
| BSXFlags dropped at spawn | #1214 | `bsx_flags = 0` synthetic default |
| SceneFlags / root_flags | #1235 | `root_flags = 0` synthetic default |
| SPT-NEW-02/03/04 doc/route | #1707/#1711/#1715 | All CLOSED; `BsRotateAboutUp` fallback doc matches code |
| SPT-NEW-05 (foliage keyword collision) | #1819 | `metalness_override: Some(0.0)` / `roughness_override: Some(0.85)` survive the `ImportedMaterial` refactor |
| SPT-NEW-01 (`detect_variant` dead code) | #1820 | Two production (logging-only) callers, unchanged |
| SPT-NEW-06 (format-notes.md byte-align) | #1821 | Doc still byte-accurate |
| NEW this cycle — mesh-level billboard field doesn't double-fire | #2206 (general-NIF fix, SpeedTree side-effect) | `placeholder_billboard_mesh`'s `billboard_mode: None` confirmed to make `spawn_mesh_instance`'s new per-mesh Billboard-attach unreachable for `.spt` placeholders |

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
health across **seven** consecutive audit cycles (2026-06-23, 07-01,
07-02, 07-03, 07-16, 07-25, 08-03). Two cross-cutting engine refactors
landed in this window (#2206's per-mesh billboard propagation on the
general NIF path, and the `ImportedMesh.material: ImportedMaterial`
encapsulation) — both were traced commit-by-commit and confirmed to
leave the `.spt` placeholder contract (graceful fallback, single
`translate_material` boundary, `Billboard` attach on the placement root)
intact.

### Suggested next step

No new issues to file. `/audit-publish` is not needed this cycle — this
report only reconfirms clean status. #1822 (SPT-NEW-07) remains open and
tracked; no action needed on it from this audit.
