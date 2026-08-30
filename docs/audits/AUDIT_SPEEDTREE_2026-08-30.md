# SpeedTree Subsystem Audit — 2026-08-30

**Scope**: `crates/spt/` (`byroredux-spt`) — the `.spt` TLV parameter-section
walker (`parser.rs`, `stream.rs`, `tag.rs`, `version.rs`, `scene.rs`) and the
placeholder-billboard importer (`crates/spt/src/import/mod.rs`) — plus the
cross-cut wiring: `byroredux/src/cell_loader/references/synth_child.rs`,
`byroredux/src/cell_loader/references/import.rs`,
`byroredux/src/cell_loader/spawn.rs` +
`byroredux/src/cell_loader/spawn/mesh_instance.rs`,
`byroredux/src/cell_loader/nif_import_registry.rs`,
`byroredux/src/scene/nif_loader.rs`, `crates/plugin/src/esm/records/tree.rs`,
`crates/plugin/src/esm/records/dispatch_world_placement.rs`,
`byroredux/src/systems/billboard.rs`,
`byroredux/src/asset_provider/texture.rs` +
`byroredux/src/asset_provider/archive.rs` (the mesh-path normaliser),
`byroredux/src/material_translate.rs` (the NIFAL boundary),
`crates/spt/docs/format-notes.md`.

**Execution**: single-pass, solo — **no sub-agents dispatched**, per this run's
explicit constraint. All six dimensions were run in-process and written to
`/tmp/audit/speedtree/dim_1..6.md` before consolidation. The `/tmp/audit/speedtree/`
and `/tmp/audit/spt_bodies/` files dated 2026-08-28 are from the *previous*
cycle and were not reused; every number below was re-measured this run.

**Depth**: `deep`.

- `cargo test -p byroredux-spt` → **51/51 lib + 3/3 synthetic-corpus pass**
  (48 last cycle; #3529 and #3531 added three guards).
- Env-gated corpus acceptance harness, all three games on disk:
  ```
  [FNV] 10 files | 10 with entries | 0 hit unknown tag | 1800 entries  | 100.00 %
  [FO3] 10 files | 10 with entries | 0 hit unknown tag | 1800 entries  | 100.00 %
  [OBL] 113 files | 113 with entries | 4 hit unknown tag | 20425 entries | 96.46 %
      tag=768 (0x0300) in treems14willowoakyoungsu / treems14canvasfreesu /
      treecottonwoodsu / shrubms14boxwood
  ```
  Byte-identical file counts, entry counts and bail offsets to every cycle back
  to 2026-05-13 — the ≥ 95 % gate holds and #3531 changed no stopping point on
  real content.
- **Three new corpus censuses**, run for this audit and not carried from any
  prior report:
  1. **TREE record field census** over `FalloutNV.esm`, `Fallout3.esm`,
     `Oblivion.esm` — 154 `.spt`-bearing TREE records; presence and payload
     length of `OBND` / `MODB` / `BNAM` / `ICON` / `SNAM` / `CNAM`, plus which
     `compute_billboard_size` tier each record actually reaches.
  2. **TREE.MODL path-shape census** over the same three plugins.
  3. **BSA folder/file-record walk** of `Oblivion - Meshes.bsa`,
     `DLCShiveringIsles - Meshes.bsa` and both `Fallout - Meshes.bsa` to
     recover the real on-disk location of every vanilla `.spt`, then a
     simulation of the exact key `TextureProvider::extract_mesh` builds.
  Censuses 2 and 3 produced the HIGH below; census 1 produced the MEDIUM.

**Method**: diffed direction against `AUDIT_SPEEDTREE_2026-08-28.md`, re-derived
the status of each of its five findings from the commits since, then walked all
six dimensions against current code. Premises were verified before reporting:
**four candidate findings were raised and dropped as stale or disproved** — see
*Candidates raised and disproved*.

**Project constraint honoured**: no `.spt` TLV tag, field layout or offset is
asserted anywhere in this report. Where the format is unsettled (tags
`12002`/`12003`, the `CNAM` field semantics, and whether `BNAM` or `MODB` is the
correct Oblivion size source) the finding names the measurement and says the
semantics are *unattested*, rather than inventing one.

---

## Skill-file discrepancy (reported per dispatch)

`.claude/commands/audit-speedtree/SKILL.md` carries a stale corpus premise that,
followed literally, would have steered an auditor away from this cycle's MEDIUM:

- **Dimension 2 checklist**: *"OBND beats BNAM intentionally … Vanilla Oblivion
  ships MODB-only / no OBND, so an OBND-first-or-default path would render
  Cyrodiil pines at half scale. Guard the ordering."* The "MODB-only" half is
  wrong: **BNAM is present on 142 of 142** vanilla Oblivion `.spt` TREE records,
  sits *above* MODB in the shipped precedence, and is what every Oblivion tree
  actually resolves through. The checklist frames MODB as the live Oblivion tier
  and directs the auditor to confirm the ordering rather than measure it.
- **Dimension 3 checklist**: *"CNAM length not shape-tolerant across the 5-float
  Oblivion vs 8-float FO3/FNV split"*. There is no split — CNAM is **32 bytes /
  8 floats on 142/142 Oblivion, 9/9 FO3 and 3/3 FNV** records. The named risk
  cannot occur; the real defect is that three docstrings and a unit test assert
  the fictional split (D3-03 below).

Both premises trace back to the same source the code copied them from, so the
skill and the code are wrong in the same direction. Per the standing rule, the
code was measured rather than trusted, and the findings below are filed against
the code; the skill's two bullets should be corrected alongside them.

---

## Dedup pass (mandatory)

Fresh `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --search
"speedtree OR spt OR TREE"` pull this run → `/tmp/audit/speedtree/issues_new.json`
(20 rows).

| Issue | Short title | GitHub state | Code state at HEAD |
|---|---|---|---|
| **#3528** | every vanilla `TREE.ICON` is a bare filename | CLOSED (`19813460`) | Fixed. `resolve_tree_icon_path` (`references/import.rs:320-353`) probes verbatim → `trees\leaves\` → `trees\billboards\`, warns on a total miss, and is scoped to the SpeedTree route (`normalize_texture_path` untouched). Guard `vanilla_tree_icons_all_resolve` present. **Not regressed** — but see D3-01: the route this fix sits on never fires on vanilla content, so its benefit is currently unreachable. |
| **#3529** | `[16, 8192]` clamp is NaN-transparent | CLOSED (`19813460`) | Fixed. `clamp_billboard_extent` (`import/mod.rs:126-149`) is `Option`-returning and rejects non-finite before clamping; all three tiers route through it. No bare `f32::clamp` remains on any tier. **Not regressed.** |
| **#3531** | zero-length 13005 candidate taken as a String | CLOSED (`19813460`) | Fixed. `is_plausible_spt_curve_string` (`parser.rs:155-189`) rejects the empty slice ahead of the vacuous `all`. Guards `tag_13005_before_zero_leading_tail_resolves_as_bare` and `empty_candidate_is_not_a_plausible_curve_string` added; #1822's guard (leading tail value `8`) retained, so both sides are pinned. **Not regressed.** |
| **#1822** (SPT-NEW-07) | tag-13005 tail-swallow misparse | CLOSED | Still fixed; corpus bail offsets byte-identical. |
| **#3533** (SPT-2026-08-28-D3-02) | `placement_root_billboard` has no producer that can yield `Some` | **OPEN** | Premise re-verified at HEAD: `import_spt_scene` calls `placeholder_root_node(/* billboard */ false)` (`import/mod.rs:198`), so `nodes[0].billboard_mode` is always `None` and `parse_and_import_spt`'s `placement_root_billboard` (`references/import.rs:458-462`) can only ever be `None`; `spawn.rs:858` is dead for `.spt`. Live wiring is `mesh_instance.rs:794-801`. **Not re-filed.** |
| **#3535** (SPT-2026-08-28-D5-01) | tags `12002` / `12003` have no corpus evidence in `format-notes.md` | **OPEN** | Premise re-verified: `grep '12002\|12003' crates/spt/docs/format-notes.md` returns nothing. **Not re-filed.** |
| **#3191** (SPT-D2-2026-08-20-01) | wind bend composed in the object-local frame | **OPEN on GitHub — fixed in code** | `apply_speedtree_wind` (`billboard.rs:230-238`) builds the world-horizontal `axis = Vec3::new(-wind_dir.y, 0.0, wind_dir.x)` and pre-multiplies. Unchanged since the prior cycle verified it. Recommend closing; not re-filed. |
| **#3192**, **#3194**, **#3195**, **#3078**, **#3080**, **#3123**, **#3190**, **#3193**, **#3275**, **#3276** | prior-cycle SpeedTree items | CLOSED | All re-checked at HEAD, all still in place. |
| **#1711** (SPT-NEW-03) | `bs_bound` not carried through `CachedNifImport` | documented divergence | Still the deliberate, commented decision (`nif_import_registry.rs:215-231`). Not a finding. |

No open issue in the search covers any of the five findings below.
`gh issue list --state all --search "MODL"` and `--search "trees\\ spt path"`
return nothing matching D3-01.

---

## Change window

Commits touching this subsystem since `AUDIT_SPEEDTREE_2026-08-28.md`: exactly
one — `19813460` (#3528 / #3529 / #3531 in this domain, plus the unrelated
#3530). `crates/spt/src/{stream,tag,version,scene}.rs` and
`crates/plugin/src/esm/records/tree.rs` are unchanged; `parser.rs` and
`import/mod.rs` changed only in the two fixes above;
`byroredux/src/cell_loader/references/import.rs` gained
`resolve_tree_icon_path` + `TREE_ICON_CANDIDATE_DIRS` and a fifth
`tex_provider` parameter on `parse_and_import_spt`.

---

## Findings

### SPT-2026-08-30-D3-01: every vanilla `TREE.MODL` is a bare `\<Name>.spt`, so `extract_mesh` misses on 100 % of SpeedTree content and the placeholder importer is never reached

- **Severity**: HIGH
- **Dimension**: TREE→Billboard Wiring
- **Location**: `byroredux/src/cell_loader/references/synth_child.rs:449-553`; `byroredux/src/asset_provider/archive.rs:96-118` (`normalize_mesh_path`); `byroredux/src/asset_provider/texture.rs:57-65` (`extract_mesh`)
- **Status**: NEW
- **Description**: The `.spt` dispatch at `synth_child.rs:513-533` is downstream
  of `tex_provider.extract_mesh(&model_path)`. That lookup never succeeds for a
  vanilla TREE record, on any of the three `.spt` games, so the `is_spt` arm is
  never entered: control takes the `None` arm at `:541`, logs
  `"SPT not found in BSA"` at debug level, increments `nif_not_found`, and the
  REFR is dropped. The entire Session-33 Phase 1 subsystem — walker, importer,
  billboard quad, `SpeedTreeWind`, and #3528's freshly-landed ICON resolver —
  is unreachable on shipped content. This is the same defect class as #3528
  (a Bethesda path convention the engine's single generic normaliser does not
  model) one step earlier in the chain, which is why #3528's guard did not
  catch it: `vanilla_tree_icons_all_resolve` pins ICON resolution, and nothing
  pins the MODL that feeds `extract_mesh`.
- **Evidence**: Measured this run over `FalloutNV.esm` / `Fallout3.esm` /
  `Oblivion.esm` and the four vanilla mesh archives.

  MODL path shape — **every** `.spt`-bearing TREE record, zero exceptions:

  | Game | `.spt` TREE records | leading-separator, no directory | has a directory | bare filename |
  |---|---:|---:|---:|---:|
  | Oblivion | 142 | **142** | 0 | 0 |
  | FO3 | 9 | **9** | 0 | 0 |
  | FNV | 3 | **3** | 0 | 0 |

  Samples: `\Dbush16.spt`, `\ShrubVineMapleSU.spt`, `\WhiteOak01.spt`,
  `\OasisElm02.spt`, `\Pine01.spt`.

  Where the archives actually keep them (folder/file-record walk):
  `Oblivion - Meshes.bsa` v103 → 113 `.spt`, all under `trees\`;
  `Fallout - Meshes.bsa` v104 (FO3 and FNV) → 10 `.spt`, all under `trees\`.
  Note the folder is `trees\`, **not** `meshes\trees\` — SpeedTree binaries live
  outside the `meshes\` root in all three games.

  The key the engine builds, traced through the three sites:

  ```
  synth_child.rs:449   model_path   = "meshes\" + "\WhiteOak01.spt"
                                    = r"meshes\\WhiteOak01.spt"
  archive.rs:98-102    normalize_mesh_path sees the "meshes\" head -> Cow::Borrowed
  bsa/mod.rs:124-126   normalize_path lowercases, '/'->'\\' (does not collapse "\\")
                       -> lookup key r"meshes\\whiteoak01.spt"
  archive holds        r"trees\whiteoak01.spt"                       => MISS
  ```

  Resolution rate, simulating that exact key against the real file tables:

  | Game | `.spt` TREE records | resolve with the current key | resolve as `trees\<name>` |
  |---|---:|---:|---:|
  | Oblivion (incl. Shivering Isles archive) | 142 | **0** | **142** |
  | FO3 | 9 | **0** | **9** |
  | FNV | 3 | **0** | **3** |
  | **total** | **154** | **0** | **154** |

- **Impact**: No SpeedTree placeholder billboard has ever rendered from a cell
  load on vanilla FNV, FO3 or Oblivion. Cyrodiil exteriors — which lean
  entirely on TREE REFRs for forest content — are treeless, and both Fallouts
  lose their `.spt` vegetation. Degradation is graceful (the cell loads, the
  REFR is skipped, nothing panics), which is why the symptom reads as "content
  gap" rather than "bug" and has survived every prior cycle. Blast radius is the
  whole `crates/spt` public surface plus #994/#997/#1000/#1001/#1002/#3528/#3529
  — all correct in isolation, none of them observable. The loose `--tree` route
  is unaffected (the user supplies the archive-internal path directly), which is
  exactly why the smoke path passes while the production path does not.
- **Related**: #3528 (same class, one step later, ICON); #1711; D3-02 below
  (both live on the `extract_mesh`/cache seam). The `Cyrodiil pines` framing in
  #1001 and #1002 presumes trees reach the screen at all.
- **Suggested Fix**: Give the SpeedTree route the same treatment #3528 gave
  ICON — a `.spt`-scoped resolver beside `resolve_tree_icon_path` that strips a
  leading separator and probes `trees\<name>.spt` before falling back to the
  authored value, keeping `normalize_mesh_path` (shared by every mesh consumer)
  untouched. Pin it with an env-gated corpus guard mirroring
  `vanilla_tree_icons_all_resolve` — assert that all 154 vanilla TREE MODLs
  resolve — and confirm the `trees\` root against the archives rather than
  hardcoding it from this report.

---

### SPT-2026-08-30-D4-01: `BNAM` ships on 100 % of vanilla Oblivion TREE records, so the `MODB` tier added by #1001 to size Cyrodiil trees is unreachable on every one of them

- **Severity**: MEDIUM
- **Dimension**: Per-Game Variants
- **Location**: `crates/spt/src/import/mod.rs:96-113` and `:244-297` (`compute_billboard_size`); `byroredux/src/cell_loader/references/import.rs:429-443`; `crates/plugin/src/esm/records/tree.rs:29-31` and `:97-99`
- **Status**: NEW
- **Description**: `compute_billboard_size`'s precedence is OBND → BNAM → MODB →
  default, and four separate comments justify the MODB tier as *the Oblivion
  path* on the grounds that Oblivion ships MODB and no OBND. The OBND half is
  true; the conclusion does not follow, because **BNAM is also present on every
  vanilla Oblivion TREE record** and sits above MODB. The MODB tier is therefore
  dead on the only game it exists for. Two of the four comments state the
  premise as fact — `import/mod.rs:98` calls BNAM "(FO3/FNV only)" and
  `tree.rs:97-99` says "`None` on Oblivion (BNAM absent there)" — and both are
  contradicted by the corpus.
- **Evidence**: Field-presence census over the three plugins, `.spt`-bearing
  TREE records only:

  | Game | records | OBND | MODB | BNAM | ICON | SNAM | CNAM | tier `compute_billboard_size` actually takes |
  |---|---:|---:|---:|---:|---:|---:|---:|---|
  | FNV | 3 | 100 % | 0 % | **100 %** | 100 % | 100 % | 100 % | OBND ×3 |
  | FO3 | 9 | 100 % | 0 % | **100 %** | 100 % | 100 % | 100 % | OBND ×9 |
  | Oblivion | 142 | 0 % | 100 % | **100 %** | 96 % | 99 % | 100 % | **BNAM ×142** |

  All 142 Oblivion BNAM payloads are the expected 8 bytes / 2 × f32, so they
  decode and win cleanly. **The MODB tier is reached by 0 records in any game.**

  Measured consequence over all 142 Oblivion records — BNAM-chosen height
  against the height the MODB tier would have produced:

  - height ratio: min **0.36**, median **0.41**, max **0.41**
  - 136 of 142 BNAMs are square (h/w median **1.000**) against the intended
    1:2 silhouette; only 6 are non-square
  - BNAM width / MODB radius: median **0.828** — so width is broadly sane and
    the divergence is almost entirely vertical
  - e.g. `Mbush16` BNAM `270×270` vs MODB-derived `326×652`; `Dbush15`
    `300×300` vs `362×724`

- **Impact**: Every Cyrodiil tree and shrub placeholder renders at roughly 41 %
  of the height #1001 intended and square rather than tall. Latent behind
  D3-01 today (nothing renders at all), so this becomes visible the moment
  D3-01 is fixed — and would then read as a *new* regression introduced by that
  fix. Also a live doc-rot hazard: four comments across two crates describe a
  precedence the code does not execute, and a future editor reading them will
  reason from the wrong corpus.
- **Related**: #1001, #1002, #3080 (the docstring repair that restated the same
  premise); D3-03 (the sibling corpus-premise error in the same record).
- **Suggested Fix**: Two separable pieces. (1) **Documentation, unconditional**:
  correct the four comments to state the measured presence table above; the
  claim "BNAM absent on Oblivion" is simply false. (2) **Behaviour, needs
  research first**: whether Oblivion should size from BNAM or MODB is a format
  question this audit deliberately does not answer — BNAM is plausibly the
  authored imposter-card dimension, which is precisely what the placeholder
  *is*, while `(R, 2R)` from a bounding-sphere radius is an admitted heuristic.
  Settle it against an attested TREE/BNAM definition or a screenshot comparison
  before reordering the tiers; do not reorder on this report alone.

---

### SPT-2026-08-30-D3-02: the import cache is keyed by model path, but `SptImportParams` is per-TREE-record, so records sharing one `.spt` all inherit the first one's size and texture

- **Severity**: LOW
- **Dimension**: TREE→Billboard Wiring
- **Location**: `byroredux/src/cell_loader/references/synth_child.rs:476-583`; `byroredux/src/cell_loader/nif_import_registry.rs:49-56` (`canonical_model_path_key`); `byroredux/src/cell_loader/references/import.rs:355-457`
- **Status**: NEW
- **Description**: `parse_and_import_spt` bakes the TREE record's ICON, OBND,
  MODB and BNAM into the returned `CachedNifImport`, but that value is stored
  under `canonical_model_path_key(&stat.model_path)` — the model path alone. It
  runs once per unique `.spt`, on whichever TREE record is placed first. Every
  other TREE record pointing at the same `.spt` silently receives the first
  record's leaf texture and billboard size. For NIF content the cache key is
  sound because the parse depends only on the file; for `.spt` the import
  depends on per-record metadata that the key does not capture.
- **Evidence**: Vanilla Oblivion has 142 `.spt` TREE records across 139 unique
  MODL values — exactly three collisions, and all three diverge:

  ```
  \ShrubVineMapleSU.spt        0x232db ShrubVineMapleSU        MODB  531.13  BNAM  440.00 x  440.00
                               0x017fc TestToddTree03          MODB  466.66  BNAM  550.36 x  368.66
  \TreeSilverBirchForestSU.spt 0x009fe TreeSilverBirchForestSU MODB 1931.37  BNAM 1600.00 x 1600.00
                               0x00898 TestToddTree            MODB 2182.98  BNAM 1576.07 x 1808.44
  \TreeSugarMapleForestSU.spt  0x0089b TreeSugarMapleForestSU  MODB 1714.09  BNAM 1420.00 x 1420.00
                               0x00899 TestToddTree02          MODB 2139.71  BNAM 1600.14 x 1772.59
  ```

  ICON is identical within each pair, so only sizing diverges. FNV (3/3) and
  FO3 (9/9) have no collisions at all.
- **Impact**: Vanilla yield is effectively nil — all three second records are
  `TestToddTree*` developer stubs. The mechanism matters for mod content: the
  documented rationale for ICON-winning-over-tag-4003 is *"mods retexture trees
  by re-pointing TREE ICON without rewriting the `.spt`"*
  (`import/mod.rs:65-68`), which is precisely the N-records-one-file shape this
  cache collapses. A retexture mod would see one of its variants take over the
  others' leaves.
- **Related**: D3-01 (both on the `extract_mesh`/cache seam); #3038 (the last
  cache-key normalisation fix); #523.
- **Suggested Fix**: For the `.spt` branch only, extend the cache key with the
  TREE form id (or with a hash of the four consumed fields) so records that
  differ get distinct entries while identical records still share one parse.
  Leave the NIF key alone.

---

### SPT-2026-08-30-D3-03: `CNAM` is 8 floats on all three games — the documented "5 × f32 on Oblivion" split is fiction, and a unit test pins it

- **Severity**: LOW
- **Dimension**: TREE→Billboard Wiring
- **Location**: `crates/plugin/src/esm/records/tree.rs:25-28`, `:92-96`, `:161-170`, `:281-300` (`parse_oblivion_short_cnam_no_bnam_no_pfig`); `crates/spt/src/import/mod.rs:75-80`
- **Status**: NEW
- **Description**: Three docstrings and one test assert that `CNAM` carries 5
  floats on Oblivion and 8 on FO3/FNV. Measured, the payload is **32 bytes / 8
  floats on 142/142 Oblivion, 9/9 FO3 and 3/3 FNV** records — there is no split.
  The parser itself is length-tolerant (`while let Ok(v) = r.f32()`), so nothing
  mis-parses today; the defect is that the documented input contract is wrong,
  and the only test covering the Oblivion shape is a synthetic fixture matching
  no vanilla record. Its name,
  `parse_oblivion_short_cnam_no_bnam_no_pfig`, asserts two false vanilla facts
  in one identifier — the short CNAM, and (per D4-01) the absent BNAM.
- **Evidence**: `CNAM` payload-length histogram, `.spt`-bearing TREE records:
  Oblivion `{32: 142}`, FO3 `{32: 9}`, FNV `{32: 3}`. No other length occurs.
  The `canopy_params: Vec<f32>` field length therefore reads 8 everywhere.
- **Impact**: No live parse defect — `canopy_params` is parse-but-don't-consume
  behind the #3190 gate. The cost is entirely forward-looking and lands on that
  gate: a wind decoder written against "5 floats on Oblivion" will index the
  wrong slots on 100 % of Cyrodiil trees. The field semantics remain
  **unattested** and this audit does not propose any; only the count is
  measured.
- **Related**: #3190 (the deferred consumer); #3276 (the last CNAM docstring
  repair, which corrected the *wind-source* claim but left the 5-vs-8 claim
  standing); D4-01 (same record, same class of corpus-premise error).
- **Suggested Fix**: Replace the 5/8 claim with the measured 8-everywhere fact
  in all four places, and rename/retarget the Oblivion test to a real vanilla
  shape (8-float CNAM, BNAM present, no OBND, MODB present) so it pins the
  corpus instead of a fiction. Keep the length-tolerant reader — it is the right
  shape for mod content — but do not describe the tolerance as covering a split
  that does not exist.

---

### SPT-2026-08-30-D1-01: `parse_spt`'s error contract is documented as two fatal conditions but has five, and three of them discard the entries already decoded

- **Severity**: LOW
- **Dimension**: Walker Byte-Accounting
- **Location**: `crates/spt/src/parser.rs:39-46` (contract docstring), `:192-249` (`read_payload`); `crates/spt/src/stream.rs:88-106` (`read_string_lp`)
- **Status**: NEW
- **Description**: The contract says *"Returns `Err(io::Error)` only on truly
  fatal conditions — magic-header mismatch or stream underflow during a
  partially-read payload."* Three further `Err` paths exist: `read_string_lp`'s
  > 64 KiB length cap, `read_payload`'s `count × stride` > 64 KiB array cap
  (`parser.rs:196-208`), and the defensive context-sensitive-kind arm
  (`:236-247`). All three return `InvalidData`, and none is an underflow. More
  substantively, all five fatal paths throw away the whole `SptScene` —
  including every `TagEntry` already decoded — whereas the in-range-unknown-tag
  path, which is the *same* "we can no longer trust the byte stream" situation,
  records `tail_offset` and returns everything decoded so far.
- **Evidence**: The two sanity caps are correct in themselves — both bound the
  *byte* count, not the element count, and the array cap is computed in `u64`
  before any allocation. The asymmetry is only in the failure handling:

  ```rust
  // parser.rs:73-82 — in-range unknown tag: keep everything, record where we stopped
  scene.unknown_tags.push((tag, tag_offset));
  scene.tail_offset = tag_offset;
  return Ok(scene);

  // parser.rs:199-207 — oversized array: discard the whole scene
  return Err(io::Error::new(io::ErrorKind::InvalidData, format!(...)));
  ```
- **Impact**: Bounded. The cell route degrades an `Err` to `SptScene::default()`
  (#3078) and the loose route does the same (#3195), so nothing crashes and the
  placeholder still renders — the cost is losing tag `2000`/`4003` on a file the
  walker could have kept the head of, which only matters for a TREE record with
  no ICON (6 of 142 vanilla Oblivion records). The doc-vs-code drift is the more
  durable cost: a future caller reading the contract will not expect
  `InvalidData` from a well-formed-but-large payload.
- **Related**: #3078, #3195 (the two degrade sites that currently absorb this).
- **Suggested Fix**: Either update the contract docstring to enumerate all five
  conditions, or — preferably — treat the two sanity-cap breaches the way an
  unknown tag is treated: record the offset into a diagnostic field, set
  `tail_offset`, and return `Ok`. Leave the magic mismatch and the mid-payload
  underflow fatal.

---

## Dimension summary (every dimension enumerated)

| Dimension | Findings | Notes |
|---|---:|---|
| 1 — Walker Byte-Accounting | **1** (LOW) | `parser.rs`/`stream.rs`/`tag.rs` re-read in full. Every `SptTagKind` decode advances exactly its claimed width (`Bare` 0, `U8` 1, `U32` 4, `Vec3` 12, `FixedBytes(n)` n, `String` 4+len, `ArrayBytes` 4+count×stride), cross-checked arm-by-arm against `dispatch_tag`. Both 64 KiB caps are on the byte count. LE-only, no host/BE read. `peek_u32_le` and `peek_string_lp_bytes` both guard `remaining() < 4`, `checked_add` the end, and never consume. #3531 verified in place with both new guards. Corpus byte-identical to prior cycles. Finding is the error-contract asymmetry. |
| 2 — Placeholder Fallback | **0** | `import_spt_scene` still has no `Err` path — one node, one mesh, unconditionally; the only `None` into the cell loader is a `parse_spt` `Err`, degraded at `references/import.rs:390-394`. Leaf-texture precedence, `bs_bound` Z-up→Y-up via `zup_to_yup_pos` with `(hx, hz, hy)`, `-Z` normals, `[0,3,2,2,1,0]` winding and the cutout fields are all intact with their guards. #3529's `clamp_billboard_extent` verified `Option`-returning on all three tiers. `BsRotateAboutUp` is still the documented world-up yaw lock; checked the antipodal case specifically — `Quat::from_rotation_arc(-Z, +Z)` resolves through `any_orthogonal_vector((0,0,-1)) = (0,-1,0)`, a pure 180° yaw, so no pitch flip. |
| 3 — TREE→Billboard Wiring | **3** (1 HIGH, 2 LOW) | The HIGH is the MODL resolution failure. Verified clean: mixed `.nif`/`.spt` coexistence, the `CachedNifImport` synthetic defaults (`bsx_flags: 0`, `root_flags: 0`, `flame_attach_offset: None`, `attach_points: None`, `furniture: None`), `TreeRecord` capture for the four consumed fields, SNAM `chunks_exact` tolerance, `has_speedtree_binary` case-insensitivity, and `mesh_instance.rs:794-801` attaching `Billboard` + `SpeedTreeWind` on the render child. `.spt` goes through the same `extract_mesh` chain as `.nif` — no parallel resolver, which is exactly why it inherits the `meshes\` assumption. |
| 4 — Per-Game Variants & Route Divergence | **1** (MEDIUM) | `version.rs` untouched; `MAGIC_HEAD` is the exact 20 bytes and rejects a one-byte flip or any input under 20 B; `detect_variant` remains log-only with zero downstream branching (`references/import.rs:371`, `nif_loader.rs:223` — both diagnostic per #1820). Both routes call `parse_spt` + `import_spt_scene` and both degrade a parse error to the placeholder. The two `is_spt` predicates differ in form but agree on `.SPT`. The documented `SptImportParams::default()` gap on the loose `--tree` route stands and is understood. Finding is the BNAM/MODB corpus-premise failure. |
| 5 — Tag Dictionary | **0** | `tag.rs` unchanged since 2026-06-09; `dispatch_tag` maps **119** distinct tag values. Spot-checked 8003/8005/8009 = 52 B, 13008 = 11 B, 13013 = 7 B, 12002 = 16 B, 12003 = 20 B, 10002 stride 1, 10003 stride 8 — all consistent with the walker's use and the unit tests. Confounders 100 / 110 / 4096 / 5376 / 11776 / 13568 and 0 / 1 / 50 / 19985 / `u32::MAX` all still `Unknown`. The four Oblivion `tag=768` bails are the known documented case and the 96.46 % rate clears the gate. The `12002`/`12003` evidence gap is #3535, already open. |
| 6 — NIFAL Material Translation | **0** | Single boundary holds: the only two material sites are `spawn/mesh_instance.rs:634` and `scene/nif_loader.rs:959`, both `translate_material`; no parallel "spt material" path and no BGSM/BGEM resolve for `.spt` (`material_path` stays `None`). `metalness_override: Some(0.0)` / `roughness_override: Some(0.85)` still set explicitly at import (#1819) with their guard. `is_pbr`, `from_bgsm` and `emissive_source` inherit `ImportedMaterial::default()` = `false` / `false` / `EmissiveSource::None` (`crates/nif/src/import/types.rs:667`, `:673`, `:695`). Two-sided alpha-test cutout carries through intact. |

**Totals**: 6 dimensions, **5 findings** — 0 CRITICAL, **1 HIGH**, **1 MEDIUM**,
**3 LOW**. Dimensions **2, 5 and 6 produced no findings**.

---

## Candidates raised and disproved (not reported)

Four candidates were raised and dropped after checking the premise against
current code — the stale-premise rate for this cycle is 4 dropped / 9 raised.

1. **"The `[16, 8192]` clamp is still NaN-transparent"** — stale. That was
   #3529 and it landed in `19813460`; `clamp_billboard_extent` is
   `Option`-returning and every tier routes through it. Re-read all three tiers
   specifically to confirm no bare `clamp` was reinstated.
2. **"`TREE.ICON` still normalises to a path in no archive"** — stale. #3528
   landed in the same commit; `resolve_tree_icon_path` probes verbatim →
   `trees\leaves\` → `trees\billboards\` and is correctly scoped to the
   SpeedTree route rather than hoisted into `normalize_texture_path`.
3. **"`placement_root_billboard` is dead, so `.spt` REFRs spawn static"** —
   premise half-true, already filed. The seam *is* dead (`placeholder_root_node`
   is called with `billboard: false`), but the live attach moved to
   `mesh_instance.rs:794-801` under #3076 and works; the dead seam is #3533,
   open, not re-filed.
4. **"Oblivion `.spt` MODL paths carrying a leading separator would
   double-prefix the *registry* key and split the cache"** — real but
   inconsequential. `canonical_model_path_key("\\Dbush16.spt")` does produce
   `meshes\\dbush16.spt` with a doubled separator, but it is applied
   consistently at both construction sites (`synth_child.rs:476` and the
   streaming loader, per #3038), so the key is stable and self-consistent. It
   is not a second bug — it is the same missing `trees\` convention as D3-01,
   observed on the cache key instead of the lookup key, and folds into that
   finding.

Additionally, `#3191` was re-checked and remains **fixed in code while open on
GitHub** — `apply_speedtree_wind` builds a world-horizontal axis and
pre-multiplies. Recommend closing it rather than re-auditing next cycle.

---

## Summary

The subsystem's internals are in good shape: the walker's byte accounting is
exact arm-for-arm, the corpus gate holds at byte-identical numbers going back
three and a half months, the placeholder importer still cannot fail, the NIFAL
boundary is single, and all three fixes from `19813460` are in place with guards.

What this cycle found is that none of it is reachable. Every vanilla `TREE.MODL`
across FNV, FO3 and Oblivion is a bare `\<Name>.spt`, the archives keep those
files under a top-level `trees\` folder outside the `meshes\` root, and the
engine's single generic mesh normaliser builds a key that matches nothing —
0 of 154 records resolve. The `.spt` branch in `synth_child.rs` has therefore
never executed on shipped content. Behind that gate sits a second corpus-premise
error of the same shape: `BNAM` is on 100 % of Oblivion TREE records rather than
absent as four comments claim, so the `MODB` tier written for Cyrodiil is
unreachable and Oblivion trees would render at a median 0.41× their intended
height the moment the first defect is fixed.

Both defects share a root cause worth naming: the subsystem's per-game
assumptions were written down as prose and never pinned by a corpus assertion.
#3528 added the first such assertion (`vanilla_tree_icons_all_resolve`) and it
works — it just guards the field one step past the one that fails. Extending
that pattern to MODL resolution and to the field-presence table would close both
findings and prevent the next one.

### Suggested next step

`/audit-publish docs/audits/AUDIT_SPEEDTREE_2026-08-30.md`

Labels: `speedtree` + `terrain-exterior` on all five. Add `import-pipeline` to
D3-01 and D3-02, `esm-plugin` + `doc-rot` to D3-03, `doc-rot` to D4-01 and
D1-01. Game labels: `game:oblivion` on D4-01, D3-02 and D3-03;
`game:oblivion` + `game:fnv` + `game:fo3` on D3-01. Types: `bug` for D3-01,
D4-01 and D3-02; `documentation` for D3-03 and D1-01.

Fix order: D3-01 first and alone — it is the gate, and every other finding's
visible impact is behind it. D4-01's documentation half can land with it; its
behavioural half should wait for an attested BNAM definition.
