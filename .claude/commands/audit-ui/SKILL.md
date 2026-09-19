---
description: "Deep audit of the game UI tracks — Scaleform/SWF (Ruffle host bridge, AVM1/AVM2 profiles, ABC adapter injection, archive navigator, offscreen wgpu readback, overlay upload, input routing) and the Oblivion/FO3/FNV MenuXml track (eval, layout, raster) with both HUD drivers"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# UI Audit: Scaleform (R4 + M48) and MenuXml (M48.4-M48.7)

Read `.claude/commands/_audit-common.md` (delta-first scoping, dedup, finding format) and `_audit-severity.md` for shared protocol.

Audits the *host contract* that decides whether a Bethesda menu works, on two tracks that share one overlay compositing path. Orchestrator; one Task agent per dimension (max 3 concurrent).

## Scope

- **Scaleform**: `crates/ui/src/` (`lib.rs` `UiManager`, deliberately not an ECS `Resource`; `profile.rs`; `prepare.rs`; `host.rs`; `avm1_host.rs` Skyrim scanner; `avm2_host.rs` FO4 `BGSCodeObj` ABC injection; `catalog.rs`; `navigator.rs`; `player.rs` offscreen wgpu; `input.rs`); protocol pins `crates/ui/tests/{hudmenu_protocol,fallout4_hudmenu_protocol}.rs`.
- **MenuXml**: `crates/menuxml/src/` (`eval`, `layout`, `raster`, `menu`, `profile` audited here; `parse`, `tex`, `font` are the parse side, owned by `/audit-parsers`).
- **Engine side**: `byroredux/src/{ui_input,hud,scaleform_hud}.rs`, `byroredux/src/commands/hud.rs`, `byroredux/src/app_frame.rs` (`tick_ui_overlay`, `tick_hud_overlay`), `app_events.rs`, `scene.rs` (`--menu`/`--hud` launch), renderer `crates/renderer/src/vulkan/{presentation.rs,texture.rs,context/post_passes.rs}` + `crates/renderer/shaders/ui.{vert,frag}`.
- **Handoffs**: GPU teardown order `/audit-concurrency`; Ruffle/wgpu `unsafe` `/audit-safety`; MenuXml XML/DDS/`.fnt` parse robustness `/audit-parsers`; what a HUD *shows* (actor values, inventory) `/audit-gameplay`.

**Ground truth**: `docs/engine/ui.md` (host contract, MenuXml section, pending list), `docs/smoke-tests/README.md`.

**Deliberately unbuilt** (verified 2026-09-19 against `docs/engine/ui.md`; re-verify, do not trust this line): engine handlers for host methods (menus receive `Null`), Papyrus↔UI bridge, menu stack/focus policy, font fidelity, full menu pack, Starfield HUD (#4470, blocked). Audit the *mechanism* that will carry them.

**Known-open**: #3429 (a Scaleform overlay that animates allocates a fresh full-viewport image and blocks on a fence per uploaded frame: `tick_ui_overlay` still uses `update_rgba`; the MenuXml HUD avoids it, see Dim 5).

## Parameters

`--focus <dims>` (default all 7) · `--depth shallow|deep` (`deep` traces archive → SWF/XML → VM/eval → host call → pixels → GPU).

**Extra fields**: **Dimension**: Profile & Bridge | AVM2 Adapter | Catalog & AVM1 Scanner | Resource Navigator | Render & Overlay Upload | Engine Wiring & Input | MenuXml & HUD Drivers. **Profile**: `SkyrimAvm1` | `Fallout4Avm2` | `MenuXml` | both | n/a.

## Phase 1: Setup

1. `mkdir -p /tmp/audit/ui`; dedup per `_audit-common.md`; read the latest `docs/audits/AUDIT_UI_*.md` (date D = delta baseline).
2. `cargo test -p byroredux-ui -p byroredux-menuxml`; record pass/ignored. Data-gated tests **return early (silent pass) without game data**: `crates/menuxml/tests/{vanilla_corpus,fo3_corpus}.rs` (`BYROREDUX_OBLIVION_DATA`, `BYROREDUX_FO3_DATA`), `crates/ui/tests/*_protocol.rs` (`BYROREDUX_SKYRIM_DATA`/`BYROREDUX_SKYRIMSE_DATA`, FO4), and `avm1_host/tests.rs::installed_skyrim_host_calls_are_all_cataloged` is `#[ignore]`d. A "verified" claim resting on one of these is unverified unless you ran it with data.
3. **Count, do not trust, catalog sizes.** Re-derive from `crates/ui/src/catalog.rs`: Skyrim array = **142** (74 `Measured` SkyUI-sourced + 68 `HeuristicNamePrefix` from the #3103 corpus sweep); FO4 = **269** (138 + 131, #2966). `docs/engine/ui.md` and the ROADMAP M48 row still quote 74 for Skyrim as of 2026-09-19 (doc rot; a 74 that meant the measured half is not a wrong count, a 74 that claims the whole catalog is).

## Phase 2: Dimensions

### Dim 1: Profile, Prepare & Host Bridge Transport
**Paths**: `crates/ui/src/{profile,prepare,host,lib}.rs`, `crates/ui/src/host/tests.rs`
**First step**: `git log --since=D -- crates/ui/src/host.rs crates/ui/src/prepare.rs crates/ui/src/profile.rs crates/ui/src/lib.rs`
**Guards**: `host/tests.rs::a_response_handler_may_re_enter_its_own_bridge` (handler is cloned out of the map before invocation; a refactor holding one borrow across the call re-introduces the panic); `prepare.rs` tests (`an_archive_menu_open_decompresses_and_parses_once`, `a_loose_avm1_movie_is_decompressed_once_and_never_parsed`, `a_profile_mismatch_is_rejected_before_any_further_decode`) via `SwfDecodeCounts`.
- Profile comes from `SwfMovie::is_action_script_3()`, never from `--game` or archive provenance. A forced `new_with_profile` that contradicts the movie fails loudly or is test-only. The `external_interface_id` used at registration equals the id the bridge filters on (mismatch = every host call vanishes silently). Malformed SWF returns `Err`, not a half-built player.
- Queue: `MAX_QUEUED_CALLS` (1024) evicts oldest with `pop_front`, counts each eviction, warns once. A drained batch may be non-contiguous; the engine (`app_frame.rs`) latches `dropped_host_calls` per menu and `host_call_gap_for_menu` resets it on a menu swap. Regression = comparing against zero or dropping the latch.
- Every bounded set (`callbacks`, `known_methods`, `unknown_methods`, `unanswered_methods` via `insert_bounded`; the player's error/load lists) caps at `MAX_DISTINCT_HOST_METHOD_NAMES` and logs once at the trip. Engine-authored `__byro*` names draw on a separate `RESERVED_HOST_METHOD_NAMES` (32) band so untrusted movie content cannot lock out `__byroBGSCodeObjReady`/`Destroy` (the guard sits in `insert_bounded`, the single choke point for all sets; check a new set goes through it).
- One SWF decode per open: `prepare_movie` → `PreparedMovie`; floor is two inflates + one tag walk. A stage taking raw bytes again is the regression; `--menu` passes `profile: None` and reads `UiManager::menu_profile()` (no pre-extract-and-detect).
- `ScaleformValue` round-trips in both directions with unrepresentable values becoming explicit `Null`; `ScaleformHostDispatch::MissingResponse` (Request with no response) stays distinct from `Unknown`.
- The bridge is `Rc`/`RefCell` (Ruffle is single-threaded): nothing hands a clone to another thread; `UiManager` stays out of the ECS resource set (`ScaleformHudDiag` is the console mirror).
**Output**: `/tmp/audit/ui/dim_1.md`

### Dim 2: AVM2 Adapter Injection (FO4 `BGSCodeObj`), highest risk
**Paths**: `crates/ui/src/avm2_host.rs`
**First step**: `git log --since=D -- crates/ui/src/avm2_host.rs`
**Guards**: `avm2_host.rs::no_other_injected_name_is_a_prefix_of_another`, `generated_adapter_pool_carries_no_abandoned_loader_strategy`, `crates/ui/tests/fallout4_hudmenu_protocol.rs` (data-gated). Bytecode surgery on a third-party binary: a wrong constant-pool index yields a movie that loads and misbehaves, and no test fails.
- Every index written is one the rewriter added or verified (append-then-reference ordering). Injection is idempotent (menu reload / resize rebuild must not duplicate helpers or traits).
- `ScaleformHostObjectState` has four variants (`NotRequired`, `NotPresent`, `AdapterInjected`, `AdapterInjectedWithoutDestroyHook`). The re-injection probe scans for `DESTROYED_EVENT`, not its strict prefix `DESTROY_CALLBACK`; the four destroy strings are emitted together or not at all. Both `scene.rs` menu-load log sites call `UiManager::host_object_state()` (else `NotPresent` logs like a healthy menu).
- The destroy callback is registered only when the movie's class declares `onCodeObjDestruction`; `AdapterInjectedWithoutDestroyHook` does not increment `code_object_destruction_count()`.
- One shared helper normalises `BGSCodeObj.Method` → `Method` while keeping the transport name in `ScaleformHostCall` (not 269 per-method copies).
- Degradation: an unparseable ABC tag or no lifecycle-class match returns the *original* bytes with `NotPresent` + `log::warn!`; the hard `Err` is kept only for `patch_root_constructor` (a partial rewrite would corrupt the SWF). No third branch may silently hard-fail.
**Output**: `/tmp/audit/ui/dim_2.md`

### Dim 3: Catalog Fidelity & the AVM1 Scanner
**Paths**: `crates/ui/src/{catalog,avm1_host}.rs`, `crates/ui/src/avm1_host/tests.rs`
**First step**: `git log --since=D -- crates/ui/src/catalog.rs crates/ui/src/avm1_host.rs`
**Guards** (default lane): `catalog.rs::{skyrim_catalog_is_sorted_and_unique, fallout4_catalog_is_sorted_and_unique, fallout4_catalog_provenance_split_matches_the_2966_sweep, skyrim_catalog_provenance_split_matches_the_3103_sweep, the_skyrim_prefix_heuristic_respects_camel_case_boundaries}` (`find` is a case-sensitive `binary_search_by`, so sortedness under `str::cmp` is its prerequisite). Data-gated: `installed_skyrim_host_calls_are_all_cataloged` (Phase 1).
- Diff the re-derived counts (Phase 1 step 3) against every number in `docs/engine/ui.md`, ROADMAP and prior reports; mismatch = doc rot.
- Only sweep-added entries may be `HeuristicNamePrefix`; a `Command`/`Request` mis-type on a heuristic entry is lower confidence than on a `Measured` one. `kind` only selects a diagnostic bucket (`record_call` queues every call and answers from the configured response regardless), so a misclassification moves a name between `unanswered_methods()` and `Queued`; it cannot drop a call. Spot-check high-traffic methods against the installed-ABC/SkyUI evidence in `docs/engine/ui.md`.
- AVM1 scanner: `GameDelegate.call` operands are read right-to-left; function bodies are nested byte slices and the constant pool descends with the walk; the stack is cleared on unmodelled actions (can lose a site, never invent one); `Avm1HostCallInventory::unresolved` counts recognised-but-unnamed sites. Runtime-named calls (`this.callbackName`) are invisible to a static walk; the catalog is a union, not a proof of completeness.
- `unknown_methods()` is live, not test-only: the frame driver warns once per `(menu, method)` (`ui_reported_host_methods`, bounded by the same cap) and the set is not cleared per frame or per menu load.
**Output**: `/tmp/audit/ui/dim_3.md`

### Dim 4: Resource Navigator (archive-backed loads)
**Paths**: `crates/ui/src/navigator.rs`, `crates/ui/src/player.rs` (load side)
**First step**: `git log --since=D -- crates/ui/src/navigator.rs`
**Guards**: `archive_menu_route_tests` in `byroredux/src/scene.rs` (CLI-argument parser only); `docs/smoke-tests/m48-menu-load.sh` (Vulkan + game data) is the only end-to-end gate.
- Movies are **untrusted content**; URL→archive resolution is confined to the game archives (`archive_movie_url` rejects `..` escapes; `resolve_url` joins against the movie URL). Verify absolute paths, non-archive schemes and network/filesystem escapes are refused (escape = HIGH); one backslash/case normalisation point matching `byroredux/src/asset_provider/`.
- The local-executor pump runs from the same place the player ticks; a load future that is never polled hangs the menu silently unless surfaced via `resource_error`. `resource_loads()` records misses as well as hits.
- Bounds: `resource_loads` dedups by `archive_path` with a hit counter and caps at `MAX_RECORDED_RESOURCE_LOADS`; `import_asset_paths` caps at `MAX_IMPORT_ASSET_PATHS` (512). One unresolvable `ImportAssets` URL is a recorded non-fatal error on both the root scan (`PreparedMovie::root_import_errors`) and nested scans; a `Result`-collecting `.collect()` reappearing turns one bad import into a whole-menu failure.
**Output**: `/tmp/audit/ui/dim_4.md`

### Dim 5: Render Path, Overlay Upload & Device Lifecycle
**Paths**: `crates/ui/src/player.rs`, `byroredux/src/app_frame.rs`, `byroredux/src/hud.rs` (`upload_frame`), `crates/renderer/src/vulkan/{texture.rs,presentation.rs}`, `crates/renderer/src/texture_registry/mod.rs`, `crates/renderer/src/vulkan/context/{post_passes,build_and_upload_instances}.rs`
**First step**: `git log --since=D -- crates/ui/src/player.rs crates/renderer/src/vulkan/presentation.rs crates/renderer/src/vulkan/texture.rs byroredux/src/hud.rs`
**Guards** (source-shape, default lane): `presentation.rs::ui_overlay_composites_after_the_tone_map_draw`; `lib.rs` test on `UiFrame::Hidden => {}` in `app_frame.rs`. Neither can see pixel stride, image hazards or device lifetime; say so instead of claiming verification.
- **Second GPU device**: the UI creates its own wgpu device on Vulkan beside `VulkanContext`. Quantify device + allocator + `TextureTarget` against `docs/engine/memory-budget.md` and *feedback_vram_baseline*; a per-menu device never reused is a leak class. Creation failure leaves the engine running with UI off, and a transient failure is not cached as permanent.
- `UiManager::render()` returns `UiFrame::{Fresh, Unchanged, Hidden}`: `Unchanged` reuses the previous handle, `Hidden` stops the UI quad (a hidden overlay must not keep compositing its last frame). Pixel format and row stride from `capture_frame` match the upload (a mismatch is a sheared overlay `cargo test` cannot see). `UiManager` size, `TextureTarget` size and `register_ui_quad` extent move together on resize with no orphaned target.
- **Overlay composites after tone-mapping**, inside the presentation pass at output resolution: the quad in the geometry pass (fog, bloom, TAA, FSR, ACES) is the regression. Both descriptor sets are rebound before the overlay draw (the tone-map draw binds an incompatible set 0); the UI pipeline is owned by `PresentationPipeline` and rebuilt only in its recreate.
- **`MAX_INSTANCES` overflow**: `ui_instance_idx` is captured immediately after the push and becomes `None` when it lands past the cap, at capture time, not after `UiOverlayDraw` is built (else `firstInstance` reads out of range; `robust_buffer_access` is off).
- **Triple-buffered in-place upload (MenuXml HUD)**: `MenuXmlHud` owns three fixed textures rotated per *upload*; `write_rgba_inplace` → `Texture::overwrite_rgba_pixels` allocates nothing and writes no descriptor, but requires that no in-flight frame still samples the target. That holds only while there is at most one upload per frame and `MAX_FRAMES_IN_FLIGHT` (`crates/renderer/src/vulkan/sync.rs`, 2) stays 2 or lower; the copy is its own submission with an UNDEFINED→TRANSFER_DST discard barrier. Nothing in `cargo test` sees a violation: a finding here needs validation-layer or RenderDoc evidence (*feedback_speculative_vulkan_fixes*), not a reading. The Scaleform path still uses `update_rgba` (#3429 above); `tick_hud_overlay` copies the frame (`to_vec`, ~3.5 MB) per *changed* frame.
- `SwfPlayer` (and its wgpu device) drops before the Vulkan allocator tears down; report the ordering here, the teardown finding in `/audit-concurrency`.
**Output**: `/tmp/audit/ui/dim_5.md`

### Dim 6: Engine Wiring & Input Routing
**Paths**: `byroredux/src/{ui_input,app_frame,app_events,scene,scaleform_hud}.rs`, `crates/ui/src/{input,lib}.rs`, `byroredux/src/main.rs` (`route_scaleform_window_event`)
**First step**: `git log --since=D -- byroredux/src/ui_input.rs byroredux/src/app_events.rs byroredux/src/scaleform_hud.rs`
**Guards**: `ui_input.rs` tests (`focus_release_does_not_recapture_or_rotate_on_return`, `mouse_look_requires_both_window_focus_and_capture`).
- Focus is a two-state contract: a focused menu stops world input; releasing focus leaves no key stuck and does not recapture the cursor or rotate the camera on return (`release_world_input` fires once; `apply_mouse_look` needs window focus **and** capture). A menu open/close mid-strafe leaves the camera still.
- Dispatch order: egui debug overlay → focused Scaleform menu → world. `is_debug_overlay_key` is checked before the UI swallow. A menu-open frame leaking an `InputAction` edge into `combat_input_system` / `interaction_system` is a finding here.
- winit → `UiInputEvent`: physical vs logical key, mouse buttons, wheel units (line vs pixel) and IME each translated once; `crates/ui/src/input.rs` stays winit-free; `set_mouse_in_stage` updates on every motion event including leaving the window.
- **HUD route is not a modal menu**: `scaleform_hud::launch` keeps world input (`set_input_focus(false)`), sets the stage transparent, and yields to `--menu` when a `UiManager` already exists; the Scaleform probe runs first and a won Scaleform route suppresses the MenuXml launch (mutually exclusive per run). `hud.off` must really hide it (`UiFrame::Hidden` for Scaleform, early return for MenuXml).
- Per-frame cost is timed into `bench_ui_ns`; tick+render happen once per frame and are skipped when no menu is loaded.
**Output**: `/tmp/audit/ui/dim_6.md`

### Dim 7: MenuXml Track & the HUD Drivers
**Paths**: `crates/menuxml/src/{eval,layout,raster,menu,profile}.rs`, `byroredux/src/{hud,scaleform_hud}.rs`, `byroredux/src/commands/hud.rs`, `docs/smoke-tests/m48-{4,5,6,7}-*-hud.sh`
**First step**: `git log --since=D -- crates/menuxml/src byroredux/src/hud.rs byroredux/src/scaleform_hud.rs docs/smoke-tests/`
**Guards**: `crates/menuxml/src/tests.rs` (synthetic eval/layout/raster/graft, default lane); data-gated corpus tests and protocol pins (Phase 1 step 2); smoke gates `m48-4-oblivion-hud.sh` (health-fill run 158→53 px after a pin), `m48-5-fo3-hud.sh` (tick columns 226→68), `m48-6-skyrim-hud.sh` / `m48-7-fo4-hud.sh` (chrome on/off pixel diff; **bars are not gated**, vanilla feeds them by GFx object-path calls Ruffle cannot make). Smokes need a Vulkan device + game data; `m48-menu-load.sh` exits 77 (SKIP) on missing data but `m48-4..7` exit 1 ("FAIL: missing …"), so an absent-data run reads as a failure, not a skip, and is not covered by `scripts/check-playable-smoke-contracts.sh`.
- **Eval**: a trait's operator chain is a *fold* (each op sees the previous working value; booleans are 2/0; `onlyif`); reads are on-demand and memoised per frame with `MAX_READ_DEPTH` (32) cutting cycles to 0, so a self-referential mod menu cannot hang. Engine overrides are keyed by lower-cased tile **name**: duplicate names (grafted prefabs, FO3 `hp_meter`/`ap_meter`) share one override, so verify grafted tiles get unique names. `ScreenTraits` `cropx`/`cropy` implement the 4:3-safe insets.
- **Layout**: locus chain for x/y, stable depth sort (document order tiebreak), `visible`/`alpha 0` skip, `clipwindow` intersection. Negative, NaN or huge authored extents must not panic or explode work.
- **Raster**: `blit` iterates the clip-rect range and only `blend` bounds-checks per pixel; verify the loop is clamped to the framebuffer *before* iterating (a huge authored width is otherwise a CPU stall on mod XML). Zoom contract: `zoom<0` stretches, `0`/`100` draws texels 1:1 clipped to the tile rect (stretch-as-default squeezes padded ribbon art into a constant-width bar), `>0` scales, crop applies after zoom; `tiled` repeats 1:1 with `cropx` as a wrapping scroll (compass strip); text wrap uses the font's metrics.
- **HUD drivers (`hud.rs`)**: bar fractions come from `fraction()`, which scans the *whole* `ActorValues` storage and takes the first entity with the key: no player filter, so any actor spawned earlier feeds the bar (the m48-4 script notes the boot state is driven by the mine's NPCs). Verify or file. Unchanged-skip: change signature (heading quantised to 0.1°) plus a 33 ms `HUD_REFRESH_INTERVAL` cadence cap; the rate limiter must leave the signature unset so a throttled change is not lost. Compass heading = `atan2(f.x, -f.z)`. Per-game facts live in `HudGameProfile`/`MenuProfile` (font table, strings source, AVIF keys 0x2C9/0x2D0 FO3/FNV, Skyrim 0x3E8-0x3EA): a per-game branch outside the profile is a doctrine violation (*feedback_format_translation*).
- **Scaleform HUD (`scaleform_hud.rs`)**: vanilla Skyrim `hudmenu` calls exactly `GetButtonFromUserEvent`/`PlaySound`/`RegisterHUDComponents`/`myLog` and registers only the `GameDelegate` `call`/`respond` pair; FO4's makes zero host calls at idle. The driver's `updateStats`/`RequestPlayerInfo` handlers (Skyrim only) and push table therefore answer SkyUI-class menus, not vanilla; they must stay catalog-consistent, and the push skips `__byro*` hooks. Response shapes are working hypotheses: unverified until a menu polls them.
**Output**: `/tmp/audit/ui/dim_7.md`

## Phase 3: Merge

Combine `/tmp/audit/ui/dim_*.md` into `docs/audits/AUDIT_UI_<TODAY>.md` (header per `_audit-common.md` Report finalization): Executive Summary (findings by severity; which profiles were traced end to end; re-derived catalog counts vs documented), **Host Contract Matrix** (profile × {detection, host object, transport, catalog size, live consumer, HUD route} verified/drifted), Findings (deduplicated), **Pending-Row Readiness** (invariants pinned for the unbuilt handlers / Papyrus bridge / menu stack; never listed as findings). Then `rm -rf /tmp/audit/ui`; suggest `/audit-publish docs/audits/AUDIT_UI_<TODAY>.md` (labels `ui`; add `legacy-compat` for menu-fidelity findings and `game:*` when one title's menus are the cause).
