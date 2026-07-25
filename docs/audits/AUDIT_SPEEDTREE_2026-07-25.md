# SpeedTree Subsystem Audit — 2026-07-25

**Scope**: `crates/spt/` (`byroredux-spt`) — the `.spt` TLV parameter-section
walker + placeholder-billboard import fallback (Session 33 Phase 1, "S1"),
plus its cross-cut wiring in `byroredux/src/cell_loader/references/mod.rs`,
`byroredux/src/cell_loader/references/import.rs`,
`byroredux/src/cell_loader/spawn.rs`, `byroredux/src/scene/nif_loader.rs`,
`crates/plugin/src/esm/records/tree.rs`, and `byroredux/src/systems/billboard.rs`.
Run as one leg of a `comprehensive` audit-suite sweep.

**Depth**: `deep` — corpus acceptance harness run live against on-disk
FNV / FO3 / Oblivion BSAs; full crate unit + integration suite run;
every carried-forward finding re-verified against current source and
`gh issue list`; `git log --since=2026-07-16` walked file-by-file across
every in-scope path (9-day window since the last audit) with each touching
commit's diff read directly (not just the commit message) to confirm
nothing relevant regressed underneath the prior clean bill of health.

**Method**: Read the full crate source directly this cycle rather than
deferring to the prior report's summary — `parser.rs`, `stream.rs`,
`import/mod.rs`, `version.rs` in full; `tag.rs`/`scene.rs` and the
cross-cut wiring (`references/import.rs::parse_and_import_spt`,
`spawn.rs`'s `Billboard` insertion, `nif_loader.rs`'s `is_spt` branch,
`tree.rs`'s `parse_tree`/`has_speedtree_binary`, `billboard.rs`'s
`compute_billboard_rotation`) by targeted grep + read. Ran the unit
suite and the `--ignored` corpus gate live. Cross-checked every prior
finding's issue number via `gh issue view`.

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

Byte-for-byte identical to the 2026-07-03/07-16 runs (same files, same
offsets, same coverage rates). All three gates clear the ≥ 95 % floor.

### Unit + integration suite

`cargo test -p byroredux-spt --release` — 46 unit tests + 3 synthetic
integration tests (`parse_synthetic_spt.rs`), all pass, 0 failures.

---

## Dedup pass (mandatory)

`gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json
number,title,state,labels --search "speedtree OR spt OR TREE"` returns
three hits, only one SpeedTree-specific:

| Issue | State | Title |
|---|---|---|
| #1822 | OPEN | SPT-NEW-07: `MaybeStringElseBare` (tag 13005) can misparse a bare 13005 sitting immediately before the geometry tail as a length-prefixed string |
| #2075 | OPEN | TD8-101: `log` declared as a dependency in 7 crates that never call it (unrelated — workspace-wide tech-debt, not SpeedTree-specific) |
| #1576 | OPEN | SF-D4-03: Starfield BFCB component-block gap (unrelated) |

`gh issue view 1822` confirms it is still `OPEN`, `updatedAt: 2026-07-02` —
unchanged since the last audit, correctly out of scope for re-reporting
per the dedup protocol (already filed, no regression, no fix landed).

No new GitHub issues reference `spt`/SpeedTree/TREE content since the
2026-07-16 audit.

---

## Change-window review (`git log --since=2026-07-16`)

Every file in scope was checked individually with `git log --since=2026-07-16
-- <file>` (not just a scope-wide `git log`, which is dominated by unrelated
FSR-upscaler and renderer work landed this window). Actual touches, each
verified by reading the diff directly:

| File | Commit(s) | What changed | SpeedTree-relevant? |
|---|---|---|---|
| `crates/spt/src/import/mod.rs` | `883f57cd` (stable surface ID for temporal shadowing) | Added `thin_glass: false` to the `placeholder_billboard_mesh` struct literal — a new `ImportedMesh` field forced by the renderer's glass/occlusion split, mechanically set to the same inert default pattern as every prior struct-literal keep-in-sync edit (`bgsm_greyscale_lut_is_alpha`, `furniture_markers`, etc., from earlier windows) | Mechanical only — a SpeedTree billboard is never glass, `false` is correct |
| `byroredux/src/cell_loader/references/mod.rs` | `6d0176b5` (split `load_references`), `4becd997` (scripting-pipeline fixes), `41eedfe1`/`7b587a86` (light anim / LIGH bit fixes) | `6d0176b5` relocated the `is_spt` dispatch block verbatim (diff shows identical removed/re-added lines, just moved) as part of extracting `spawn_synth_child`; `4becd997` added VMAD-to-first-synthetic-child gating (`refr_script_instance_for_synth_child`) — applies to the shared REFR loop but SCOL/PKIN VMAD fan-out is orthogonal to the `.spt` model-path dispatch; light-anim commits don't touch this function at all | No logic change to the `.spt` route |
| `byroredux/src/cell_loader/references/import.rs` | `f9ad6ca2` (Fix #2111) | Removed a redundant NIF-header re-parse in `parse_and_import_nif` (reads `scene.bsver` instead) — `parse_and_import_spt` (a separate function in the same file) is untouched by this diff | Not applicable — NIF-only fix |
| `byroredux/src/cell_loader/spawn.rs` | `61b0cea7`, `bad8619a`, `8961fbdd`, `7b587a86`, `41eedfe1`, `388b9969`, `cd2b5fe4`, `01796841` | All touch collision-consolidation, exterior terrain collision, light/decal/vertex-color plumbing — none touch the `Billboard`/`placement_root_billboard` block; import list reordering only in the `Billboard` line's vicinity | No logic change to Billboard insertion (confirmed by direct read, line 412-418 below) |
| `byroredux/src/scene/nif_loader.rs` | `388b9969`, `cd2b5fe4`, `4be4992f` | Decal component, vertex-color-format, Skyrim+ FaceGen fixes — none touch the `is_spt` branch (confirmed: `is_spt`/`import_spt_scene`/`SptImportParams::default()` call sites unchanged, lines 163-207) | Not applicable |
| `byroredux/src/material_translate.rs` | `41eedfe1`, `6c56e311`, `19703131` | Light-anim refactor, volumetric/water shader refactor, clippy-only fix — `metalness_override`/`roughness_override`/`resolve_pbr` plumbing (lines 153-164) is byte-identical to the last audit | Not applicable |
| `crates/spt/src/{parser,tag,stream,version,scene}.rs`, `crates/plugin/src/esm/records/tree.rs`, `byroredux/src/systems/billboard.rs` | none | Zero commits touch these files in the window | N/A — untouched |

**Current state, confirmed by direct read (not by trusting the diff
summary alone)**:

- `byroredux/src/cell_loader/spawn.rs:412-418` — `Billboard::new(mode)` is
  still inserted exactly when `cached.placement_root_billboard.is_some()`.
- `byroredux/src/cell_loader/references/import.rs:288-326` —
  `parse_and_import_spt` still logs `detect_variant` (non-dispatch,
  #1820 guard) and returns `None` on `parse_spt` `Err` (magic mismatch /
  underflow) without aborting the rest of the cell.
- `byroredux/src/scene/nif_loader.rs:163-207` — the `--tree` loose route
  still calls `import_spt_scene(&scene, &SptImportParams::default(), …)`,
  i.e. still no TREE metadata (documented route divergence, not a bug).

No functional regression found in the change window.

---

## Fresh dimension-by-dimension verification (read directly this cycle)

### Dimension 1 — Walker Byte-Accounting: **CLEAN**

Read `crates/spt/src/parser.rs` and `crates/spt/src/stream.rs` in full.
Confirmed directly (not by inference from tests):

- `parse_spt` returns `Err` only for magic mismatch (`starts_with(MAGIC_HEAD)`
  check) or `SptStream` underflow (`read_bytes` when `remaining() < n`).
  Every other path — out-of-range tag, in-range-but-`Unknown` tag — is
  recorded (`tail_offset`, `unknown_tags`) and returns `Ok(scene)`.
- The main loop peeks the tag (`peek_u32_le`, non-advancing), checks
  `(TAG_MIN..=TAG_MAX).contains(&tag)` before consuming it, and only
  advances the cursor (`read_u32_le`) once the tag is confirmed a known,
  dispatchable kind — so a rejected/unknown tag's 4 bytes are never
  double-counted.
- `read_payload` maps every `SptTagKind` variant to an exact byte count:
  `U8`→1, `U32`/`Vec3` via `read_u32_le`/`read_vec3_le` (4/12 bytes),
  `FixedBytes(n)`→n, `String`→4 + len (via `read_string_lp`),
  `ArrayBytes{stride}`→4 + count×stride. `Unknown`/`MaybeStringElseBare`
  never reach `read_payload` (handled inline in the walker) — the
  `SptTagKind::Unknown | MaybeStringElseBare` arm in `read_payload` is a
  defensive `Err` bail that should be unreachable in practice.
- `MaybeStringElseBare` (tag 13005): consumes the tag (`read_u32_le`),
  then peeks the next u32. If it resolves to a known in-range tag →
  `Bare` (no further bytes consumed, walker re-syncs on the peeked tag
  next iteration). Otherwise treats the peeked u32 as a string length and
  calls `read_string_lp()` (which itself re-reads the length via its own
  `read_u32_le`). Both arms tested directly:
  `tag_13005_followed_by_known_tag_resolves_as_bare`,
  `tag_13005_followed_by_string_length_resolves_as_string`,
  `tag_13005_at_eof_does_not_panic` (peek returns `None` on truncated
  input → falls to the `String` arm → `read_string_lp` fails cleanly
  with `UnexpectedEof`, not a panic). All three pass.
- 64 KiB caps: `read_string_lp` caps on `len` (the byte count itself,
  post length-read) before calling `read_bytes`; `ArrayBytes` computes
  `total_bytes = count as u64 * stride as u64` (saturating multiply, so
  a `count` near `u32::MAX` can't wrap) and bails with `Err` before
  calling `read_bytes`, not after allocating. Confirmed the cap is on
  the byte total, not just the raw `count` field, per the checklist.
- LE-only, unconditional: no version branches anywhere in `stream.rs`;
  every multi-byte read is an explicit `from_le_bytes`. No host-endian
  or BE assumption found.
- `TAG_MIN..=TAG_MAX` = `100..=13_999` — unchanged from prior audits.

One residual, already tracked: `tag.rs`'s comment on `MaybeStringElseBare`
(and the parser's own doc comment) explicitly flags that a pathological
mod file where a genuine string length happens to coincide with a
dictionary tag value would misparse as `Bare` — this is **#1822 /
SPT-NEW-07**, unchanged, correctly not re-reported.

### Dimension 2 — Placeholder Fallback Correctness: **CLEAN**

Read `crates/spt/src/import/mod.rs` in full (all 783 lines, including the
full test module).

- `import_spt_scene` has no `Err`/`Option` return — it always produces
  exactly one node + one mesh. Confirmed by signature (`-> ImportedScene`,
  infallible) and by the fact every code path in the function body is
  unconditional construction, no early return.
- Leaf-texture precedence: `params.leaf_texture_override` (`TREE.ICON`)
  `.or_else(...)` falls back to `scene.leaf_textures().first()` (`.spt`
  tag 4003, first-wins on duplicates) `.or_else` implicit final `None` if
  neither — texture path left unset so the renderer's own missing-texture
  placeholder takes over. Tests `leaf_texture_override_wins_over_spt_tag`,
  `falls_back_to_spt_leaf_tag_when_no_override`,
  `empty_texture_leaves_path_unset_for_renderer_placeholder` all pass and
  match the described behavior exactly.
- `compute_billboard_size` precedence, read directly: `if let Some(bounds)`
  (OBND) → else `if let Some(billboard_size)` (BNAM) → else
  `if let Some(bound_radius).filter(|r| *r > 0.0)` (MODB, `(R, 2R)`) →
  else the `256×512` default. Every returning branch clamps both
  dimensions to `[16.0, 8192.0]` via `.clamp(...)` after `.abs()` (so
  inverted/negative bounds recover a positive magnitude before clamping).
  8 dedicated tests (`bnam_drives_placeholder_size_when_obnd_absent`,
  `obnd_precedence_over_bnam`, `bnam_clamps_to_safe_band`,
  `bnam_precedence_over_modb`, `modb_drives_placeholder_size_when_obnd_absent`,
  `modb_clamps_to_safe_band`, `obnd_precedence_over_modb`,
  `corrupt_obnd_clamps_size_to_safe_band`) all pass and pin the exact
  precedence order + clamp band from the skill's checklist and the
  #1001/#1002 finding history.
- Normal/winding: front-face normals `[0.0, 0.0, -1.0]` on all 4 verts;
  index winding `[0, 3, 2, 2, 1, 0]`. Directly recomputed the triangle-0
  geometric normal from the position data by hand (cross product of
  edges `p1-p0` and `p2-p0` with `p0/p1/p2` = indices `0,3,2` = BL, TL,
  TR at the default 256×512 size): `e1 = (256,512,0)`, `e2 = (256,0,0)`,
  cross-Z = `e1.x*e2.y - e1.y*e2.x = 256*0 - 512*256 = -131072` — negative,
  confirming the CCW-from−Z convention the code and its two dedicated
  tests (`placeholder_normals_point_negative_z_for_billboard_arc`,
  `placeholder_index_winding_produces_negative_z_geometric_normal`) both
  assert.
- `bs_bound` Z-up→Y-up: `center_yup = zup_to_yup_pos([cx,cy,cz])`,
  `half_yup = [hx, hz, hy]` — routes through the shared coordinate helper
  (not a hand-rolled swap), matching the NIF importer's own convention.
  `placeholder_uses_obnd_bounds_when_present` pins a concrete example
  (Z-up center `(0,0,400)` → Y-up `(0,400,0)`, half-extents
  `(50,50,400)`→`(50,400,50)`) and passes.
- Two-sided cutout: `alpha_test: true`, `alpha_threshold: 0.5`,
  `alpha_test_func: 6` (GREATEREQUAL), `has_alpha: false`, `two_sided:
  true` — all set as literal struct fields, matching the checklist
  exactly; `has_alpha`/`alpha_test` are mutually exclusive by construction
  (one is a hardcoded `false`, the other a hardcoded `true`).

The two mechanical field additions since the last audit
(`thin_glass: false` from `883f57cd`) keep the placeholder honest as
`ImportedMesh` grows new fields — correctly defaulted to inert, not a bug.

### Dimension 3 — TREE → Billboard Wiring: **CLEAN**

- `references/import.rs:288-326` (`parse_and_import_spt`): returns `None`
  (not `Err`/panic) when `parse_spt` fails, logged via `log::warn!` — the
  REFR is skipped, the rest of the cell load continues. Confirmed by
  reading the function directly; matches the S1 acceptance contract
  ("never an `Err` out of the cell loader").
  `SptImportParams` is built from the matching `record_index.trees` entry
  keyed by the same form ID resolved through `index.statics` (dual-target
  lookup, per the TREE record design) — unchanged from prior audits.
- `spawn.rs:412-418`: `if let Some(mode) = cached.placement_root_billboard
  { world.insert(placement_root, Billboard::new(mode)); }` — verified this
  cycle by direct grep + read, not carried forward from the prior report.
  This is the #994 regression guard and it holds.
- `crates/plugin/src/esm/records/tree.rs`: `has_speedtree_binary()` is the
  case-insensitive `.spt` extension predicate (tested:
  `has_speedtree_binary_is_case_insensitive`, plus a negative test for a
  `.nif`-pointing TREE record). `parse_tree` captures CNAM with explicit
  shape tolerance — Oblivion (5×f32) vs FO3/FNV (8×f32) — both shapes
  directly asserted in `parse_tree_oblivion_shape`-style tests
  (`canopy_params.len() == 5` / `== 8` per fixture). BNAM is `None` on
  Oblivion/Skyrim+, `Some((w,h))` on FO3/FNV, matching the doc comment.
  No mis-shape-tolerance bug found.
- No commit in the change window touches `crates/plugin/src/esm/records/tree.rs`
  or `crates/plugin/src/esm/records/grup_walker.rs` — the #1889-era VWD
  wiring traced in the 2026-07-16 audit is untouched this cycle.

### Dimension 4 — Per-Game Variants & Route Divergence: **CLEAN**

- `crates/spt/src/version.rs` unchanged since 2026-05-09 (no commits in
  the window). `detect_variant`/`SpeedTreeVariant`/`MAGIC_HEAD` all as
  documented: `MAGIC_HEAD` is the exact 20-byte signature checked via
  `starts_with`, independent of `detect_variant`'s guess.
  `detect_variant` still has its two production callers
  (`references/import.rs:303`, `nif_loader.rs`) — both logging-only,
  confirmed by direct read this cycle. #1820 guard holds.
- Route divergence: `nif_loader.rs`'s `--tree` loose path still calls
  `import_spt_scene(&imported_scene_input, &SptImportParams::default(),
  ...)` — confirmed no TREE metadata threaded (no ICON override, no OBND/
  BNAM sizing), same as every prior audit. This is a documented, accepted
  limitation of the loose-file visualizer path, not a defect in the cell
  route.

### Dimension 5 — Tag Dictionary: **CLEAN**

No commits touch `tag.rs` this window. Unit tests
(`fixed_byte_payload_tags`, `string_payload_tags`, `u32_payload_tags`,
`vec3_payload_tags`, `unknown_for_out_of_dictionary_tags`,
`tag_13005_is_maybe_string_else_bare`, `bare_markers_round_trip`,
`u8_payload_tags`) all pass this run. The live corpus histogram (above)
is byte-identical to prior runs, so no new confounder tag has surfaced
in the 133-file vanilla corpus.

### Dimension 6 — NIFAL Material Translation for Placeholders: **CLEAN**

- `placeholder_billboard_mesh` (read in full above): `is_pbr: false`,
  `from_bgsm: false`, `bgem_glass: false`, `thin_glass: false` (new this
  window, correctly inert), `metalness_override: Some(0.0)`,
  `roughness_override: Some(0.85)`, `emissive_source:
  EmissiveSource::None`. Test
  `placeholder_billboard_sets_foliage_pbr_overrides_regardless_of_texture_path`
  exercises all three real vanilla-style leaf-texture-path keyword
  collisions (Boxwood→wood, Elderberry→glass-seam, generic) and confirms
  the overrides survive regardless of texture path — the #1819 guard
  holds.
- `material_translate.rs:157-158`: `metalness: mesh.metalness_override
  .unwrap_or(f32::NAN)`, `roughness: mesh.roughness_override
  .unwrap_or(f32::NAN)` — unchanged. Since the SpeedTree path always sets
  `Some(...)`, the placeholder never falls through to
  `Material::resolve_pbr`'s NaN-triggered keyword-classifier arm. Both the
  cell-loader route and the `--tree` loose route funnel through this same
  `translate_material` call (single boundary preserved).
- No commit in the window touches `crates/core/src/ecs/components/material.rs`'s
  `resolve_pbr` in a way that changes this contract (only unrelated
  light-anim/volumetric refactors touched `material_translate.rs`, and the
  PBR-resolution lines are byte-identical to the last audit).

---

## Findings

None. Zero new findings this cycle. The only open tracked item
(#1822 / SPT-NEW-07) is unchanged, already filed, and correctly out of
scope for re-reporting per the dedup protocol — re-verified directly
against current `parser.rs` source this cycle (see Dimension 1 above),
not just carried forward from the prior report's text.

---

## Regression Guards (verified in place, NOT re-reported)

| Finding | Issue | Guard verified this cycle |
|---|---|---|
| SPT-D4-01 (cell placeholder loses `Billboard`) | #994 | `spawn.rs:412-418` inserts `Billboard` when `placement_root_billboard.is_some()` — read directly |
| SPT-D4-02 (`bs_bound` Z-up→Y-up) | #995 | `import/mod.rs` routes center via `zup_to_yup_pos`, half-extents `(hx,hz,hy)` — read directly; `placeholder_uses_obnd_bounds_when_present` passes |
| SPT-D5-01 (`wind` docstring) | #996 | `SptImportParams.wind` doc still says CNAM, not BNAM |
| SPT-D2-01 ("first wins" leaf tex) | #997 | `import/mod.rs` `.first()`; `leaf_texture_override_wins_over_spt_tag` passes |
| SPT-D3-01 (pinned regression sample) | #998 | `tests/parse_synthetic_spt.rs` byte-pinned fixture passes |
| SPT-D1-01 (13005 bimodal) | #999 | `MaybeStringElseBare`; both `tag_13005_*` guards pass (residual tail edge tracked as #1822/SPT-NEW-07) |
| SPT-D4-03 (normal/winding) | #1000 | `-Z` normals + `[0,3,2,2,1,0]` winding; hand-recomputed cross product confirms negative Z this cycle |
| SPT-D4-04 (default size / MODB) | #1001 | `compute_billboard_size` OBND→BNAM→MODB→default; `modb_drives_placeholder_size_when_obnd_absent` passes |
| SPT-D5-02 (BNAM precedence) | #1002 | OBND-beats-BNAM; `obnd_precedence_over_bnam` passes |
| BSXFlags dropped at spawn | #1214 | `bsx_flags = 0` synthetic default |
| SceneFlags / root_flags | #1235 | `root_flags = 0` synthetic default |
| SPT-NEW-02/03/04 doc/route | #1707/#1711/#1715 | All CLOSED; `BsRotateAboutUp` fallback doc (`billboard.rs:124-138`) matches code |
| SPT-NEW-05 (foliage keyword collision) | #1819 | `metalness_override: Some(0.0)` / `roughness_override: Some(0.85)`; regression test passes |
| SPT-NEW-01 (`detect_variant` dead code) | #1820 | Two production (logging-only) callers confirmed this cycle |
| SPT-NEW-06 (format-notes.md byte-offset nit) | #1821 | Doc still byte-accurate |

The **14000-band Oblivion tail** (4 files bail at tag `768`) remains the
documented `format-notes.md` Phase-1 limitation, above the 95 % gate,
placeholder-covered — not re-reported.

---

## Per-Dimension Bill of Health

| Dimension | Verdict | Notes |
|---|---|---|
| 1 — Walker Byte-Accounting | Clean (1 residual edge tracked as #1822) | Verified byte counts, fatal/non-fatal error split, 64 KiB caps, LE-only reads directly against `parser.rs`/`stream.rs` source this cycle. |
| 2 — Placeholder Fallback | Clean | `import_spt_scene` infallible; precedence chains, clamps, Z-up→Y-up, `-Z` winding hand-verified (cross-product recomputed) + test-covered. |
| 3 — TREE → Billboard Wiring | Clean | `Billboard` insertion, `parse_and_import_spt`'s graceful `None`-on-fail, and CNAM shape tolerance all re-verified directly. |
| 4 — Per-Game Variants & Route Divergence | Clean | Both routes call `parse_spt` + `import_spt_scene`; `detect_variant` logging-only; `MAGIC_HEAD` exact-match unchanged. |
| 5 — Tag Dictionary | Clean | No edits this window; live corpus histogram byte-identical to five prior audit runs. |
| 6 — NIFAL Material Translation | Clean | Placeholder canonicalised at the single `translate_material` boundary on both routes; foliage PBR overrides survive keyword-collision texture paths. |

The S1 placeholder contract — graceful fallback over zero rendering, never
an `Err` out of the cell loader, `.spt` REFRs routed to the SpeedTree
importer — holds end to end (live corpus run + unit suite green, matching
five prior audits' numbers exactly).

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
health across six consecutive audit cycles (2026-06-23, 07-01, 07-02,
07-03, 07-16, 07-25). The only change to in-scope code since the last
audit was one mechanical struct-literal field addition
(`thin_glass: false`, forced by an unrelated renderer glass/occlusion
change) — verified correct. Every other commit touching a file in scope
this window (VMAD-to-first-synthetic-child gating, NIF streaming-worker
bsver reparse removal, terrain-collision consolidation, light-animation
refactors) was traced and confirmed orthogonal to the `.spt` route.

### Suggested next step

No new issues to file. `/audit-publish` is not needed this cycle — this
report only reconfirms clean status. #1822 (SPT-NEW-07) remains open and
tracked; no action needed on it from this audit.
