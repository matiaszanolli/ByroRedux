Found by the new P5 soak gate (`docs/smoke-tests/p5-soak.sh`, FNV GSProspectorSaloonInterior reference route) on 2026-10-01, first 30-minute run.

## Signature

Cycle 10's `input.press quickload` restored the body to **exactly** the quicksaved pose, but `grounded` stayed `false` for the full 300 s wait while `vertical_velocity` sat at the `-2000` terminal clamp and the position never moved:

```
# F5-time pose (grounded when saved), cycle 10:
body=(107.73, 3525.68, 856.52) grounded=true  vertical_velocity=0.00
# after F9 restore, final poll of 300 s:
body=(107.73, 3525.68, 856.52) grounded=false vertical_velocity=-2000.00
```

The restore itself completed (`save load: restored player pose at (107.7, 3525.7, 856.5)`), the engine kept ticking (scheduler logs continued), and the debug server kept answering `player.status` — so this is **not** a lost process or a stuck transition: the capsule rests at the restore pose but both ground-contact authorities (`result.grounded` from `move_character` and the `cast_capsule_down_surface_and_normal` walkable probe, `byroredux/src/systems/character.rs` ~L415-524) read no support.

## Repro rate

- 30-min soak run 1: FAILED at cycle 10 (`/tmp/byro-p5-soak.oNbNbm`, samples + full logs retained).
- 7-min soak rerun: PASS, 27 cycles incl. two ring wraps (`/tmp/byro-p5-soak.IKMQ1I`).
- Fast hammer (F5→activate→F9 without the walk segments, constant restore pose): 15/15 OK.

≈1 failure in ~52 restore cycles; the failing ingredient appears to need the per-cycle walk segments (each cycle restores to a *different* walk-back pose) or the slower cadence.

## Hypotheses (unchecked)

1. Session-replacement teardown leaves a stale/duplicate collider under the restored capsule; the KCC's excluded-collider handle and the probe disagree with the live set, so both miss the floor while depenetration pins the position.
2. The walk-back endpoint lands on marginal geometry (porch threshold/steps near the saloon door, z≈856); restoring the capsule half-embedded on a slope/step edge puts `result.grounded=false` and the walkable-normal probe under threshold simultaneously.
3. A race between the exterior streaming worker teardown and the interior rebuild's collider registration, hit only at the slower soak cadence.

## Evidence retained

`/tmp/byro-p5-soak.oNbNbm/`: `session.stderr` (10 restore lines, transition + loading-screen lifecycle), `c10.*` (F5 pose, arrival, restore poll), `soak.csv` (RSS curve — memory was bounded, ~2.6-2.8 GiB, so this is not the memory detector firing).

The soak gate now also captures per-second `player.status` samples and a `phys.stats`/`phys.census` pair on this specific failure, so a recurrence will carry the physics census that this first occurrence lacked.
User-reported 2026-10-01 while watching the retargeted p2 gate run in the FNV Prospector Saloon: "Hair is off."

## What it looks like

The NPC's hair mesh (GSSunnySmiles here — `Characters\Hair\HairBun.NIF` per the load log) renders as a detached mass floating **in front of / over the face** instead of sitting on the scalp. Head itself is bald; the hair geometry hangs ahead of the forehead. Same for other female patrons observed in the cell.

## Attribution — longstanding, not a recent regression

Camera-matched A/B at HEAD vs `cba78990e` (2026-09-30, the commit *before* the outdoor-sun-for-interiors change `387bb9d9c` and before the #5154 meter↔tonemapper compensation `4bf2ec3a4` — neither is an ancestor of the old build):

- HEAD: `/tmp/byro-hair/head_face.png`
- `cba78990e`: `/tmp/byro-hair/old_face.png`

Both frames show the identical detached-hair placement. Method: `cam.tp <entity>` + `input.look 0 -25` + `screenshot` in both builds (framing verified identical).

## Suspects

- `head_parts_use_head_bone_mount(GameKind::Fallout3NV)` — the FO3/FNV hair-part mount on the shared head-bone basis (`byroredux/src/npc_spawn/resumable/`): the mount basis or the hair NIF's local transform composes wrong, translating the part forward/down instead of onto the scalp.
- The FaceGen head (`HeadFemale.NIF`) and the hair part may disagree on head-bone bind pose — hair authored against the skeleton's head node while the FaceGen head is placed at the mesh's own origin.

Not yet checked whether the same defect shows on FO3 (same head-part mount family) or on male NPCs.
User-reported 2026-10-01 while watching the retargeted p2 gate run in the FNV Prospector Saloon: "interior brightness is overblown."

## What it looks like

The interior renders heavily overexposed: windows clip to pure white with no shape inside them, the floor and bar surfaces are bleached, mid-tones washed out. HUD bar text also renders doubled/offset in the same session (lower-left) — possibly a separate HUD-scale/draw bug, noted here only as an observation.

## Attribution — longstanding, not a recent regression

Camera-matched A/B at HEAD vs `cba78990e` (2026-09-30 — **before** both `387bb9d9c` "one canonical outdoor sky and sun for interiors" and `4bf2ec3a4` (#5154 meter↔tonemapper desaturation compensation); verified neither is an ancestor of the old build):

- HEAD: `/tmp/byro-hair/head_face.png` / `head_tp.png`
- `cba78990e`: `/tmp/byro-hair/old_face.png`

Both frames show the same clipped-window/bleached-floor character; the old build is, if anything, marginally brighter. So the current exposure pipeline did not introduce it.

## Where to look next

- Auto-exposure meter (EV100, `exposure_meter.comp` + adaptation): interiors with bright window portals may be metered off the window luminance, over-lifting the room.
- Interior ambient/sky term intensity for FNV-era cells (XCLL ambient color handling).
- A/B further back (pre-auto-exposure-default `a070baaad`) to bracket when it appeared, if a fix session wants the anchor.

Both screenshots retain the camera pose in their filename set; the pose is reproducible via `cam.tp 156` + `input.look 0 -25` on the FNV saloon fixture.
User-reported 2026-10-01 (crop attached to the session) while reviewing the FNV wasteland exterior at grid (-17,0): "look at the noise at the dirt stain."

## What it looks like

Dark dirt-stain patches on the terrain render with heavy clustered speckle — multi-pixel blobs of gray/white grain on near-black terrain, not uniform per-pixel salt-and-pepper. The blobby, clustered character is the classic signature of undersampled Monte-Carlo shading (ReSTIR-DI / path-traced GI variance) that the SVGF temporal filter is not smoothing, with the worst case exactly where relative variance is highest: low-luminance rough surfaces.

## Context from the same session

- Auto-exposure measured ~0.4-0.5 on this exterior (vs the old fixed default 0.85 — auto actually darkens exteriors), so the stains are rendered dim; dim surfaces show MC variance worst after tonemap.
- Screenshot: `/tmp/byro-hair/ext_auto.png` (exterior, auto) — the crop in the report came from this family of frames.
- Unknown from the crop alone: whether the noise persists with a fully static camera after SVGF has accumulated for seconds (progressive convergence would rule accumulation-reset bugs in or out), or whether it is worst in motion. First check when investigating: static-cam multi-second capture vs in-motion capture.

## Where to look

- `svgf_temporal.comp` variance-driven blend radius / alpha on low-luminance texels (the variance estimate may saturate and effectively bypass filtering).
- ReSTIR-DI reservoir reuse on the terrain (large triangles, few candidates) — spatial bias/visibility-reuse limits.
- The GI pass's per-hit top-K light sampling (`pathHitRadiance`, #4017) on dark splat materials.
- Interaction with TAA (YCoCg variance clamp may re-introduce grain the SVGF pass removed, or vice versa).

Related family: RT lighting recovery doc (`docs/engine/rt-lighting-material-recovery.md`) — noise/quality on dim surfaces is its territory.
- **Severity**: LOW
- **Dimension**: EXAL boundary discipline (unit lift completeness)
- **Location**:
  - `crates/plugin/src/esm/records/spatial_units.rs:156-158` (deferral comment);
  - `crates/plugin/src/esm/records/misc/water.rs:1348-1357` (`noise_uv_scale_{a,b,c}` at 120/124/128);
  - `byroredux/src/env_translate.rs:806-814` (clamp `[1/4096, 1/8]`).
- **Status**: NEW. #5151's issue body says "settle it with a capture before lifting", but #5151 is closed, and no open issue tracks this (searched "noise UV Starfield" and "tile sizes water Starfield capture").
- **Tier Violated**: no-fabrication (a unit is chosen implicitly: the tile is read as BU).
- **Game Affected**: Starfield.
- **Description**:
  - Every other Starfield DNAM length is lifted. The three tile sizes (vanilla 72.11 / 39 / 13 m-or-BU) are inverted as BU, so the primary noise tile repeats every ~1 m.
  - Read as metres, the tile is 5 048 BU, which the translate clamp would cap at 4 096.
  - FO76, the same layout in BU, authors 279 / 168 / 56.
  - The Starfield-only `displacement` (72/76/80) and `normal_falloff` (52/56/60) lanes have the same unsettled status and are not listed anywhere.
- **Impact**: If the lanes are metric, Starfield water normals tile ~70× too finely. Today nothing records that an open decision exists.
- **Suggested Fix**: File a tracking issue (capture-gated) covering the tile sizes and the other Starfield DNAM lanes with an unclassified unit. List them in watal.md's per-game unit table.

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games sharing the decoder / other consumers of the same field / other copies of the stale text)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL `translate_*` (or `spatial_units::normalize`) boundary — never pushed into shaders/renderer, never re-derived at render time
- [ ] **TESTS**: A regression test pins this specific fix
- **Severity**: LOW
- **Dimension**: EXAL boundary discipline (test coverage of the parse-boundary lift)
- **Location**:
  - `crates/plugin/src/esm/records/spatial_units.rs:106-124` (LGTM arm, including the #5002 lines 117-123);
  - `crates/plugin/src/esm/records/misc/world.rs` test `starfield_lgtm_data_decodes_the_sf_xcll_layout` (calls `parse_lgtm` directly: wire values, no normalize);
  - `crates/plugin/src/esm/records/spatial_units_tests.rs` (no LGTM or XCLL case).
- **Status**: NEW
- **Tier Violated**: n/a (test gap)
- **Game Affected**: Starfield
- **Description**:
  - `spatial_units_tests.rs` pins REFR/LIGH/SCOL (`:55`), WTHR (`:158`) and WATR (`:245`). It has no case for `index.lighting_templates`, and none for CELL XCLL `lighting()`: fog near/far, fog_clip, light fades, the SF height mid/ranges.
  - Deleting the whole LGTM arm, or just #5002's four height lines, leaves every test green.
  - The baseline's suggested-fix text ("a spatial_units test like the XCLL/LGTM ones") assumed tests that do not exist.
- **Impact**: A regression to metric LGTM/XCLL fog (the ~70× class of #5134) would go undetected. 6 vanilla LGTMs and every Starfield interior XCLL depend on this arm.
- **Suggested Fix**:
  - Add a `parse_esm` fixture with a 108-B Starfield LGTM, using `ShipInteriorLT` values plus distinct heights, and a cell with a 108-B XCLL.
  - Assert fog/clip/fade/heights ×70 and the scales/gravity untouched.
  - Add a non-Starfield companion that asserts the authored values.

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games sharing the decoder / other consumers of the same field / other copies of the stale text)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL `translate_*` (or `spatial_units::normalize`) boundary — never pushed into shaders/renderer, never re-derived at render time
- [ ] **TESTS**: A regression test pins this specific fix
**Source**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `docs/engine/nifal.md` (§ parked Starfield kinds; §3 "Drawn-surface exemptions"); `byroredux/src/asset_provider/material/merge.rs` (the `.mat` gate comment in `merge_external_material`); `byroredux/src/material_translate.rs` (`every_exterior_spawner_inserts_a_boundary_material` doc)
- **Status**: NEW
- **Description**:
  - nifal.md still says "zero Starfield texture roles are produced, so the gap is latent". It also names `mat_path_forwards_no_texture_roles_until_cdb_phase_2_lands` as the pin that "gets rewritten", but that test no longer exists. 18fce7e43 forwards colour/normal/emissive/height and replaced it with `mat_path_lookup_miss_keeps_presence_only_and_no_textures` and `mat_path_merges_cdb_authored_textures_when_indexed`.
  - nifal.md records none of the CDB scalar translations: flat colour, alpha, glass, SSS.
  - The `merge_external_material` comment still describes Phase 1: "forwards no authored field. Phase 2 should return `Merged`…".
  - nifal.md §3 says there are "exactly four deliberate exemptions": Cornell, `crates/save`, ground cover, and mesh→participating medium (#5102). The spawner-guard doc lists Cornell, save, ground cover and `scene.rs` demo primitives. The two lists differ.
  - The guard doc names `npc_spawn/resumable.rs`, which is now `npc_spawn/resumable/mod.rs`. It names `cornell.rs`, but the Cornell `MeshHandle` inserts are in `cornell/builders.rs`.
- **Suggested Fix**: Rewrite the nifal.md Starfield paragraph to describe the forwarded roles and point at the live pins. Reconcile the two exemption lists. Update the guard-doc paths.
- **Folded in**: REN-D12-2026-10-03-03 (Dim 12). The `mat.set` glass-optics comment in `byroredux/src/commands/scene.rs` still says "`cornell.rs`'s `glass()`"; since #5090 (8f38df6df) `glass()` lives in `byroredux/src/cornell/builders.rs`. Fix the pointer in the same change.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix (or a doc-pin / `_audit-validate.sh` check where one fits)

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/shaders/svgf_temporal.comp` — `main()`, the bilinear-tap loop's `isnan(sInd)` guard and the `else if (length(motion * screen.xy) < 1.5)` nearest-tap branch (`nearID`)
- **Status**: NEW (gap in the closed #903 fix; #737 added the fallback before #903 landed, and #903 guarded only the bilinear loop)
- **Description**: #903 drops a non-finite history tap with `continue` inside the 4-tap bilinear loop. If every tap is dropped (or carries ~0 weight), `wTotal <= 0.01`, and the sub-pixel-motion fallback runs. That fallback picks `q = ivec2(round(prevPx))`. This is always one of the same four taps, so it is the one just rejected. It applies only the mesh-ID and normal tests, then reads `prevIndirectHistTex`/`prevMomentsHistTex` with **no** finite check. With a parked camera, `motion == 0`, so `prevPx == p` (within float error). The bilinear weight is ≈1 on the pixel itself and ≈0 on the other three taps, so a NaN in the pixel's own history always reaches the fallback and is taken back. `mix(NaN, currInd, alphaC)` then writes NaN to this frame's temporal output. That output *is* the next frame's history (à-trous does not feed back, per `svgf.rs`'s history wiring). The NaN therefore self-perpetuates for as long as the camera stays parked or moves under 1.5 px/frame.
- **Evidence**: bilinear loop: `if (any(isnan(sInd)) || any(isinf(sInd)) || any(isnan(sMom)) || any(isinf(sMom))) { continue; }`. Fallback branch: `histInd = texelFetch(prevIndirectHistTex, q, 0).rgb; histMom = sMom.xy; histAge = sMom.z; hasHistory = true;`, with no `isnan`. The current-frame sample has no guard either: the firefly clamp `if (currLum > maxL)` is false for NaN, so a single-frame NaN in raw indirect enters history directly. No test pins #903 on the SVGF side (`grep -n "isnan\|#903" crates/renderer/src/vulkan/svgf.rs` finds nothing).
- **Impact**: This is dormant defence-in-depth, and #903 itself records "no live NaN source today". But in the parked case, the one where history lives longest, the guard does nothing. A transient NaN from any future RT branch would become a permanent dark or garbage blob, spread each frame by the 3 à-trous iterations into composite until the camera moves. The `presentation.frag` `ImageHealth` non-finite counter (#2736) would report it but not clear it.
- **Related**: #903, #737, #1159 (all closed). #4782 (same class, V-buffer history, closed).
- **Suggested Fix**: Apply the same `isnan`/`isinf` rejection to the fallback's `sMom` and `histInd` before setting `hasHistory`. Better, sanitise `currInd` once at the top of `main()` so no non-finite value is ever written to history. Pin both with a GLSL source-scan test in `svgf.rs`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix

**Source**: `docs/audits/AUDIT_SAFETY_2026-10-05.md` — `SAFE-D4-2026-10-05-01` (HEAD `a2c24b16e`)

- **Severity**: LOW. This is hardening and consistency. The block is commented, and its invariant holds for a conforming driver.
- **Dimension**: Unsafe-Block Discipline
- **Location**: `crates/renderer/src/vulkan/device.rs:653`, which is new in `7f6ab8e8f` (#4895). The pre-existing siblings are at `:466` and `:481`.
- **Status**: NEW.
  - #5120 (CLOSED, `b7bc84722`) took SAFE-D4-2026-09-29-01's suggested fix in `byroredux/src/app_events.rs`: `selected_gpu.device_name_as_c_str().unwrap_or_default()`.
  - The renderer kept the raw form and gained one more instance.
- **Description**:
  - `CStr::from_ptr` scans for a NUL with no length bound. The spec guarantees `deviceName` is a NUL-terminated `char[VK_MAX_PHYSICAL_DEVICE_NAME_SIZE]`, so the SAFETY comment is true.
  - But the soundness rests on driver conformance, and a non-terminated name would read past the `properties` struct on the stack.
  - ash 0.38's `PhysicalDeviceProperties::device_name_as_c_str()` performs the same conversion through `CStr::from_bytes_until_nul` over the fixed array. That is bounded and needs no `unsafe`.
  - All three blocks feed only `log::warn!` / `log::info!` formatting.
- **Evidence**:
  ```rust
  // SAFETY: device_name is a fixed-size [c_char; 256] array
  // null-terminated by the Vulkan driver. The pointer remains valid
  // while `properties` is in scope.
  let name = unsafe { CStr::from_ptr(properties.device_name.as_ptr()) };
  log::warn!("Rejecting GPU {name:?}: missing required Vulkan features {missing:?}");
  ```
- **Impact**: None on conforming drivers. It adds 3 avoidable `unsafe` blocks to the renderer's count, and the codebase now spells one conversion two ways.
- **Related**: #5120, #4895, SAFE-D4-2026-09-29-01.
- **Suggested Fix**: Replace all three with `properties.device_name_as_c_str().unwrap_or_default()`, the spelling #5120 already uses, deleting three `unsafe` blocks and their SAFETY comments. This improves the existing code rather than adding a helper.

## Publisher note

Publisher re-check at HEAD: `crates/renderer/src/vulkan/device.rs` has **four** `CStr::from_ptr(…device_name.as_ptr())` sites, not three — `:466`, `:481`, `:524` (`selected.properties.device_name`) and `:653`. All four can take `device_name_as_c_str().unwrap_or_default()`. (`:388` is the analogous `extension_name` comparison; ash also exposes `extension_name_as_c_str()`.)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)

**Source**: `docs/audits/AUDIT_UI_2026-10-05.md` (UI-D3-2026-10-05-01) · audit HEAD `a2c24b16e`

- **Severity**: LOW. Diagnostic metadata only; every `kind` is correct.
- **Dimension**: Catalog & AVM1 Scanner
- **Profile**: SkyrimAvm1
- **Location**:
  - `crates/ui/src/catalog.rs`:
    - `:30-42` (`HeuristicNamePrefix`: "kind was inferred … a `Command` here is a weaker claim")
    - `:180` ("27 of them (one per request-typed method)")
    - `:186-189`, `:197` ("these 68 names"), `:712-714`, `:758` ("68 sweep entries")
    - `:733-734` (a broken line join inside the assertion messages)
  - `crates/ui/src/avm1_host/tests.rs`: the `four_arg >= 14` floor and its message
  - `crates/ui/src/avm1_host.rs:107-112,333-338` (the arity merge keeps the maximum)
  - `docs/engine/ui.md:603-609` ("14 by SkyUI's fourth-argument rule … and 2 by the name-prefix heuristic") and
    `:749` ("101 default tests plus 3 ignored")
- **Status**: NEW. #4720 and #4721 are closed; this is drift those fixes left behind.
- **Description**:
  - **All 16 requests are measured.** The data run reports 16 four-argument methods. The pin asserts that every
    four-argument name is typed `Request`, and the catalog holds exactly 16 `Request` entries. So both
    `request_heuristic` entries (`GetMouseButtonForSetDestination`, `ShouldShowMod`) were measured at four arguments.
  - **All 64 heuristic commands are measured.** 125 two-argument methods = 62 measured commands + 64 heuristic
    commands − `SliderClose`, which is never called. So every `command_heuristic` entry was measured at two arguments.
  - **The floor is too low.** The sweep floor `four_arg >= 14` sits two below the measured 16, so the scanner can
    silently lose two request sites. Its message ("the corpus previously measured 14 request-typed methods") misstates
    the measurement, and so does the skill's Dim 3 premise.
  - **Mixed arities are undetectable.** `arg_counts` keeps `max(count)` per name. A name called at both 2 and 4
    arguments collapses to 4, so the documented "no mixed-arity name" claim cannot be checked.
  - **Test count.** `ui.md`'s "101 default tests plus 3 ignored" counts the ignored tests twice: `--list` gives 101
    including the 3 ignored ones.
- **Evidence**:
  ```
  arities: 16 four-argument (request) methods, 125 two-argument (command) methods
  ```
  This line is from `installed_skyrim_host_calls_are_all_cataloged`, run with data. The re-derived split is
  `command 62 / request 14 / command_heuristic 64 / request_heuristic 2`.
- **Impact**:
  - The provenance is the stated input for future handler work ("a `Command` here is a weaker claim"), and it is now
    wrong for every Skyrim sweep entry.
  - The floor no longer fails loudly when the scanner loses request sites.
- **Related**: #4720, #4721 (both closed), UI-D3-2026-09-21-01
- **Suggested Fix**:
  - Promote the 66 entries to `Measured`, or add a measured-arity provenance.
  - Raise the floor to 16.
  - Record every arity seen for a name (or its min and max) and assert that no name mixes them.
  - Correct the numbers in the doc comments and in `ui.md`.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in sibling docs / comments / skill files
- [ ] **TESTS**: Where a number is restated, a test or measured source pins it

