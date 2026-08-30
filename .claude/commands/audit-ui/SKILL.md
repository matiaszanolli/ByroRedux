---
description: "Deep audit of the Scaleform/SWF UI (R4 + M48) — Ruffle host bridge, AVM1/AVM2 profile split, ABC adapter injection, archive navigator, offscreen wgpu readback, input routing"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Scaleform / SWF UI Audit (R4 + M48)

Audit `crates/ui/` — the Ruffle-backed Scaleform host layer — plus its engine-side
wiring. Before this skill existed, `crates/ui/` had **no owner**: the `ui-deep`
preset borrowed `/audit-safety` + `/audit-concurrency` + `/audit-tech-debt`,
which covers FFI and drift but never audits the *host contract* — the part that
decides whether a vanilla Bethesda menu actually works.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, methodology,
deduplication, context rules, and finding format. See
`.claude/commands/_audit-severity.md` for the severity scale. Do NOT duplicate
those here.

## Scope

**Crate**: `crates/ui/src/`
- `crates/ui/src/lib.rs` — `UiManager` (main-loop-owned, deliberately **not** an
  ECS `Resource`: Ruffle's `Player` is not `Send + Sync`).
- `crates/ui/src/profile.rs` — `ScaleformProfile::{SkyrimAvm1, Fallout4Avm2}` +
  `detect` / `from_movie` / `is_avm2`.
- `crates/ui/src/host.rs` + `crates/ui/src/host/` — `ScaleformHostBridge`,
  `ScaleformHostCall`, `ScaleformHostDispatch`, `ScaleformValue`,
  `MAX_QUEUED_CALLS`.
- `crates/ui/src/avm2_host.rs` — the FO4 `BGSCodeObj` ABC-injection adapter
  (`ScaleformHostObjectState`, the `__byro_fallout4_*` helper/callback names).
- `crates/ui/src/catalog.rs` — `ScaleformHostCatalog`, the pinned per-profile
  host-method inventories.
- `crates/ui/src/navigator.rs` — `ScaleformResourceProvider`,
  `ScaleformResourceLoad`, archive-backed URL resolution + local-executor pump.
- `crates/ui/src/player.rs` — `SwfPlayer`: offscreen wgpu device, `TextureTarget`,
  frame capture.
- `crates/ui/src/input.rs` — the engine-neutral `UiInputEvent` vocabulary.

**Engine-side wiring** (Dimension 7 — outside the crate):
`byroredux/src/ui_input.rs` (winit → `UiInputEvent` translation,
`dispatch_window_event`, `release_world_input`, `is_debug_overlay_key`),
`byroredux/src/app_frame.rs` (the per-frame tick → `drain_host_calls` → `render` →
`update_rgba` chain and `ui_texture_handle` — moved out of *main.rs* by the
#2731 split; window/input event routing is its sibling `byroredux/src/app_events.rs`),
`byroredux/src/scene.rs`
(`UiManager` construction), and the renderer side
`crates/renderer/src/vulkan/context/resources.rs` (`register_ui_quad`) with
`crates/renderer/shaders/ui.vert` / `crates/renderer/shaders/ui.frag`.

**Ground truth — read before auditing**:
- `docs/engine/ui.md` — the authoritative host contract: profile split,
  `GameDelegate` / `BGSCodeObj` semantics, the pinned catalog counts, and the
  explicit *Pending* row (host-method behavior, remaining `_global.gfx` stubs,
  Papyrus↔UI bridge, menu-stack/focus policy, font fidelity, full menu pack).
- *creation_engine_ui_system* + *text_replacement_system* in project memory —
  the 34 vanilla menus and the markup/font system this layer eventually serves.

**Deliberately unbuilt — do NOT report as bugs** (verified 2026-08-20; they are
the *Pending* row — re-verify against `docs/engine/ui.md` rather than trusting
this line, per the "never write an instruction to not look" convention in
`_audit-common.md`):
engine handlers for host methods (menus currently receive `Null` by design),
Papyrus↔UI bridge, menu stack / focus policy, font fidelity, the full menu pack.
Audit the *mechanism* that will carry them, and flag anything that would make
landing them harder.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: comma-separated dimension numbers. Default: all 7.
- `--depth shallow|deep`: `shallow` = API/contract check; `deep` = trace a menu
  load end-to-end (archive → SWF → VM → host call → pixels → GPU). Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: Profile & VM Selection | Host Bridge Transport | AVM2 Adapter
  Injection | Catalog Fidelity | Resource Navigator | Render & Device Lifecycle
  | Engine Wiring & Input Routing
- **Profile**: `SkyrimAvm1` | `Fallout4Avm2` | both | n/a

## Phase 1: Setup

1. Parse `$ARGUMENTS` for `--focus`, `--depth`.
2. `mkdir -p /tmp/audit/ui`.
3. `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/ui/issues.json`.
4. Read the most recent `docs/audits/AUDIT_UI_*.md` if one exists; otherwise read
   the UI sections of the most recent `AUDIT_SAFETY_*`, `AUDIT_TECH_DEBT_*` and
   `AUDIT_INCREMENTAL_*` reports — that is where this crate's findings have been
   filed until now, and therefore where your duplicates are.
5. `cargo test -p byroredux-ui` and record the pass/ignored counts. Several tests
   need a real wgpu device and are `#[ignore]`d — note which, because a
   "verified" claim that depends on an ignored test is not verified.
6. **Count, do not trust, the catalog sizes.** `docs/engine/ui.md` quotes a
   74-method Skyrim catalog and a 269-method FO4 catalog (grown from 138 on
   2026-08-24, and the earlier "sample, not a complete surface" caveat was
   dropped from the doc along with it — re-verify that caveat's removal is
   still accurate rather than assuming it). Re-derive both from
   `crates/ui/src/catalog.rs` before citing either number — a quoted count that
   no longer matches the array is exactly the drift class #2730 was filed for.

## Phase 2: Launch Dimension Agents

### Dimension 1: Profile & VM Selection
**Entry points**: `crates/ui/src/profile.rs` — `ScaleformProfile::detect`,
`from_movie`, `is_avm2`, `external_interface_id`; `crates/ui/src/player.rs` —
`SwfPlayer::new`, `new_with_profile`, `from_resource_provider`, `profile`
**Checklist**:
- Detection is `SwfMovie::is_action_script_3()` — a property of the movie, not a
  guess from the game name or file path. Verify no caller infers the profile from
  `--game` / archive provenance; a Skyrim-family menu shipped as AS3 must route to
  `Fallout4Avm2` on its own evidence.
- `new_with_profile` lets a caller force a profile. Verify a forced profile that
  contradicts the movie either fails loudly or is documented as a test-only
  override — silently running AVM1 host wiring on an AVM2 movie yields a menu
  that loads, renders, and answers nothing.
- `external_interface_id` differs per profile (`byroredux-skyrim-ui` /
  `byroredux-fallout4-ui`). Verify the id used at registration matches the id the
  bridge filters on — a mismatch makes every host call vanish silently.
- `SwfMovie::from_data` failure path: a malformed/encrypted SWF must produce an
  `Err`, not a partially-initialized player.
**Output**: `/tmp/audit/ui/dim_1.md`

### Dimension 2: Host Bridge Transport (bidirectional ExternalInterface)
**Entry points**: `crates/ui/src/host.rs` — `ScaleformHostBridge` (`register_method`,
`set_response`, `set_response_values`, `set_response_handler`, `drain_calls`,
`dropped_calls`, `queued_call_count`, `available_callbacks`, `has_callback`,
`unknown_methods`, `unanswered_methods`, `code_object_destruction_count`),
`ScaleformHostCall`, `ScaleformValue`, `MAX_QUEUED_CALLS`
**Checklist**:
- **Drain-based queue with a backstop.** `MAX_QUEUED_CALLS` (1024) evicts oldest
  and increments `dropped_calls`; the overflow warn is one-shot
  (`overflow_warned`) with further drops counted, not logged. Verify: eviction is
  `VecDeque::pop_front` (O(1), not `Vec::remove(0)`), the counter increments once
  per eviction, and the one-shot warn cannot re-arm into per-frame spam.
- **A drained batch may be non-contiguous** once eviction has fired. Verify
  `drain_calls`' doc contract is honoured by consumers — any consumer that treats
  `sequence` as gap-free is wrong. **The engine now actually reads the drop
  counter (#2969, `a984836c`)**: `UiManager::dropped_host_calls()` sits beside
  `drain_host_calls()`, and `byroredux/src/app_frame.rs` latches it, warning on
  each *increase* with how many calls the menu lost and how many the current
  batch holds. Latched rather than compared against zero so the message tracks
  increases instead of repeating every frame; a **decrease** is a menu swap
  handing over a fresh bridge, which `host_call_gap` treats as a reset rather
  than letting `checked_sub` wrap it into an enormous gap. The bridge's own
  producer-side warn says "a call was lost"; this says "the batch you are about
  to act on has a hole in it", which is the one that stops being cosmetic once
  the loop routes calls into quest / inventory / player state. Regression =
  dropping the latch, or comparing against zero. Sibling check: every other
  bounded channel here (`callbacks_capped`, `known_methods_capped`,
  `unknown_methods_capped`, `unanswered_methods_capped` via `insert_bounded`,
  and `player.rs`'s `resource_errors_capped` / `resource_loads_capped`) logs
  once at the point it trips — the drop counter was the only one stored for a
  consumer and never read.
- **One SWF decode per menu open (#2968, `0e91fc5e`).** `crates/ui/src/prepare.rs`
  (`prepare_movie`, `PreparedMovie`, `SwfDecodeCounts`) does the decompress once
  and the tag parse at most once, then hands each load stage what it wanted.
  `SwfPlayer`'s constructors used to hand raw bytes to four independent stages —
  profile detection, host-object injection, `ImportAssets` extraction, and
  Ruffle's `SwfMovie::from_data` — each re-inflating the whole compressed stream
  and two of them walking every tag, synchronously on the winit main-loop
  thread. On FO4's multi-megabyte `hudmenu.swf` / `pipboymenu.swf` that was four
  inflates and two tag walks per menu open. The final `SwfMovie::from_data`
  still decompresses (Ruffle exposes no constructor taking an already-decoded
  `SwfBuf`), so the floor is **two inflates and one tag walk**, not one — and
  `SwfDecodeCounts` exists to make that assertable rather than intended.
  Regression = a stage re-added that takes raw bytes instead of `PreparedMovie`.
- `ScaleformValue` conversion in both directions: AS → Rust on call arguments,
  Rust → AS on `respond`. Verify number/bool/string/null round-trips and that an
  unrepresentable value becomes an explicit Null rather than a panic.
- `ScaleformHostDispatch::{Unknown, MissingResponse}` are the diagnosis channel.
  Verify a `Request`-kind method with no registered response resolves to
  `MissingResponse` (not `Unknown`) — those are different bugs for whoever lands
  the handler, and collapsing them destroys the signal.
- `set_response_handler` closures run inside the VM callback. Verify they cannot
  re-enter the bridge in a way that deadlocks the `RefCell`/borrow (`state.borrow()`
  held across a call into ActionScript is the classic re-entrancy panic).
- Interior mutability: the bridge is `Rc`/`RefCell`-based because Ruffle is
  single-threaded. Verify nothing hands a bridge clone to another thread and that
  `UiManager` staying out of the ECS `Resource` set is still true.
**Output**: `/tmp/audit/ui/dim_2.md`

### Dimension 3: AVM2 Adapter Injection (FO4 `BGSCodeObj`)
**Entry points**: `crates/ui/src/avm2_host.rs` — the ABC rewrite path
(`swf::avm2::read::Reader` → `Writer`, `decompress_swf` / `parse_swf` /
`write_swf`), `ScaleformHostObjectState`, the `__byro_fallout4_*` helpers and
`__byroBGSCodeObj*` callbacks, `ADAPTER_NAME`
**Why it is the highest-risk dimension**: this is bytecode surgery on a
third-party binary before Ruffle parses it. A wrong constant-pool index does not
fail a test — it produces a movie that loads and misbehaves.
**Checklist**:
- Constant-pool / multiname index handling: every index written must be one the
  rewriter itself added or verified present. Off-by-one into a `ConstantPool` is
  the signature failure here — check the append-then-reference ordering.
- Idempotency: injecting twice (menu reload, resize-triggered rebuild) must not
  double-install helpers or duplicate traits. Verify a guard on `ADAPTER_NAME` or
  the helper prefix.
- `ScaleformHostObjectState` has four variants: `NotRequired` (AVM1),
  `NotPresent` (movie doesn't declare `BGSCodeObj`/`onCodeObjCreate`),
  `AdapterInjected`, and `AdapterInjectedWithoutDestroyHook` — added
  2026-08-24 for movies whose lifecycle class declares `onCodeObjCreate` but
  not the optional `onCodeObjDestruction` trait. A movie that never creates
  `BGSCodeObj` must land in `NotPresent` and be visible to the engine — not
  silently look identical to success.
- Lifecycle ordering: constructor patch → object populated → `onCodeObjCreate`
  → … → destroy callback (only if declared) → `code_object_destruction_count`.
  **The destroy callback is registered only when the movie's class declares
  `onCodeObjDestruction`** (no longer unconditional) — verify the injector
  checks for the trait before registering rather than assuming its presence,
  and that `AdapterInjectedWithoutDestroyHook` correctly skips incrementing
  `code_object_destruction_count()` (there is no hook to invoke it). For
  movies that DO declare the hook, verify the destroy path still runs on menu
  close and the counter is observable.
- Every forwarding function must normalize `BGSCodeObj.Method` → logical method
  `Method` while retaining the transport name in `ScaleformHostCall`. Verify the
  normalization is one shared helper, not repeated per method (212 copies of a
  string split is exactly the drift `/audit-tech-debt` Dim 2 exists for).
- Failure to rewrite must degrade to "no host object", never to a corrupted SWF
  handed to Ruffle. Verify the error path returns the *original* bytes.
**Output**: `/tmp/audit/ui/dim_3.md`

### Dimension 4: Catalog Fidelity & Drift
**Entry points**: `crates/ui/src/catalog.rs` — `ScaleformHostCatalog::for_profile`,
`methods`, `host_object`, `find`; `ScaleformHostMethodKind::{Command, Request}`
**Checklist**:
- Re-derive both method counts from the source arrays (Phase 1 step 6) and diff
  them against every number quoted in `docs/engine/ui.md` and in this repo's
  audit reports. A mismatch is doc rot → `/audit-tech-debt` Dim 3 severity floor.
- `Command` vs `Request` classification is what decides whether the menu waits
  for a response. A method mis-typed as `Command` leaves an AS callback hanging
  forever; mis-typed as `Request` produces a spurious `MissingResponse`. Spot-check
  the classification of the highest-traffic methods against the SkyUI /
  installed-ABC evidence cited in `docs/engine/ui.md`.
- `find` case-normalization: the catalog is documented as case-preserving with
  case-insensitive lookup. Verify exactly one normalization point.
- Methods observed at runtime but absent from the catalog surface through
  `unknown_methods()`. Verify that path is live (not test-only) — the frame
  driver (`byroredux/src/app_frame.rs`, post-#2731) logs
  a one-shot warn per unknown method; confirm the de-dup set actually suppresses
  repeats and is not cleared per frame.
**Output**: `/tmp/audit/ui/dim_4.md`

### Dimension 5: Resource Navigator (archive-backed loads)
**Entry points**: `crates/ui/src/navigator.rs` — `ScaleformResourceProvider`,
`resolve_url`, `ScaleformResourceLoad`, the local-executor pump;
`crates/ui/src/player.rs` — `from_resource_provider`, `resource_loads`,
`resource_error`
**Checklist**:
- URL → archive path resolution must be confined to the game archives. Verify a
  menu cannot escape to the filesystem or the network: relative traversal
  (`../`), absolute paths, and any non-archive scheme must be refused. Ruffle
  movies are **untrusted content** — this is a real trust boundary, treat an
  escape as HIGH.
- Case-insensitivity + backslash→forward-slash normalization match the
  engine-wide asset convention (`byroredux/src/asset_provider/`). Verify one
  normalization point, not two divergent ones.
- The local-executor pump must be driven from the same place the player ticks.
  A load future that is created but never polled hangs the menu with no error —
  verify pending loads are either advanced each tick or surfaced through
  `resource_error`.
- `resource_loads()` is the observability channel for what a menu actually
  requested. Verify it records misses as well as hits — a missing asset that
  leaves no trace is undebuggable.
- Unbounded growth: `resource_loads` (`Vec<ScaleformResourceLoad>`, exposed as
  `&[ScaleformResourceLoad]`) still accumulates for the life of the player —
  confirm it is bounded or cleared on menu swap. Contrast with the sibling
  `import_asset_paths` set, which is **now bounded** (`MAX_IMPORT_ASSET_PATHS`
  = 512, `extend_import_asset_paths`, 2026-08-24): it latches
  `import_asset_paths_capped` and logs one warn on overflow rather than
  growing without limit. A hostile or malformed `ImportAssets` graph with more
  than 512 distinct paths is the concrete DoS shape this closes — verify
  `resource_loads` doesn't have the same unbounded exposure to that same
  input.
**Output**: `/tmp/audit/ui/dim_5.md`

### Dimension 6: Render Path & Device Lifecycle
**Entry points**: `crates/ui/src/player.rs` — `SwfPlayer::new` (wgpu instance /
adapter / device creation, `Descriptors`, `TextureTarget`, `WgpuRenderBackend`),
`tick`, `render`, `dimensions`; `byroredux/src/app_frame.rs` (the
`update_rgba` upload); `crates/renderer/src/vulkan/context/resources.rs`
(`register_ui_quad`)
**Checklist**:
- **A second GPU device.** The UI creates its own wgpu device on the Vulkan
  backend, separate from `VulkanContext`. Quantify it: one extra logical device,
  one extra allocator, and the `TextureTarget` at menu resolution. Check it
  against `docs/engine/memory-budget.md` and against `feedback_vram_baseline`
  (RT floor is 6 GB; budget total under ~4 GB) — a per-menu device that is
  created and never reused is a leak class, not a style question.
- Device/adapter creation failure must leave the engine running with the UI off,
  never panic. Verify the failure is not cached in a way that permanently
  disables the UI for the session (a transient failure should be retryable).
- `render()` returns `Option<&[u8]>` — `None` means "no new frame". Verify the
  engine reuses the previous `ui_texture_handle` on `None` (it does today) rather
  than uploading a stale or empty buffer.
- Pixel format and row stride from `capture_frame` must match the
  `update_rgba` expectation exactly. A stride mismatch is a sheared overlay, and
  nothing in `cargo test` can see it — say so rather than claiming verification.
- Resize: `UiManager.width/height` vs the `TextureTarget` size vs
  `register_ui_quad`'s descriptor. Verify all three move together on a window
  resize, and that a resize during an active menu does not orphan the old target.
- **The overlay composites AFTER tone-mapping, in the presentation pass
  (regression guard, #3426, `b28acb0c`).** The UI quad used to draw at the tail
  of the *main geometry* pass, alpha-blended into colour attachment 0 — the
  render-resolution HDR direct-lighting G-buffer — so every menu went through
  height fog / volumetric transmittance keyed off the world depth still under
  it, the M58 bloom add, TAA accumulation with a zero motion vector and no FSR
  reactive or transparency mask, FSR upscaling, and only then
  `aces(graded * exposure)` in `presentation.frag`, which maps linear 1.0 to
  ~0.80 — white menu chrome reached the swapchain at ~80% grey. It now draws
  inside the presentation pass, immediately after the fullscreen tone-map
  triangle and in the same subpass, at **output** resolution straight onto the
  swapchain. `create_ui_pipeline` is built against that render pass (one colour
  attachment, so the old eight-entry blend table with its
  `color_write_mask(empty)` G-buffer masking is gone) and is owned by
  `PresentationPipeline`, which is what a swapchain recreate rebuilds — the
  former `VulkanContext::pipeline_ui` and its geometry-pass lifecycle are
  retired. `ui.vert`/`ui.frag` and the shared scene pipeline layout are
  unchanged, so the overlay still reads its bindless `textureIndex` out of the
  instance SSBO; **both descriptor sets are rebound before the draw** because
  the tone-map draw binds a layout-incompatible set 0 — dropping that rebind is
  the subtle regression. No texture-format change: the capture is sRGB-encoded
  bytes uploaded as `R8G8B8A8_SRGB`, the sampler linearises, Vulkan blends in
  linear space. Regression = the quad moving back into the geometry pass, or a
  UI pipeline rebuilt anywhere but the presentation-pass recreate.
- Teardown order: `SwfPlayer` (and its wgpu device) must drop before the Vulkan
  context tears down its allocator. Cross-reference `/audit-concurrency` Dim 6 —
  report the ordering fact here, keep the GPU-teardown finding there.
**Output**: `/tmp/audit/ui/dim_6.md`

### Dimension 7: Engine Wiring & Input Routing
**Entry points**: `byroredux/src/ui_input.rs` — `dispatch_window_event`,
`release_world_input`, `is_debug_overlay_key` (still called from
`byroredux/src/main.rs`'s `route_scaleform_window_event`);
`byroredux/src/app_frame.rs` — the UI block in the frame loop (tick →
`drain_host_calls` → `render` → upload); `byroredux/src/app_events.rs` — the
winit event arm and `release_world_input_for_ui`, the
`has_input_focus` gates. **Note (2026-08-15/16)**: this dimension now shares the
input surface with the un-owned gameplay slice — `byroredux/src/interaction.rs`
became the canonical player-action producer (`ActionState`/`InputAction`, the
hold/look edges) and its `interaction_system` is the first `Stage::Update`
exclusive. UI focus must still win over world controls; a menu-open frame that
leaks an `InputAction` edge into `combat_input_system` is a Dim 7 finding.
`crates/ui/src/lib.rs` — `UiManager::handle_input`,
`set_mouse_in_stage`, `has_input_focus`, `visible`, `menu_name`
**Checklist**:
- **Focus is a two-state contract**: when the menu has focus the world must stop
  receiving input, and when focus is released the world must not stay latched.
  `release_world_input` exists for exactly that. Verify a menu open/close cycle
  leaves no key stuck down (open a menu mid-strafe → close → camera keeps moving
  is the concrete failure).
- The debug-overlay key must remain reachable while a menu holds focus
  (`is_debug_overlay_key` is checked before the UI swallow). Verify the ordering.
- winit → `UiInputEvent` translation: verify the physical/logical key split,
  mouse-button mapping, wheel-delta units (line vs pixel), and IME events are
  each translated once. `crates/ui/src/input.rs` is deliberately winit-free —
  flag any winit type that leaked into the crate.
- `set_mouse_in_stage` drives hover/cursor behavior; verify it is updated on
  every relevant motion event, including leaving the window.
- Per-frame cost: the UI block is timed into `bench_ui_ns` when benching. Verify
  the tick+render happens once per frame and is skipped entirely when no menu is
  loaded (`ui_manager` is `None`) — a hidden menu that still ticks Ruffle is pure
  waste, and `/audit-performance` has no dimension that would catch it.
- The host-call consumer logs at `debug` per call and warns once per unknown
  method. Verify the warn de-dup set (`ui_reported_host_methods`) is not reset
  per menu load in a way that re-spams on every open.
**Output**: `/tmp/audit/ui/dim_7.md`

## Phase 3: Merge

1. Read all `/tmp/audit/ui/dim_*.md`.
2. Combine into `docs/audits/AUDIT_UI_<TODAY>.md`:
   - **Executive Summary** — findings by severity; which profile(s) were actually
     traced end-to-end; the re-derived catalog counts vs the documented ones.
   - **Host Contract Matrix** — profile × {detection, host object, transport,
     catalog size, live consumer} with verified/drifted per cell.
   - **Findings** — grouped by severity, deduplicated.
   - **Pending-Row Readiness** — which invariants this audit pinned for the
     handlers, Papyrus↔UI bridge and menu-stack work that are still unbuilt.
     Do **not** list those as findings.
3. Cross-audit dedup: GPU teardown ordering belongs to `/audit-concurrency`
   Dim 6, Ruffle/wgpu `unsafe` to `/audit-safety`, generated-adapter and catalog
   drift to `/audit-tech-debt`. Report the fact once, here, with a pointer.

## Phase 4: Cleanup

1. `rm -rf /tmp/audit/ui`
2. Inform the user the report is ready.
3. Suggest: `/audit-publish docs/audits/AUDIT_UI_<TODAY>.md`
   (domain label: `ui`; add `legacy-compat` when the finding is about Bethesda menu
   fidelity, and the matching `game:*` when it is specific to one title's menus).
