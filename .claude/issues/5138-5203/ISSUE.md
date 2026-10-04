# Issues 5138–5203 — doc-rot batch (AUDIT_SPEEDTREE/TOOLING/EXTERIOR/RENDERER 2026-09-29..10-03)

## #5138
[OPEN] SPT-2026-09-29-D1-01: #4122 left the spt crate's docs asserting the desync it fixed — `SptScene::tail_offset` still says 46 % of files stop mid-payload, `tag.rs` says 13013 is 7 bytes, 12002/12003 still "unevidenced"

**Source report**: `docs/audits/AUDIT_SPEEDTREE_2026-09-29.md` (report ID `SPT-D1-01`)
**Severity**: LOW
**Dimension**: Walker Byte-Accounting (also Tag Dictionary)

## Location
- `crates/spt/src/scene.rs` (`SptScene::tail_offset` rustdoc)
- `crates/spt/src/tag.rs` (`SptTagKind::FixedBytes` doc; 12002/12003 arms)
- `crates/spt/docs/format-notes.md` (12002/12003 caveat)
- `crates/spt/examples/spt_tail.rs`
- `crates/spt/src/stream.rs` (module doc)

## Description
`9fcbee478` fixed the three mis-sized entries and updated the dispatch arms, but several claims were not updated:
- The public rustdoc on `SptScene::tail_offset` still lists "In 46 % of files the resync needs a 1-3 byte shift, meaning the walker stopped *inside* a payload it mis-sized" and closes with "Treat it as 'where parsing gave up'". The gate #4122 added now measures 0/159 shifted and 159/159 stopping on a 14 000-band tail tag, so `tail_offset` is the true TLV boundary at `TAG_MAX`.
- The `SptTagKind::FixedBytes` doc gives "tag `13013` = 7 bytes"; the value is 4.
- The 12002 and 12003 arms still say "Size only … no recorded corpus evidence"; `format-notes.md` repeats it, while the same file's 2026-09-24 entry records that both "decode cleanly to their next tags".
- `spt_tail.rs` describes the 46 % stop in the present tense.
- Separately (predating the baseline), the `stream.rs` module doc says errors surface as `Err(SptParseError::Truncated)`. No such type exists; the parser returns `io::Error`.

## Evidence
Corpus run this audit: `[SI] 159 files | 159 on boundary | 159 shift-0`. `grep -rn SptParseError crates/spt` has exactly one hit, the doc line.

## Impact
A consumer reading the public API doc would treat `tail_offset` as untrustworthy, although it is now the exact precondition point #3808 named for raising `TAG_MAX`. The false 7-byte and "unevidenced" claims invite a re-litigation like the 2026-07-04 "768" dispute.

## Related
#4122, #3535 (12002/12003 evidence), #4120 (prior "sweep missed a file" doc-rot pattern).

## Suggested Fix
- Rewrite the `tail_offset` bullets to state the post-#4122 measurement: the stop is the true boundary, and tail tags start at 14 000.
- Change the `FixedBytes` example to 4 bytes.
- Replace the 12002/12003 caveats in `tag.rs` and `format-notes.md` with a pointer to the 2026-09-24 side confirmation.
- Mark the `spt_tail.rs` sentence as pre-#4122.
- Replace `SptParseError::Truncated` with `io::Error` (`UnexpectedEof`).

Validated at HEAD 9fcfdc3fc: `scene.rs` still carries the "46 %" and "where parsing gave up" lines; `tag.rs` doc still says `13013` = 7 bytes while dispatch is `FixedBytes(4)`; `SptParseError` appears only in the `stream.rs` doc.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other docs in crates/spt referencing pre-#4122 behaviour)


## #5150
[OPEN] TOOL-D2-2026-09-29-01: `debug-cli.md` residual drift after the `63c0aee3b` reconcile — base64 screenshots, the `/tmp` tex.dump default, and the "5 s" timeout

**Source report**: `docs/audits/AUDIT_TOOLING_2026-09-29.md`
**Severity**: LOW
**Dimension**: Protocol & Registry (exposure: developers)

## Location
- `docs/engine/debug-cli.md` (`Screenshot { path? }` protocol row; `tex.dump` row; the "per-client thread's 5 s `recv_timeout`" note; "Last reconciled 2026-08-25" stamp)
- `crates/debug-protocol/src/lib.rs` (`DebugResponse::Screenshot`)
- `tools/byro-dbg/src/display.rs`, `tools/byro-dbg/src/tui.rs` (still match the dead variant)

## Description
1. `Screenshot { path? }` "…else returns base64 PNG". The server never does this: with `path: None` it writes `screenshot_<secs>.png` and answers `ScreenshotSaved`. `DebugResponse::Screenshot` is never constructed anywhere — a dead variant that byro-dbg still matches.
2. `tex.dump` "default `/tmp/tex_dump.png`". It is now `texture-dumps/tex_dump.png` and needs a startup-configured archive, which contradicts the file's own header.
3. "The per-client thread's 5 s `recv_timeout`". It is 30 s (`dd99cd0f3`), and the new 10 s client timeout (TOOL-D1-01) is not documented.
4. "Last reconciled 2026-08-25".

## Evidence
The doc lines quoted above are present at HEAD; `DebugResponse::Screenshot` has only match-arm references (byro-dbg `tui.rs`/`display.rs`), no constructor.

## Impact
Doc rot only. Readers get the wrong behaviour for two commands.

## Related
#4756 (open; its scope — counts and missing rows — is fixed; this is the residual content drift), TOOL-D1-2026-09-29-01, #4374 (removed `ListLoadedAssets` for the same dead-variant reason). Label gap: debug server / byro-dbg have no own label → `tech-debt`.

## Suggested Fix
Correct the three statements and bump the reconcile stamp. Then either implement the documented base64 return for `path: None` or delete the `Screenshot` response variant.

Validated at HEAD 9fcfdc3fc: debug-cli.md still carries the base64, `/tmp/tex_dump.png`, 5 s and 2026-08-25 lines; no `DebugResponse::Screenshot` constructor exists.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other debug-cli.md rows touched by 63c0aee3b)


## #5172
[OPEN] EXT-D2-2026-10-02-01: `GpuTerrainTile` grew from 160 to 176 B (#4903), but six docs and comments still say 160 B and one cites a test that no longer exists

- **Severity**: LOW (doc rot)
- **Dimension**: Terrain, splatting
- **Location**:
  - `crates/renderer/shaders/include/terrain_sample.glsl:20`. #4918 rewrote this exact comment to "160 B" on 09-30, and #4903 made it stale the next day.
  - `docs/engine/memory-budget.md:161`: row says "160 B … pinned by `gpu_terrain_tile_is_160_bytes`", "~160 KB". The test is now `gpu_terrain_tile_is_176_bytes`; the real size is 176 B and ~176 KB, and the row omits `base_cover_affinity` and `base_diffuse_index`.
  - `crates/renderer/src/vulkan/scene_buffer/constants.rs:260` ("1024 × 160 B = 160 KB").
  - `crates/renderer/src/vulkan/scene_buffer/buffers.rs:552`.
  - `crates/renderer/src/vulkan/context/shrink_frame_scratch.rs:112` (rationale string).
  - Skills: `.claude/commands/audit-exterior/SKILL.md:42`, `.claude/commands/audit-safety/SKILL.md:188`.
- **Status**: Regression of #4918 at the `terrain_sample.glsl` site; the other sites are NEW.
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Description**: The layout pin `gpu_terrain_tile_is_176_bytes` is correct. No guard scans the memory-budget terrain row, which is how it drifted.
- **Suggested Fix**:
  - Update each site to 176 B and name the live test.
  - Optionally extend the `memory_budget_ledgers…` guard to cover the terrain-tile row (`size_of::<GpuTerrainTile>()`).

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Every other copy of the stale text (specs, code comments, skill files) updated in the same change
- [ ] **TESTS**: If a source-scan or doc-sync test can pin the corrected statement, add it

## #5177
[OPEN] EXT-D3-2026-10-02-05: Doc rot from #4903 / #4907 / #4906 (bundle)

- **Severity**: LOW (doc rot / test gap)
- **Dimension**: Ground-cover pipeline (plus one Dim 2 test comment)
- **Location**: see each item
- **Status**: NEW
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Description**:
  1. **`docs/engine/exal-groundcover.md`, three stale sections.** None of the three fix commits touched the spec doc.
     - §3 (`:151-153`) still describes `affinity(splat)` as the 8 weights "dotted with" `cover_affinity`. It is now an ordered mix from the BTXT base.
     - §12.3 (`:1195`) still says the blade blends toward "the splat-weighted average of the cell's painted layer diffuse textures". It now uses BTXT base plus `byroTerrainSplatAlbedo`.
     - §12.12 (`:1624-1626`) states accept probability `density × …` and pure climate selection. It omits the `density/share` bake and the placeable-only weighting.
  2. **`groundcover_blade.frag:136-147`** still says the blend is "a weighted average … not `mix` against a base texture: the blade's base has no BTXT base layer of its own". That is the premise #4907 removed, and the code directly below contradicts it.
  3. **Three doc comments repeat the false premise #4903 removed**: `DEFAULT_COVER_AFFINITY` (`crates/core/src/ecs/components/groundcover.rs:107-113`), `groundcover_translate.rs:71-73`, and `shader_constants_data.rs:232-236`.
     - They say the scatter shader uses the default for unpainted ground because the base "has no LTEX record".
     - `GROUNDCOVER_DEFAULT_AFFINITY` is still emitted to GLSL but no shader reads it.
  4. **Moot rationale.** `terrain.rs:1084-1088` and `components.rs:471-473` say unused slots must hold the default "or an unused layer reads as a vegetation hole". Under the ordered mix a zero-weight lane cannot affect the result.
  5. **Weak #4903 guard.** The "host-side half" of `groundcover_affinity_composes_the_base_in_diffuse_loop_order` exercises a closure defined inside the test, not the shader. The source half counts 8 `mix` calls but not lane pairing: `affinity0.y` paired with `splat0.x` would pass.
  6. **Wrong direction in a #4905 test comment.** `terrain_splat_tests.rs:190` labels SW row 0 "top edge (faces the cell above)". Row 0 is the south border: row 16 is north, as the same test's `:193` says. The assertion itself is correct.
- **Suggested Fix**:
  - Rewrite the cited text.
  - Drop the dead GLSL constant export, or document it as Rust-only.
  - Pin lane pairing in the #4903 guard.

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Every other copy of the stale text (specs, code comments, skill files) updated in the same change
- [ ] **TESTS**: If a source-scan or doc-sync test can pin the corrected statement, add it

## #5181
[OPEN] EXT-D4-2026-10-02-04: Stale pointers in sky/weather docs after #5087 and #4925, plus a misplaced rustdoc block in `weather.rs`

- **Severity**: LOW (doc rot)
- **Dimension**: Sky, weather, sun
- **Location**:
  - `docs/engine/skyal.md:555`: names `context/draw.rs` for `interior_portal_sky_preserves_room_weather_gate`. It is now `crates/renderer/src/vulkan/context/frame_params.rs:1201-1202`; the doc landed in e4df3abb9 at 09:14 on 2026-10-01, and c57e5cc4a moved the test the same day.
  - `crates/renderer/shaders/include/clouds.glsl:317`: "The host packs `[dir.x, speed, dir.z, 0]` (`build_composite_params`)". The packer is `pack_sky_dome`, at `frame_params.rs:1065-1070` since #4925.
  - `crates/renderer/shaders/composite.frag:575`: `draw.rs::hdr_clear`. It lives in `context/begin_frame_recording.rs:125`. This is older than this pass.
  - `byroredux/src/systems/weather.rs:83-119`: the rustdoc blocks for `pick_tod_pair` (`:83-91`) and `compute_sun_arc` (`:92-110`) sit above `SUN_SOUTH_TILT` (`:111-120`). Rustdoc attaches all three to the const, and `compute_sun_arc` (`:122`) and `pick_tod_pair` (`:161`) render undocumented. This has been the case since 2026-06-02.
- **Status**: NEW
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Description / Impact**:
  - Readers following skyal.md's gate table, or the GLSL wind-packing comment, land in a file that no longer holds the code.
  - The wind-packing comment matters most: `wind_consumers_match_the_host_packing` exists because this exact swizzle was misread once already.
  - The two TOD functions behind the sun arc and palette have no rendered docs.
- **Suggested Fix**:
  - Repoint the three references: `frame_params.rs`; `pack_sky_dome`; `begin_frame_recording.rs`.
  - In `weather.rs`, move the "Walk a `build_tod_keys` table…" block to directly above `pick_tod_pair`, and the "Derive sun direction…" block to directly above `compute_sun_arc`.

---

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Every other copy of the stale text (specs, code comments, skill files) updated in the same change
- [ ] **TESTS**: If a source-scan or doc-sync test can pin the corrected statement, add it

## #5182
[OPEN] EXT-D5-2026-10-02-02: The Oblivion row of the #4910 table settles a frame without evidence; by the doc's own convention the un-rotated read mirrors north and south

- **Severity**: LOW
- **Dimension**: Water translation (WATAL)
- **Location**:
  - `docs/engine/watal.md:481` (the Oblivion row: "**un-rotated**", not OPEN).
  - `byroredux/src/env_translate.rs:592-595` and `:608`.
  - The test comment at `env_translate.rs:3230-3232`.
  - The parser derives the angle at `crates/plugin/src/esm/records/misc/water.rs:600-606`.
- **Status**: NEW. Related to #5136 and #4910.
- **Tier Violated**: no-fabrication
- **Game Affected**: Oblivion
- **Description**:
  - The parser stores Oblivion's layer 0 as `atan2(y, x)` of the `DATA[28]/[32]` scroll pair, and the docs call it a counter-clockwise direction-of-travel angle in the record frame. The un-rotated read then places `(cos θ, sin θ)` straight into engine XZ.
  - The same doc's Z-up→Y-up map (`watal.md:405-406`: "(x, y, z) maps to Y-up (x, z, −y)", "+Z is game south") sends a record-frame pair (x, y) to engine (x, −y), so φ = −θ.
  - So if the pair is a world-frame Z-up vector, the un-rotated read is a north/south mirror. If it is a UV-space vector, its relation to world depends on Oblivion's water UV mapping, which nobody has established.
  - Either way, "the angle is not a bearing" only shows that the +90° bearing formula does not apply. It does not show that φ = θ.
  - The commit message itself says the un-rotated read is "the pre-2026-09-24 status quo, not a frame claim". The table nevertheless lists FO3/FNV and Starfield as OPEN and Oblivion as settled.
  - #4910 also silently changed the frame of Oblivion's `wind_direction` (`DATA[4]`, authored per #4931), which feeds the physics-current fallback at `env_translate.rs:1001-1003`. It went from +90° to un-rotated, and neither table mentions it.
- **Evidence**: Census of `Oblivion.esm`, 23 WATR:
  - The scroll pairs are `(0.0011, 0.0011)` (DefaultWater family, θ = 45°), `(0.001, 0.002)` (dungeon/sewer, θ = 63°), `(0.0008, 0.0008)` (SwampWater), and zero on the rest.
  - Read as world-frame vectors, they would drift NE/NNE in game terms. The un-rotated read drifts them SE/SSE.
  - No Oblivion WATR has a directional kind: no river/stream/creek/rapid name, no NAM5, no NAM0. So the `wind_direction` physics-fallback change is latent in vanilla.
- **Impact**: The pattern drift direction on every Oblivion water may be mirrored, though the visual effect on near-isotropic slow scrolls is small. The spec presents an unverified frame as resolved, which is the same defect class as #5136.
- **Suggested Fix**: Mark the Oblivion row "un-rotated, OPEN", as FO3/FNV are. State that a world-frame reading would need φ = −θ, and add a row for Oblivion's `wind_direction`. Settle the question with a capture, or from Oblivion's water shader UV convention, before choosing a frame.

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Every other copy of the stale text (specs, code comments, skill files) updated in the same change
- [ ] **TESTS**: If a source-scan or doc-sync test can pin the corrected statement, add it

## #5185
[OPEN] EXT-D5-2026-10-02-05: watal.md §2 still says Starfield pigment concentrations are normalized in the shader, and records neither #5151's metric lift nor the open unit of the noise-UV tile

- **Severity**: LOW
- **Dimension**: Water translation (WATAL)
- **Location**: `docs/engine/watal.md:340-343`; the code is at `byroredux/src/env_translate.rs:905-917` and `crates/plugin/src/esm/records/spatial_units.rs:151-158`.
- **Status**: NEW. Stale since #4285 (closed); the missing record dates from #5151 (closed).
- **Tier Violated**: no-fabrication (documentation)
- **Game Affected**: Starfield
- **Description**:
  - The §2 text reads: "Its authored pigment concentrations remain in their vanilla 0..20 range and are normalized in the shader against the shared `STARFIELD_WATER_CONCENTRATION_REFERENCE`".
  - Since #4285 the ÷20 runs at the WATAL translate (`env_translate.rs:905-913`, comment "normalized … HERE, at the WATAL translate boundary, not in `water.frag`"). The shader only clamps (`water.frag:563`).
  - §2 has no mention of #5151 (Starfield DNAM metres → BU: depth, underwater fog near/far and noise falloff ×70, absorption ÷70).
  - The open item #5151 left behind is recorded only in a code comment (`spatial_units.rs:157-158`): the noise UV tile sizes at DNAM 120/124/128 "stay unlifted until a capture settles their unit". The skill's promotion rule wants open items in watal.md §2.
- **Impact**: The spec places a per-game unit conversion at render time, which is the defect #4285 fixed. A reader cannot find out from WATAL that the Starfield noise tile unit is unresolved.
- **Suggested Fix**:
  - Rewrite `watal.md:340-343` to say the normalization happens at the translate.
  - Add a #5151 sentence on the metric lift and its boundary (`spatial_units::normalize`, Starfield-gated, FO76 untouched).
  - List "Starfield noise-UV tile unit (DNAM 120/124/128)" as open in §2.

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Every other copy of the stale text (specs, code comments, skill files) updated in the same change
- [ ] **TESTS**: If a source-scan or doc-sync test can pin the corrected statement, add it

## #5186
[OPEN] EXT-D6-2026-10-02-02: The `.btt` recon in object_lod.rs misreads the header, and its claim that `.lst` is "not needed" is false: the billboard size and atlas UV rect live only in the `.lst`

- **Severity**: LOW
- **Dimension**: Distant LOD and trees
- **Location**: `byroredux/src/cell_loader/object_lod.rs:765-773` (the doc on `tree_lod_supported`); the same claim is in the 8e512b02e commit message and the #4913 comment.
- **Status**: NEW
- **Tier Violated**: no-fabrication
- **Game Affected**: Skyrim
- **Description**:
  - The comment reads the header as "three u32s (`15, 9, 21` — version, counts?)". It concludes that "the `.lst` lists are generation-time species data and not needed to consume the baked `.btt`".
  - The census shows something different. A `.btt` is a u32 **group count**, then per group a u32 **tree-type index**, a u32 count, and that many 32-byte records. Each record is: f32 x, y, z (world, Z-up); f32 rotation (radians); f32 scale; u32 REFR FormID; two zero u32s.
  - For `tamriel.4.4.-12.btt`, 15/9/21 means 15 groups, then type 9 with 21 trees.
  - The type index keys the `.lst`: u32 count, then 32-byte entries of u32 index, f32 width, f32 height, f32 u_min, v_min, u_max, v_max, u32.
  - The `.lst` is therefore the only source of each billboard's world size and its rect in `<ws>treelod.dds`. A consumer cannot render a `.btt` without it.
  - `exal.md:347-352` already says this ("the 9 `.lst` tree species lists that key them", "a `.btt`+`.lst` … consumer"). The code comment contradicts the spec.
- **Evidence**: A Python parse of `Skyrim - Meshes1.bsa` (LZ4 v105):
  - 380 of 386 `.btt` parse to exactly their length under this layout. Every type index is below its worldspace's `.lst` entry count (tamriel 34, dlc2solstheimworld 36, sovngarde 15, …).
  - The 6 exceptions are `dlc2solstheimworld.4.{12,16}.{4,8,12}.btt`, which carry trailing bytes past the declared groups (e.g. 2,796 of 3,244 bytes). That tail is unexplained.
  - `tamriel.lst` entry 0: index 0, 566.4 × 1521.5 BU, UV (0.809, 0.002)–(0.895, 0.250).
- **Impact**: A future consumer that follows the in-code recon would skip the `.lst` and have no billboard dimensions or atlas coordinates.
  - Side note: each `.btt` record carries a REFR FormID, so the tree tier *does* have per-object ids. #3307's premise that "baked quads carry no per-object ids" holds for `.bto` but not for `.btt`. That matters when VWD culling is designed.
- **Suggested Fix**: Replace the recon paragraph with the verified layout of both files, record the 6 Solstheim files that carry trailing data as open, and drop the "not needed" sentence. Note the per-tree REFR FormID for #3307.

---

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Every other copy of the stale text (specs, code comments, skill files) updated in the same change
- [ ] **TESTS**: If a source-scan or doc-sync test can pin the corrected statement, add it

## #5202
[OPEN] REN-D1-2026-10-03-04: `docs/engine/renderer.md` describes the BLAS budget with the pre-#3839 formula

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (HEAD `f002763b4`)

- **Severity**: LOW (doc-rot)
- **Dimension**: AS Correctness (documentation)
- **Location**: `docs/engine/renderer.md`:
  - the feature bullet "LRU eviction (budget = `device_local / 3`, floored at 256 MB)";
  - the Acceleration Structures section's "**BLAS LRU eviction**: budget is `device_local / 3`, floored at `MIN_BLAS_BUDGET_BYTES = 256 MB`".
- **Status**: NEW. #3866 fixed four other sites of this exact staleness (`mod.rs`, `constants.rs`, `predicates_tests.rs`, `memory-budget.md`). `renderer.md` was not in its scope.
- **Description**: The real rule is `blas_budget_for_heap(heap, reserved) = ((heap − reserved) / 3).clamp(MIN_BLAS_BUDGET_BYTES, MAX_BLAS_BUDGET_BYTES)`.
  - `reserved = screen_scaled_reservation_bytes(extents, volumetrics, upscaler_sdk_bytes)`, which is re-derived on resize by `recompute_blas_budget_for_current_state`.
  - `MAX_BLAS_BUDGET_BYTES` is 1 GiB.
  - The doc omits the reservation, the 1 GiB ceiling and the runtime re-derivation. `memory-budget.md` already states the correct formula.
  - The same section also says static BLAS are "built once when the mesh is uploaded". In fact they are built by `build_blas_batched` at cell or NIF load and restored after eviction by `restore_missing_static_blas_for_draws`.
- **Impact**: An operator sizing VRAM from `renderer.md` overestimates the BLAS budget, by a factor of 4 on a 12 GB card at the ceiling.
- **Related**: #3866, #3839, #3988.
- **Suggested Fix**: Restate both sites against `blas_budget_for_heap` and link `memory-budget.md` §Acceleration Structures as the ledger.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix (or a doc-pin / `_audit-validate.sh` check where one fits)


## #5203
[OPEN] REN-D3-2026-10-03-01: #5055 shrank `GpuLight` to 64 B, but four texts still describe the 80 B identity-carrying struct, including the `NoUninit` SAFETY comment, which names a test that no longer exists

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: the stale texts are listed under Evidence.
- **Status**: NEW. This absorbs Dim 2's REN-D2-2026-10-03-02, which found the same `renderer.md` site; that ID is retired into this one. #5055 (CLOSED) is the change itself. #4952 (CLOSED) was the previous size-drift on the same struct. No open issue covers these sites.
- **Description**:
  - c705c310d moved the ReSTIR remap identity off the GPU struct and updated `gpu_types.rs`, the size pin (`gpu_light_is_80_bytes` → `gpu_light_is_64_bytes`), `shader-pipeline.md` and the `memory-budget.md` light row.
  - Four other texts still state the old layout:
    - Two are prose.
    - One is the SAFETY justification of an `unsafe impl`. It cites "80 B total", "one `[u32; 4]` identity" and the renamed test as the pin holding the layout fixed.
  - The invariant it relies on still holds: four `[f32; 4]`, no padding. The justification text and its named guard are wrong.
- **Evidence**:
  - `docs/engine/renderer.md` § Multi-light SSBO: "Each `GpuLight` is an 80-byte struct of four `vec4`s and a `uvec4` identity … `history_id` identifies the producer across light animation and priority reordering; zero declines selection reuse."
  - `MAX_LIGHTS` rustdoc in `crates/renderer/src/vulkan/scene_buffer/constants.rs`: "1023 lights × 80 bytes plus the 4112-byte remap header is about 84 KiB". The live figure is 1023 × 64 + 4112 = 69 584 B ≈ 68 KiB.
  - `hash_light_upload` rustdoc in `crates/renderer/src/vulkan/scene_buffer/descriptors.rs`: "`GpuLight` is `#[repr(C)]` with four `[f32; 4]` fields, one `[u32; 4]` identity, and no implicit padding."
  - SAFETY comment on `unsafe impl crate::vulkan::buffer::NoUninit for super::gpu_types::GpuLight` (same file): "four `[f32; 4]` fields and one `[u32; 4]` identity (16 B each, 80 B total) … `gpu_light_is_80_bytes` (in `gpu_instance_layout_tests.rs`) holds the layout fixed." `grep -rn "fn gpu_light_is_80_bytes" crates/` finds nothing.
- **Impact**: No runtime effect. A reviewer checking the `NoUninit` justification follows it to a test that does not exist. renderer.md tells readers the identity is GPU-resident, which is the design #5055 removed; its rustdoc says "Do not re-add identity data here".
- **Related**: #5172 (OPEN, the same class of drift for `GpuTerrainTile` 160 → 176 in the same window). Stale skill premises below (Dim 3 Guard line, Dim 10 `history_id` bullet).
- **Suggested Fix**:
  - Rewrite the four texts for the 64 B / four-`vec4` struct and name `gpu_light_is_64_bytes`.
  - In renderer.md, say the identity rides `FrameInputs.light_ids`, CPU-only.
  - The window produced two Gpu* size drifts and both left prose behind. Consider extending the `gpu_material_size_claims` scanner (which #4952 already suggested) to every struct with a size pin (`GpuLight`, `GpuTerrainTile`, `GpuInstance`, `GpuCamera`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix (or a doc-pin / `_audit-validate.sh` check where one fits)


