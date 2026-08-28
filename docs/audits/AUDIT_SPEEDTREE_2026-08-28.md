# SpeedTree Subsystem Audit — 2026-08-28

**Scope**: `crates/spt/` (`byroredux-spt`) — the `.spt` TLV parameter-section
walker (`parser.rs`, `stream.rs`, `tag.rs`, `version.rs`, `scene.rs`) and the
placeholder-billboard importer (`crates/spt/src/import/mod.rs`) — plus the
cross-cut wiring: `byroredux/src/cell_loader/references/synth_child.rs`
(the `is_spt` dispatch), `byroredux/src/cell_loader/references/import.rs`
(`parse_and_import_spt`), `byroredux/src/cell_loader/spawn.rs`,
`byroredux/src/cell_loader/spawn/mesh_instance.rs`,
`byroredux/src/cell_loader/nif_import_registry.rs`,
`byroredux/src/scene/nif_loader.rs`, `crates/plugin/src/esm/records/tree.rs`,
`byroredux/src/systems/billboard.rs`, `byroredux/src/boot.rs`,
`byroredux/src/asset_provider/texture.rs`,
`byroredux/src/asset_provider/archive.rs` (the texture-path normaliser the
placeholder's only visible surface goes through),
`byroredux/src/material_translate.rs` + `byroredux/src/helpers.rs` (the NIFAL
boundary), `crates/spt/docs/format-notes.md`.

Single-pass, solo execution per this run's explicit constraint — **no
sub-agents dispatched**. All six dimensions read, traced and verified
directly (Bash/Read/Grep only).

**Depth**: `deep`. Beyond source review this cycle ran

- `cargo test -p byroredux-spt --lib` → **48/48 pass**.
- `cargo test -p byroredux --bin byroredux billboard` → **12/12 pass**
  (includes `parked_camera_wind_pass_skips_a_marked_entity_without_billboard`,
  `scheduler_access_tests::billboard_declaration_matches_shared_clock_and_component_surface`
  and `cell_loader::references::import_tests::parse_and_import_spt_surfaces_billboard_mode_on_mesh`).
- The env-gated corpus acceptance harness against all three on-disk games:
  ```
  [FNV] 10 files | 10 with entries | 0 hit unknown tag | 1800 entries total  | 100.00 % coverage
  [FO3] 10 files | 10 with entries | 0 hit unknown tag | 1800 entries total  | 100.00 % coverage
  [OBL] 113 files | 113 with entries | 4 hit unknown tag | 20425 entries total | 96.46 % coverage
      trees\treems14canvasfreesu.spt      | tag=768 (0x0300) at offset 6211
      trees\shrubms14boxwood.spt          | tag=768 (0x0300) at offset 4507
      trees\treecottonwoodsu.spt          | tag=768 (0x0300) at offset 5641
      trees\treems14willowoakyoungsu.spt  | tag=768 (0x0300) at offset 5946
  ```
  Byte-identical file counts, entry counts *and* bail offsets to every prior
  cycle back to 2026-05-13 — an independent confirmation that **#1822's
  `peek_string_lp_bytes` rewrite changed no walker stopping point on real
  content**, and that the ≥ 95 % gate still holds.
- Two independent corpus censuses run for this audit (not carried from a
  prior report):
  1. **TREE `ICON` census** over `FalloutNV.esm`, `Fallout3.esm` and
     `Oblivion.esm` — 90 unique values, cross-checked against
     `crates/plugin/tests/parse_real_esm.rs`'s own vanilla TREE counts
     (3 FNV / 9 FO3), which match exactly.
  2. **BSA folder/file-record walk** of `Fallout - Textures2.bsa` and
     `Oblivion - Textures - Compressed.bsa` to recover the *actual* on-disk
     directory of the leaf textures those ICON values name.
  Both censuses are what produced the HIGH finding below; neither had been
  run in any prior SpeedTree cycle.

**Method**: diffed direction against `AUDIT_SPEEDTREE_2026-08-24.md` (the
most recent prior cycle), re-derived the status of every one of its findings
from the commits that landed since, then walked all six dimensions. Per the
dispatch, the `#3192` re-fix (`8e97b4e5`, "drive the parked-camera billboard
pass from the SpeedTreeWind set") got a dedicated coherence pass — reported
in full under **Primary check** below.

**Project constraint honoured**: no `.spt` TLV field layout is asserted
anywhere in this report. Where the format is unsettled (tags `12002`/`12003`,
the empty-curve-string arm of tag `13005`, and the Bethesda `TREE.ICON`
resolution rule) the finding says *needs research* and names the evidence
that would settle it, rather than proposing a value.

---

## Dedup pass (mandatory)

Cached: `/tmp/audit/speedtree/issues.json` (fresh `gh issue list … --search
"speedtree OR spt OR TREE"` pull, this run), plus targeted
`gh issue view` on every issue the prior cycle tracked.

| Issue | Short title | GitHub state | Code state at HEAD |
|---|---|---|---|
| **#1822** (SPT-NEW-07) | tag-13005 tail-swallow misparse | CLOSED | Fixed. `peek_string_lp_bytes` + `is_plausible_spt_curve_string` re-read in full; `checked_add` bound, no consume on a failed peek. Corpus run above shows byte-identical bail offsets to the pre-fix cycles ⇒ no regression. One residual sliver filed below as **SPT-2026-08-28-D1-01** (empty candidate string), which the #1822 fix does not cover. |
| **#3078** | fatal `parse_spt` discards a recoverable placeholder | CLOSED | Fixed; `references/import.rs:307-311` degrades to `SptScene::default()` with `log::warn!`. |
| **#3080** | `import/mod.rs` docstring documents pre-#1001/#1002 size chain | **CLOSED** this window (`0dc59c2b`) | Verified: `crates/spt/src/import/mod.rs:22-27` now spells out `OBND → BNAM → MODB → 256 × 512` with the `[16, 8192]` clamp. Prior cycle listed this as still-open; it is closed. |
| **#3123** | billboard system reads `TotalTime` undeclared | CLOSED | Fixed (`boot.rs:1248`). |
| **#3190** | `SpeedTreeWind` built from unpinned CNAM floats | CLOSED | Fixed by deletion; `references/import.rs:328-332` hardcodes `let wind = Some((1.0, 0.0));`. |
| **#3191** | wind bend composed in object-local frame | **OPEN on GitHub — fixed in code** | `apply_speedtree_wind` (`billboard.rs:237`) builds the world-horizontal `axis = Vec3::new(-wind_dir.y, 0.0, wind_dir.x)` and **pre**-multiplies `Quat::from_axis_angle(axis, angle) * base`. Guards `speedtree_world_lean_is_camera_orbit_invariant` + `reversing_wind_reverses_mean_lean` both pass. Recommend closing; not re-filed. |
| **#3192** | parked-camera gate bypassed in windy exteriors | CLOSED (`8e97b4e5`) | Fixed, and re-verified independently — see **Primary check**. |
| **#3193** | dead geometry-tree wind branch | CLOSED | Resolved by deletion; premise re-confirmed at HEAD (no production entity carries `SpeedTreeWind` + `MeshHandle` without `Billboard`). |
| **#3194** | no non-finite gust guard | CLOSED | Still in place (`billboard.rs:218`). Note the sibling gap this audit found in `compute_billboard_size` (**SPT-2026-08-28-D2-01**) is the *same* NaN-transparency class in the other SpeedTree consumer. |
| **#3195** | loose `--tree` route deletes tree on parse error | CLOSED | Fixed; `nif_loader.rs:215-230` warns and degrades, both attach sites insert `SpeedTreeWind::new(1.0, 0.0)`. |
| **#3275** (SPT-D3-2026-08-24-01) | stale `MeshHandle` in the billboard `Access` | **CLOSED** this window (`3f634818`) | Verified: `boot.rs:1246-1252` no longer names `MeshHandle`, and `scheduler_access_tests::billboard_declaration_matches_shared_clock_and_component_surface` now pins the component surface in both directions. |
| **#3276** (SPT-D2-2026-08-24-01) | two stale CNAM wind docstrings | **CLOSED** this window (`a924244e`) | Verified: `crates/spt/src/import/mod.rs:70-79` and `nif_import_registry.rs:156-160` both now say "neutral runtime constant `(1.0, 0.0)` … parsed but not consumed until a citable field layout lands (#3190)". |

**Both findings from the 2026-08-24 cycle are closed.** No open issue in the
search covers any of the five findings filed below; `gh issue list --state
all --search "leaf texture ICON"` returns only #1819 (the *classifier*
collision), which is a different defect from the *resolution* failure filed
here as SPT-2026-08-28-D3-01.

---

## Change window

Commits touching this subsystem since `AUDIT_SPEEDTREE_2026-08-24.md`:
`3f634818` (#3275), `a924244e` (#3276 + three sibling doc repairs),
`0dc59c2b` (#3080), and `8e97b4e5` (#3192, the only behavioural change —
200 lines in `byroredux/src/systems/billboard.rs`, 4 in `systems/water.rs`).
`crates/spt/src/{parser,stream,tag,version,scene}.rs` are **unchanged since
`7453f565`** (the #1822 fix), and `crates/spt/src/import/mod.rs` changed only
in its docstrings.

---

## Primary check: is the `#3192` parked-camera path coherent at HEAD?

`8e97b4e5` split `make_billboard_system`'s single loop into two passes
(`byroredux/src/systems/billboard.rs:109-140`) sharing one body
(`update_billboard`, `:161-196`):

```rust
if camera_changed {
    for (entity, billboard) in bq.iter() {
        let tree_wind = swq.as_ref().and_then(|q| q.get(entity).copied());
        update_billboard(&mut gq, entity, billboard, tree_wind, pass);
    }
} else {
    let Some(swq) = swq.as_ref() else { return; };
    for (entity, tree_wind) in swq.iter() {
        let Some(billboard) = bq.get(entity) else { continue; };
        update_billboard(&mut gq, entity, billboard, Some(*tree_wind), pass);
    }
}
```

Verdict: **the path is coherent and behaviour-preserving.** Traced item by
item:

- **#1374's dirty-set guarantee holds and is now stronger.** The whole-system
  early-out (`:91-93`, `!camera_changed && !wind_active && !wind_state_changed`)
  is untouched; the new `else` arm additionally never *visits* an unmarked
  billboard, so `gq.get_mut` — the call that arms `GlobalTransform`'s
  TRACK_CHANGES set — cannot fire for one. That is a strict improvement over
  the `4e1afcbe` in-loop `continue`, which still walked the whole `Billboard`
  storage and did a `SpeedTreeWind` lookup per entity.
- **No behavioural divergence between the arms.** Both call the same
  `update_billboard`, which recomputes the base rotation and then applies the
  bend. On a parked frame the recomputed base is bit-identical (camera pose
  and entity translation both unchanged), so a marked tree is updated exactly
  as before and an unmarked billboard is written by neither arm.
- **The `!wind_active but wind_state_changed` case is handled.** Wind falling
  to calm still enters the `else` arm, and `update_billboard` writes the
  un-bent base rotation — trees relax rather than freezing mid-lean. Pinned
  by `active_weather_direction_change_rebends_stationary_speedtree`.
- **No new lock pair.** `gq` is still the single `query_mut::<GlobalTransform>()`
  handle from #829, threaded by `&mut` into `update_billboard` rather than
  re-acquired; the two early `return`s added (`:101-103` for a missing
  `Billboard` storage, `:129-131` for a missing `SpeedTreeWind` storage) sit
  *after* `last_cam` is updated (`:95`), so the camera sentinel cannot
  desynchronise on a frame that bails.
- **The `Access` declaration matches** (`boot.rs:1244-1252`:
  `ActiveCamera`, `TotalTime`, `WindField`, `Billboard`, `SpeedTreeWind`,
  writes `GlobalTransform`) — no over- or under-declaration, and #3275's new
  test pins it.

The one incoherence is in the *justification*, not the code: the new arm's
comment (and the commit message) name the placement root as the entity the
`bq.get(entity)` skip exists for, and at HEAD no production site attaches
`SpeedTreeWind` to a placement root. Filed as **SPT-2026-08-28-D3-02** (LOW).

---

## Findings

### SPT-2026-08-28-D3-01: every vanilla `TREE.ICON` is a bare filename, so the placeholder billboard's only visible surface never resolves

- **Severity**: HIGH
- **Dimension**: TREE→Billboard Wiring (secondary: Per-Game Variants)
- **Location**: `byroredux/src/cell_loader/references/import.rs:318-320` →
  `crates/spt/src/import/mod.rs:137-155` →
  `byroredux/src/asset_provider/archive.rs:274-300`
  (`normalize_texture_path`) → `byroredux/src/asset_provider/texture.rs:31-38`
- **Status**: NEW
- **Description**: The S1 deliverable is "a yaw-billboard quad **textured
  with the leaf texture**" (`crates/spt/src/import/mod.rs:16-27`). The leaf
  texture is taken from the TREE record's `ICON` sub-record, which wins over
  the `.spt`'s own tag 4003 (the tag-4003 path is additionally rejected for
  vanilla content, since those are absolute exporter paths —
  `is_relative_texture_path`, `import/mod.rs:352-359`). `ICON` is passed
  through verbatim:

  ```rust
  // byroredux/src/cell_loader/references/import.rs:318
  let leaf_texture_override = tree_record
      .map(|t| t.leaf_texture.as_str())
      .filter(|s| !s.is_empty());
  ```

  and lands unmodified in `MaterialTextureSet::base_color`
  (`crates/spt/src/import/mod.rs:137-155`). The archive lookup then applies
  the engine's only path normalisation:

  ```rust
  // byroredux/src/asset_provider/archive.rs:289-299
  let has_prefix = bytes.len() >= 9
      && bytes[..8].eq_ignore_ascii_case(b"textures")
      && (bytes[8] == b'\\' || bytes[8] == b'/');
  if has_prefix { after_data } else { Cow::Owned(format!("textures\\{}", after_data)) }
  ```

  **Every vanilla `TREE.ICON` is a bare filename with no directory
  component**, so this produces `textures\<Name>.dds` — a path that does not
  exist in any shipped archive. `resolve_texture_view_with_clamp`
  (`byroredux/src/asset_provider/texture.rs:346-420`) has no alternate-path
  search: one `tex_provider.extract` miss and the material gets the magenta
  checker handle.
- **Evidence**:
  - **ICON census** (run this audit, over `FalloutNV.esm`, `Fallout3.esm`,
    `Oblivion.esm`): 90 unique `TREE.ICON` values, **0 of which contain a
    path separator**. Per-game counts are 3 / 9 / 81 — the FNV and FO3
    numbers match `crates/plugin/tests/parse_real_esm.rs:843-859` and
    `:1395-1416`'s own "vanilla FNV ships 3 TREE bases" / "vanilla FO3 ships
    9" assertions exactly, so the census is capturing the TREE set the
    engine's own parser sees. Samples:
    `WhiteOakLeaves01.dds`, `EuonymusBush01.dds`,
    `WastelandShrub01Foliage.dds` (FNV); `ElmLeaves01.dds`,
    `SugarMapleLeaves01.dds` (FO3); `DShrubLeaves01.dds`,
    `ShrubBoxwoodLeaves.dds`, `MTreeLeaves01.dds` (Oblivion).
  - **Where those files actually live** (direct BSA folder-record + file-record
    walk, this audit):
    | ICON value | Real archive path |
    |---|---|
    | `WhiteOakLeaves01.dds` | `textures\trees\leaves\whiteoakleaves01.dds` (`Fallout - Textures2.bsa`) |
    | `WastelandShrub01Foliage.dds` | `textures\trees\leaves\wastelandshrub01foliage.dds` |
    | `EuonymusBush01.dds` | `textures\trees\leaves\euonymusbush01.dds` **and** `textures\trees\billboards\euonymusbush01.dds` |
    | `ShrubBoxwoodLeaves.dds` | `textures\trees\leaves\shrubboxwoodleaves.dds` (`Oblivion - Textures - Compressed.bsa`) |
  - What the engine asks for instead: `textures\WhiteOakLeaves01.dds` — no
    such folder record exists in either archive.
  - No compensating logic anywhere: `grep -rn "trees"` across
    `byroredux/src/asset_provider/`, `references/import.rs` and
    `crates/spt/src/import/mod.rs` returns only prose comments, no path
    construction.
- **Impact**: **100 % of vanilla SpeedTree placeholders on all three
  supported `.spt` games render with the missing-texture checker instead of
  their leaf card.** This is the one thing the S1 placeholder exists to do;
  the geometry, sizing (#1001/#1002), Z-up→Y-up bounds (#995), `-Z` winding
  (#1000), billboard-on-mesh wiring (#3076) and wind response (#3190-#3195)
  are all correct and all invisible behind a checker quad. It also matches
  the project's documented "chrome / posterized ⇒ run `tex.missing` first"
  symptom, which means any exterior-tree visual complaint filed against
  lighting or the walker is likely to be this instead. No crash, no data
  loss, no GPU hazard — visual only, but total and systematic.
- **Related**: #1819 / SPT-NEW-05 (the *classifier* keyword collision on the
  same ICON strings — a different defect on the same field; note that
  finding's own evidence quotes `ShrubBoxwoodLeaves.dds` and
  `WhiteOakLeaves01.dds` as bare filenames without noticing they never
  resolve). #468 (the original `textures\` prefix fix in
  `normalize_texture_path`, which is the same shape of bug one directory
  level up). #997 (the ICON-wins-over-tag-4003 precedence this defeats).
- **Suggested Fix**: **Do not hardcode a prefix from this report's sample.**
  The corpus shows `textures\trees\leaves\` for every sampled ICON, but
  `EuonymusBush01.dds` also exists under `textures\trees\billboards\`, so a
  single blind prefix is not obviously the rule Bethesda's SpeedTree runtime
  used — settle it first against the GECK/UESP `TREE:ICON` field
  documentation, then encode the resolved rule *once*. The mechanically safe
  interim shape, which needs no format claim, is a candidate chain in the
  SpeedTree route only (never in `normalize_texture_path`, which is shared
  by every other consumer): probe `TextureProvider::has_texture` for the
  normalised path first, then a small ordered list of `textures\trees\…`
  candidates, and log a single warning naming the ICON when none hit. Pair
  it with a corpus test that asserts all 90 vanilla ICON values resolve to a
  real archive entry — that test is the actual regression guard, and it is
  cheap now that the census exists.

---

### SPT-2026-08-28-D2-01: the `[16, 8192]` billboard-size clamp is NaN-transparent, so a non-finite `BNAM` produces a NaN quad, NaN `LocalBound` and NaN BLAS vertices

- **Severity**: MEDIUM
- **Dimension**: Placeholder Fallback
- **Location**: `crates/spt/src/import/mod.rs:232-252` (`compute_billboard_size`),
  specifically `:238-242`; consumed at `:167` → `placeholder_billboard_mesh`
  (`:279-345`)
- **Status**: NEW
- **Description**: `compute_billboard_size` documents its clamp as the
  corrupt-input guard — *"All paths clamp to the `[16, 8192]` band so corrupt
  input can't produce a 1-pixel mosquito or a floor-to-skybox planet-sized
  billboard"* (`:228-230`). The BNAM tier is:

  ```rust
  // crates/spt/src/import/mod.rs:238-242
  if let Some((w, h)) = params.billboard_size {
      let width = w.abs().clamp(16.0, 8192.0);
      let height = h.abs().clamp(16.0, 8192.0);
      return (width, height);
  }
  ```

  `f32::clamp` is NaN-transparent: it returns `self` unchanged when `self` is
  NaN. Verified empirically for this audit (`rustc -O`, `f32::NAN.abs()
  .clamp(16.0, 8192.0)` → `NaN`, `is_nan() == true`). The two sibling tiers
  are both immune by construction — `params.bounds` reaches the cell route
  only as `i16`→`f32` from `ObjectBounds` (`references/import.rs:322-326`,
  never NaN), and the MODB tier is explicitly filtered
  (`.filter(|r| *r > 0.0)`, `:245`, which NaN fails). **BNAM is the one tier
  fed by a raw, unvalidated `f32` read off disk** — `parse_tree`'s
  `find_sub(subs, b"BNAM") … Some((r.f32().ok()?, r.f32().ok()?))`
  (`crates/plugin/src/esm/records/tree.rs:170-177`) does no finiteness check.

  A NaN width/height propagates straight into `placeholder_billboard_mesh`'s
  `positions`, into `local_bound_center`/`local_bound_radius`
  (`:339-340`), and from there into the ECS `LocalBound` insert
  (`byroredux/src/cell_loader/spawn/mesh_instance.rs:761-769`, which has no
  finiteness gate) and the batched GPU vertex upload. The `is_finite` guards
  that do exist in `spawn.rs` (`:112-126`, `:191-199`, `:240`) all belong to
  the packed-Havok proxy synthesiser and are never on this path.
- **Evidence**:
  - `crates/spt/src/import/mod.rs:238-242` (quoted above) vs. `:228-230`'s
    stated contract.
  - `crates/spt/src/import/mod.rs:245` — the MODB tier's `> 0.0` filter,
    which *does* reject NaN, demonstrating the pattern was applied
    inconsistently rather than deliberately omitted.
  - `crates/plugin/src/esm/records/tree.rs:170-177` — BNAM read with no
    finiteness check.
  - `crates/spt/src/import/mod.rs:633-645` — the existing guard
    `bnam_clamps_to_safe_band` covers negative (`-500.0`) and oversized
    (`50_000.0`) BNAM but not NaN, so the hole is untested.
  - Live `rustc` check of `f32::NAN.abs().clamp(16.0, 8192.0)` → `NaN`.
- **Impact**: A malformed BNAM yields four NaN vertex positions and a NaN
  bounding sphere. Downstream that is a NaN `WorldBound` (which
  `bounds.rs`'s parent-fold then propagates up the placement hierarchy),
  NaN frustum-cull comparisons, and NaN vertices in a static BLAS build —
  which is undefined behaviour on the Vulkan side, not merely a visual
  artifact. **Reachability is mod-content-only**: BNAM is consumed only when
  OBND is absent, and vanilla FO3/FNV ship OBND on 100 % of TREE records
  while Oblivion ships no BNAM at all. Rated MEDIUM on that basis (missing
  error handling on a recoverable path) rather than higher; escalate if the
  NaN is ever shown to reach an acceleration-structure build in practice.
- **Related**: #3194 — the *identical* NaN-transparency class in the other
  SpeedTree consumer (`apply_speedtree_wind`'s gust), which was filed and
  fixed with exactly the guard missing here
  (`billboard.rs:218`, `let gust = if gust.is_finite() { gust.max(0.0) } else { 0.0 };`).
  #1002 (the audit that added the BNAM tier and its clamp).
- **Suggested Fix**: Filter the BNAM tier the way the MODB tier already is —
  `params.billboard_size.filter(|(w, h)| w.is_finite() && h.is_finite())` —
  so a non-finite pair falls through to the next tier instead of poisoning
  the quad. Extend `bnam_clamps_to_safe_band` (or add a sibling) with a
  `(f32::NAN, f32::NAN)` case asserting the default 256 × 512 fallback.

---

### SPT-2026-08-28-D1-01: `is_plausible_spt_curve_string` accepts a zero-length candidate, leaving a 4-byte residue of #1822

- **Severity**: LOW
- **Dimension**: Walker Byte-Accounting
- **Location**: `crates/spt/src/parser.rs:171-176`, reached from the
  `MaybeStringElseBare` arm at `:120-134`
- **Status**: NEW (residual of #1822, not a regression of it)
- **Description**: #1822's fix gates the tag-13005 `String` arm on the peeked
  candidate bytes being printable-ASCII curve text:

  ```rust
  // crates/spt/src/parser.rs:171-176
  fn is_plausible_spt_curve_string(bytes: &[u8]) -> bool {
      bytes
          .iter()
          .all(|&b| matches!(b, 0x20..=0x7E | b'\t' | b'\n' | b'\r'))
  }
  ```

  `Iterator::all` is vacuously `true` on an empty slice, and
  `SptStream::peek_string_lp_bytes` (`stream.rs:112-131`) returns
  `Some(&[])` for a declared length of `0`. So a bare `13005` sitting
  immediately before a geometry tail whose leading `u32` is `0` still takes
  the `String` arm, consumes 4 bytes as an empty string, and shifts
  `tail_offset` 4 bytes past the true tail start — the exact failure mode
  #1822 was filed to close, for the one candidate length the printable-ASCII
  discriminator cannot discriminate. A leading `0` (a zero count or index) is
  a perfectly ordinary thing for a binary tail to begin with.
- **Evidence**:
  - `parser.rs:171-176` and `stream.rs:126-130` (`self.bytes.get(start..end)`
    with `end == start` yields `Some(&[])`).
  - The #1822 regression guard
    `tag_13005_before_geometry_tail_resolves_as_bare_not_swallowed_string`
    (`parser.rs:437-463`) deliberately uses a leading tail value of `8`, not
    `0`, so the empty-candidate arm is untested in either direction.
  - Not observed in the corpus: the 4 real bimodal-13005 files
    (`treems14canvasfreesu`, `shrubms14boxwood`, `treecottonwoodsu`,
    `treems14willowoakyoungsu`) all carry the 104-byte `BezierSpline` blob,
    and this cycle's corpus run reproduces their bail offsets exactly.
- **Impact**: Bounded and today theoretical — 4 bytes of `tail_offset` drift
  plus one spurious empty `SptValue::String` entry, on a file shape not
  present in the 133-file vanilla corpus. The geometry tail is not decoded in
  Phase 1, so nothing consumes the drifted offset yet; it would matter to the
  Phase 2 tail decoder. Filed so the hole is closed *before* a consumer
  exists rather than after.
- **Related**: #1822 (the fix this is the residue of), #999 (the original
  bimodal-13005 handling).
- **Suggested Fix**: **Which arm is correct for a zero-length candidate is a
  format question, not a code-style one, and this audit does not answer it** —
  `Bare` consumes 0 bytes and `String` consumes 4, so guessing wrong just
  moves the desync. Settle it by dumping the four known bimodal files (and
  any mod-content sample) with `cargo run -p byroredux-spt --features recon
  --example spt_dissect` to see whether a zero-length 13005 payload occurs at
  all, record the answer in `crates/spt/docs/format-notes.md`'s "tag 13005
  bimodal payload" section, and only then add the one-line guard
  (`!bytes.is_empty() &&`) or an explicit zero-length arm. Add a
  `tag_13005_before_zero_leading_tail` test either way, since the behaviour is
  currently unpinned.

---

### SPT-2026-08-28-D3-02: `placement_root_billboard` has no producer that can ever yield `Some`, and its docstring plus two consumer comments still describe the pre-#3076 root-billboard model

- **Severity**: LOW
- **Dimension**: TREE→Billboard Wiring
- **Location**: `byroredux/src/cell_loader/nif_import_registry.rs:148-155`
  (the field + docstring), `byroredux/src/cell_loader/spawn.rs:783-790`
  (the dead consumer), `byroredux/src/systems/billboard.rs:133-134`
  (the misattributed skip comment)
- **Status**: NEW
- **Description**: #3076 moved the SpeedTree billboard from the placement
  root onto the renderable mesh. `import_spt_scene` now builds its root with
  `placeholder_root_node(/* billboard */ false)`
  (`crates/spt/src/import/mod.rs:164`), so
  `references/import.rs:364-367`'s

  ```rust
  let placement_root_billboard = imported
      .nodes.first().and_then(|n| n.billboard_mode).map(BillboardMode::from_nif);
  ```

  is structurally always `None` — pinned by
  `import_tests.rs:68` (`assert_eq!(cached.placement_root_billboard, None)`).
  Every *other* `CachedNifImport` constructor hardcodes `None`
  (`references/import.rs:184`, `partial.rs:114`, `precombined.rs:786`).
  Consequently the field is `None` at every construction site in the
  codebase, and its only consumer —

  ```rust
  // byroredux/src/cell_loader/spawn.rs:788-790
  if let Some(mode) = cached.placement_root_billboard {
      world.insert(placement_root, Billboard::new(mode));
  }
  ```

  — is unreachable. Three pieces of documentation still describe the removed
  model:
  1. `nif_import_registry.rs:148-155`: *"`Some` for `NiBillboardNode`-rooted
     content and for SpeedTree `.spt` placeholders, which need the placement
     root to yaw-track the camera"* — false for `.spt` since #3076, and no
     NIF path ever sets it either (the same docstring's next sentence
     concedes this).
  2. `spawn.rs:783-787`: *"Without this insertion `.spt` REFRs render as
     static quads"* — the insertion no longer happens and `.spt` REFRs do not
     render as static quads, because `mesh_instance.rs:792-794` attaches the
     `Billboard` on the mesh.
  3. `systems/billboard.rs:133-134`, added by `8e97b4e5`: *"A `SpeedTreeWind`
     without `Billboard` has no orientation this system owns — **the
     placement root is exactly that**."* The placement root is *not* that.
     `grep -rn "SpeedTreeWind"` shows exactly three production insert sites
     (`mesh_instance.rs:799`, `nif_loader.rs:550`, `nif_loader.rs:1034`) and
     all three are on mesh entities that receive `Billboard` in the same
     statement group; no site attaches `SpeedTreeWind` to a placement root.
     The new test `parked_camera_wind_pass_skips_a_marked_entity_without_billboard`
     (`billboard.rs:515-566`) labels its synthetic entity "The placement
     root: SpeedTreeWind without Billboard" for a configuration nothing
     builds.
- **Evidence**: the four call sites and three comments quoted above;
  `crates/spt/src/import/mod.rs:164` and `:260` (`billboard_mode:
  billboard.then_some(…)` with `billboard == false`);
  `byroredux/src/cell_loader/references/import_tests.rs:68`.
- **Impact**: None at runtime — the guard in `billboard.rs` is correct
  defensive code whichever entity motivates it, and the `spawn.rs` branch is
  simply never taken. The cost is that three documents disagree with the
  code about which entity owns a `.spt` billboard, which is precisely the
  question #3076 and #2206 were filed to settle; the next contributor
  touching this path reads the *field's own docstring* (the most local, most
  specific source) and gets the pre-#3076 answer.
- **Related**: #3076 (moved the billboard to the mesh), #2206 (the per-mesh
  attach), #3192 / `8e97b4e5` (added the third stale comment), #3193 (the
  prior cycle's identical "no production entity is in this configuration"
  determination — still true).
- **Suggested Fix**: Either delete `placement_root_billboard` and its
  `spawn.rs` consumer outright (nothing can set it), or, if it is being kept
  as a seam for a future `NiBillboardNode`-rooted NIF producer, say exactly
  that in the docstring and note that no producer exists today. Reword
  `billboard.rs:133-134` to describe the guard as defensive rather than
  naming the placement root, and retitle the test entity in
  `parked_camera_wind_pass_skips_a_marked_entity_without_billboard`
  accordingly.

---

### SPT-2026-08-28-D5-01: tags `12002` / `12003` are the only `FixedBytes` dictionary entries with no corpus evidence recorded in `format-notes.md`

- **Severity**: LOW
- **Dimension**: Tag Dictionary
- **Location**: `crates/spt/src/tag.rs:128-131`, vs.
  `crates/spt/docs/format-notes.md:403-413`
- **Status**: NEW
- **Description**: Every other fixed-size dictionary entry carries a recorded
  corpus observation with a confidence figure in the format-notes table —
  `8003`/`8005`/`8009` at `format-notes.md:405` ("fixed 52-byte payloads,
  100 % confidence"), `13008` at `:412` ("modal 11-byte payload"), `13013` at
  `:413` ("modal 7-byte payload"). The two `12xxx` entries do not:

  ```rust
  // crates/spt/src/tag.rs:128-131
  // 16 bytes — tag 12002 (4 × f32 = matrix row?).
  12002 => SptTagKind::FixedBytes(16),
  // 20 bytes — tag 12003.
  12003 => SptTagKind::FixedBytes(20),
  ```

  `grep -n "1200[0-9]" crates/spt/docs/format-notes.md` returns exactly one
  line — `:360`, which lists `12000` and `12001` among the **bare** markers.
  `12002` and `12003` appear nowhere in the observation log: no histogram, no
  confidence, no sample offset. The `(4 × f32 = matrix row?)` gloss is an
  unsupported interpretation sitting in the same comment as the load-bearing
  size.
- **Evidence**: the grep result above; `format-notes.md:342-414` (the
  "Recovered tag → payload-size table" and its `#### 52-byte fixed payload` /
  `#### Other notable tags` subsections, where every other `FixedBytes` entry
  is justified); `tag.rs:207-208` (the unit test pins the sizes but, being
  derived from the same source, cannot corroborate them).
- **Impact**: Documentation/evidence gap, not a demonstrated defect — the
  corpus gate passes at 100 % / 100 % / 96.46 %, so if either size were wrong
  in a way that desynced the walker on vanilla content it would almost
  certainly already show as an extra unknown-tag bail. But the gate only
  counts `Unknown` bails; a wrong-but-plausible size that happens to land on
  another valid tag would pass it silently, and a wrong fixed size is exactly
  the Dimension-1 desync trigger this dimension exists to spot-check. Under
  the project's No-Guessing policy, two dictionary entries whose derivation
  cannot be reconstructed are a liability for the Phase 2 tail decoder that
  will have to trust them.
- **Related**: #1821 (the earlier format-notes byte-alignment correction),
  the `format-notes.md` 2026-05-09 dictionary table this omission sits in.
- **Suggested Fix**: Re-run `cargo run -p byroredux-spt --features recon
  --example spt_tagmap` (and `spt_transitions`) over the three BSAs and add a
  `12002` / `12003` row to `format-notes.md`'s payload-size table with the
  observed histogram and confidence, exactly as the `8003` / `13008` /
  `13013` rows have. If the histogram does not support a single fixed size,
  demote the entries to `Unknown` — a clean walker bail is the contract the
  placeholder relies on, and is strictly safer than a size the log cannot
  justify. Drop or evidence the `matrix row?` gloss either way.

---

## Dimension summary (every dimension enumerated)

| Dimension | Findings | Verdict / basis this cycle |
|---|---:|---|
| 1 — Walker Byte-Accounting | **1** (LOW) | `parser.rs`/`stream.rs` unchanged since the #1822 fix; re-read both in full. Each `SptTagKind` decode advances exactly its claimed width (`Bare` 0, `U8` 1, `U32` 4, `Vec3` 12, `FixedBytes(n)` n, `String` 4+len, `ArrayBytes` 4+count×stride) — cross-checked arm-by-arm against `dispatch_tag`. The 64 KiB caps are on **byte count** in both places (`read_string_lp`'s `len`, and `ArrayBytes`' `count as u64 * stride as u64` **before** the allocation, `parser.rs:196-208`) — correct. `parse_spt` still returns `Err` on exactly the two fatal conditions; in-range unknown tags stay non-fatal. Readers are unconditionally LE, no host-endian or big-endian path. `peek_u32_le`/`peek_string_lp_bytes` both `remaining() < 4` guard and never consume. The one finding is the empty-candidate sliver above. |
| 2 — Placeholder Fallback | **1** (MEDIUM) | `import_spt_scene` still has **no** `Err` path (single node, single mesh, unconditionally). Size precedence is OBND → BNAM → MODB → 256 × 512 with `[16, 8192]` clamps and `#3080`'s docstring now matches. `bs_bound` Z-up→Y-up via `byroredux_core::math::coord::zup_to_yup_pos` with half-extents reshuffled `(hx, hz, hy)` — unchanged. Normals `-Z`, indices `[0, 3, 2, 2, 1, 0]`, and `compute_billboard_rotation`'s `BsRotateAboutUp` still the documented world-up yaw-lock approximation (`billboard.rs:283-295`), matching the arc `placeholder_billboard_mesh` documents. Cutout fields (`alpha_test`, `0.5`, func `6`, `two_sided`, `has_alpha: false`) unchanged. The finding is the NaN hole in the clamp. |
| 3 — TREE→Billboard Wiring | **2** (1 HIGH, 1 LOW) | The `.spt` dispatch, `CachedNifImport` synthetic defaults (`bsx_flags: 0`, `root_flags: 0`, `flame_attach_offset: None`, `attach_points: None`, `furniture: None`), mixed `.nif`/`.spt` coexistence, and the shared `extract_mesh` lookup chain are all intact — `.spt` bytes go through the same `GameArchive` path as NIFs, no parallel resolver. `TreeRecord` capture is lossless and still shape-tolerant across the 5-float Oblivion / 8-float FO3-FNV CNAM split (`tree.rs:161-168`, `while let Ok(v) = r.f32()`), with `corrupt_snam_truncated_chunk_drops_silently` and `parse_oblivion_short_cnam_no_bnam_no_pfig` pinning both. The HIGH is the ICON resolution failure; the LOW is the dead `placement_root_billboard` seam. |
| 4 — Per-Game Variants & Route Divergence | **0** | `version.rs` untouched; `detect_variant` still log-only with zero downstream branching (`references/import.rs:280-287` is its sole production call, deliberately diagnostic per #1820), and `MAGIC_HEAD` still rejects a one-byte flip and any input under 20 bytes. Both routes call `parse_spt` + `import_spt_scene` identically and both now degrade a parse error to the placeholder (#3195). Checked the two routes' `is_spt` predicates for case divergence — `nif_loader.rs:209-213` uses `Path::extension().eq_ignore_ascii_case("spt")` while `:414` uses `cache_key.ends_with(".spt")` on an **already-lowercased** `cache_key` (`:413`), so `.SPT` content routes identically through both; not a finding. The documented `SptImportParams::default()` gap on the loose route stands and is understood. Note the HIGH above is *worse* on the loose route (no ICON at all) but its root cause is the cell route's, so it is filed once under Dimension 3. |
| 5 — Tag Dictionary | **1** (LOW) | `tag.rs` untouched since 2026-06-09. Spot-checked `8003`/`8005`/`8009` = 52 B, `13008` = 11 B, `13013` = 7 B, `10002` stride 1, `10003` stride 8 against `format-notes.md:403-413` and `:501-524` — all corroborated. Confounders `4096`, `5376`, `11776`, `13568`, `100`, `110` all still `Unknown` (`unknown_for_out_of_dictionary_tags`). The four Oblivion outliers' `tag=768` bail is the known, documented, ≥95 %-gate-passing case. The finding is the two undocumented `12xxx` sizes. |
| 6 — NIFAL Material Translation | **0** | Single boundary holds: both routes reach `translate_material` through `spawn.rs` / `nif_loader.rs`, with no parallel "spt material" path and no BGSM/BGEM resolve for `.spt`. `placeholder_billboard_mesh`'s `metalness_override: Some(0.0)` / `roughness_override: Some(0.85)` survive `Material::resolve_pbr` unchanged (`material.rs:1054-1080` only fills NaN slots, then clamps — 0.0 and 0.85 are both in-band), pinned by `placeholder_billboard_sets_foliage_pbr_overrides_regardless_of_texture_path`. `is_pbr`/`from_bgsm` stay false; `emissive_source` stays `None`; the two-sided alpha-test cutout maps through intact. One candidate raised and **disproved by census** — see below. |

**Totals**: 6 dimensions, **5 findings** — 0 CRITICAL, 1 HIGH, 1 MEDIUM,
3 LOW.

---

## Candidates raised and disproved (not reported)

1. **"The `#3192` split orphaned the parked-camera refresh for ordinary
   billboards, or reopened #1374."** Disproved — see **Primary check**. Both
   arms share one body, the system-level early-out is untouched, and the new
   arm strictly reduces the entities visited on a parked frame. The
   sentinel-update ordering was checked specifically because the commit added
   two new early `return`s: `last_cam` is written at `:95`, before both.
2. **"`classify_glass_into_material` can still promote a SpeedTree
   placeholder to `MATERIAL_KIND_GLASS`, because #1819's
   `metalness_override: Some(0.0)` sits *below* the classifier's `metalness
   >= 0.3` conductor short-circuit (`byroredux/src/helpers.rs:128-131`) and
   the placeholder's `alpha_test: true` satisfies its
   `has_transparent_coverage` gate."** The code premise is correct — the
   glass arm *is* reachable for `.spt` placeholders, and #1819's fix does not
   cover it. But the gate that actually fires is
   `is_glass_keyword_path` (`crates/core/src/ecs/components/material.rs:780-786`:
   `glass`/`crystal`/`window`/`bottle`/`jar`/`vial`, the hardened
   `path_indicates_ice`, and word-boundary `gem`), and this audit's 90-value
   ICON census contains **zero** matches for any of them, including zero
   matches for the hardened ice heuristic (the `ShrubGenericElderberry…`
   cross-word `ice` that #1819's report cited no longer matches, because
   `path_indicates_ice` now requires component-initial position or a
   following ice noun). No vanilla `.spt` tree can reach the glass arm on any
   of the three games. Recorded here rather than filed, since the exposure is
   mod-content-only and purely theoretical against every shipped corpus.
3. **"`TreeRecord::has_speedtree_binary()` — documented as *the* `.spt`
   predicate — has no production caller; both dispatch sites re-implement it
   inline."** Verified true (`grep` shows only `tree.rs`'s own tests and
   `crates/plugin/tests/parse_real_esm.rs:852,1406,1572`), and
   `synth_child.rs:512-516` duplicates the method's exact expression
   character for character. But the duplication is forced by ordering, not
   sloppiness: `is_spt` must be decided from `model_path` *before*
   `record_index.trees.get(&child_form_id)` is consulted (`:521-523`), and no
   `TreeRecord` is in hand at that point. Not a defect; noted for the next
   cycle's diff in case a refactor makes the record available earlier.
4. **"A billboard that spawns while the camera is parked never gets its
   camera-facing rotation."** True, and true of the `.spt` placeholder — but
   it is a property of #1374's camera-motion gate, present in every revision
   since, not something `8e97b4e5` introduced, and cell streaming implies
   camera motion in practice. Out of scope for this subsystem; not filed.
5. **"`#1822`'s printable-ASCII gate may have changed the walker's stopping
   point on the four real bimodal files."** Disproved by the corpus run:
   `20425` entries and bail offsets `6211 / 4507 / 5641 / 5946` are
   byte-identical to the 2026-05-13, 2026-06-23, 2026-07-01, 2026-07-02 and
   2026-07-03 runs recorded in this directory.

---

## Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 1 |
| LOW | 3 |
| **Total** | **5** |

Both findings from the 2026-08-24 cycle are closed (#3275, #3276), as are
#3080, #3123, #3190, #3192, #3193, #3194 and #3195 — #3191 alone remains open
on the tracker while being verifiably fixed in code, and should be closed.

The headline result is a defect that eleven prior cycles of this audit
walked past: the parser half of this subsystem is in excellent shape — three
consecutive cycles with no walker defect, a stable ≥ 95 % corpus gate, and
`#1822` verified non-regressive against byte-identical bail offsets — and the
importer half is correct in every dimension the prior cycles examined
(sizing, bounds, winding, PBR, billboard ownership, wind response). But the
one thing the placeholder exists to show, the leaf card, has never rendered:
`TREE.ICON` is a bare filename on 90 out of 90 vanilla records across three
games, the texture normaliser only prepends `textures\`, and the files
actually live under `textures\trees\leaves\`. Every vanilla SpeedTree
billboard in the engine today is a magenta checker quad.

That finding also carries a methodological note worth keeping: it was found
by censusing the real data (ESM `ICON` values, then BSA folder records)
rather than by reading the code path again, and the code path reads
perfectly correct in isolation at every step. The remaining four findings are
smaller — a NaN hole in a clamp that documents itself as the corrupt-input
guard, a 4-byte residue of #1822 that needs a format answer before it gets a
fix, a dead `placement_root_billboard` seam with three stale comments
pointing at it, and two dictionary sizes with no recorded derivation.

### Suggested next step

```
/audit-publish docs/audits/AUDIT_SPEEDTREE_2026-08-28.md
```

Domain labels: `speedtree` + `terrain-exterior`; add `import-pipeline` for
SPT-2026-08-28-D3-01 (the archive-lookup half), `tech-debt` for
SPT-2026-08-28-D3-02 and SPT-2026-08-28-D5-01, and
`game:fnv` + `game:fo3` + `game:oblivion` on SPT-2026-08-28-D3-01 since it is
confirmed on all three corpora. Recommend the publish pass also close #3191.

TALLY: CRITICAL=0 HIGH=1 MEDIUM=1 LOW=3
