# AUDIT — Concurrency, Dimension 7 (UI-weighted) (`/audit-concurrency --focus 7`)

**Date**: 2026-08-12 · **Repo**: `/mnt/data/src/gamebyro-redux` @ `9e227649` · **Depth**: deep
**Suite**: `ui-deep` (focused run) · **Dedup baseline**: `/tmp/audit/issues.json` (400 issues, all states)
**Dimension scratch input**: `/tmp/audit/concurrency_ui/dim_7.md`

---

## 1. Scope — read this before acting on the report

This is the `ui-deep` suite's run of **`/audit-concurrency` Dimension 7 ONLY — worker threads,
pumps, and thread-safety bounds — weighted to the Scaleform/SWF UI host layer.**

It **complements and does not replace** `docs/audits/AUDIT_CONCURRENCY_2026-08-12.md`, which is
the morning `renderer-deep` run of **Dimensions 1, 2 and 3 only**. That report states in its own
scope section that Dimension 7 was not executed. Nothing here restates its dims-1-3 findings; where
this report touches renderer code it is at a call site the morning run did not reach (the morning
run never mentions the UI overlay, Ruffle, `update_rgba`, or the bindless-set slot rotation).

| Dim | Subject | Run here? |
|---|---|---|
| 1 | Vulkan queue & AS sync | NO — covered by the morning report |
| 2 | Compute → AS → fragment chains | NO — covered by the morning report |
| 3 | ECS lock ordering & deadlock | NO — covered by the morning report |
| 4 | Scheduler access declarations | NO — not covered by either report today |
| 5 | RwLock `Resource`↔`Storage` / physics | NO — not covered by either report today |
| 6 | Resource lifecycle | NO — not covered by either report today |
| 7 | Worker threads, pumps, `Send`/`Sync` bounds | **YES** |

**`crates/ui/` is an un-owned subsystem.** Per the "Un-owned subsystems" table in
`.claude/commands/_audit-common.md` there is no `/audit-ui` skill, so this Dimension-7 pass is the
**only concurrency coverage `crates/ui/` has ever received**. `docs/audits/AUDIT_SAFETY_2026-08-12.md`
records it as "CENSUS ONLY — effectively SKIPPED"; a `docs/audits/` grep confirms no prior report
examines the host bridge, the AVM2 adapter, or the archive navigator. Every finding below is
therefore NEW — none matched the dedup baseline (including the 21 texture-role issues #2693–#2713
filed earlier today, which do not overlap).

**Method.** Read-only source analysis plus `cargo test -p byroredux-ui --lib`
(**16 passed, 3 ignored** — the three need installed Fallout 4 / Skyrim SE corpora — **0 failed**).
**No engine was launched.** The one finding that touches a Vulkan valid-usage rule (CONC-D7-UI-01)
is derived from source *ordering* — where `begin_frame` sits relative to the call site — not from
"this looks wrong", and it carries a named, cheap runtime confirmation signal per the
speculative-fix guardrail.

### Findings by severity

| Severity | Count | IDs |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 2 | CONC-D7-UI-01, CONC-D7-UI-02 |
| MEDIUM | 2 | CONC-D7-UI-03, CONC-D7-UI-04 |
| LOW | 2 | CONC-D7-UI-05, CONC-D7-UI-06 |
| **Total** | **6** | |

### The headline

The Ruffle pump and readback themselves are **clean** (see §3 — the guard-scoping, single-poller,
and RefCell-reentrancy invariants all hold, several of them non-obviously). The defect is one step
downstream: the UI overlay uploads its readback **before** `draw_frame`, and the bindless texture
registry's "immediate write is safe because we are recording this slot right now" invariant is only
true **inside** `draw_frame`. From outside it, `current_slot` still names the *previous* frame's
descriptor set — the one a pending submission is sampling the UI texture through.

---

## 2. Findings

### CONC-D7-UI-01: UI overlay rewrites a bindless descriptor on the previous, still-pending frame slot

- **Severity**: HIGH (`_audit-severity.md`: "Vulkan spec violation → at least HIGH")
- **Dimension**: 7 — Worker Threads (UI host layer ↔ renderer boundary)
- **Location**: `byroredux/src/main.rs:662-695`, `crates/renderer/src/texture_registry.rs:1460-1488`,
  `crates/renderer/src/vulkan/context/draw.rs:1460`
- **Status**: NEW
- **Description**: `TextureRegistry::apply_descriptor_write` writes the new descriptor
  **immediately** into `bindless_sets[self.current_slot]` and defers the write for every *other*
  slot into `pending_set_writes`. Its SAFETY comment justifies the immediate write with
  "`self.current_slot` is being recorded by the CPU right now — no submitted command buffer can be
  reading `bindless_sets[self.current_slot]` concurrently". `current_slot` is set by
  `TextureRegistry::begin_frame`, whose **sole call site is inside `draw_frame`**
  (`draw.rs:1460`), after the both-slot `wait_for_fences`. The UI overlay path runs
  `ui.tick()` → `ui.render()` → `texture_registry.update_rgba(...)` in `main.rs` **before**
  `ctx.draw_frame(...)` is entered. At that moment `current_slot` still holds the *previous*
  iteration's frame index, whose command buffer was submitted at the end of the previous
  iteration and is in the pending state. `MAX_FRAMES_IN_FLIGHT` is 2
  (`crates/renderer/src/vulkan/sync.rs:6`), so `current_slot` is deterministically the slot the
  in-flight submission is using.
  The bindless layout is created with `PARTIALLY_BOUND | UPDATE_AFTER_BIND`
  (`texture_registry.rs:331-332`) — **not** `UPDATE_UNUSED_WHILE_PENDING`. `UPDATE_AFTER_BIND`
  covers the bind→submit window, not the pending window; the layout's own comment states the
  safety argument as *"safe because only previously-unbound array indices are written"*, which is
  exactly the premise `update_rgba` breaks: it rewrites an **already-bound, actively-sampled**
  array index. And even if `UPDATE_UNUSED_WHILE_PENDING` were added, its exemption is conditioned
  on the descriptor not being *dynamically used* by the pending command buffer — here it is:
  `draw.rs:2681-2691` appends a UI instance carrying `texture_index: ui_tex`, `geometry_pass.rs:62`
  binds `texture_registry.descriptor_set(frame)` (== `bindless_sets[frame]`), and `ui.frag` samples
  that index every frame the overlay is up.
- **Evidence**:
  - `texture_registry.rs:1476` — `self.bindless_sets[self.current_slot]` as the immediate write target.
  - `draw.rs:1460` — `self.texture_registry.begin_frame(&self.device, frame);` — the only writer of
    `current_slot`, inside `draw_frame`, after the fence wait.
  - `main.rs:686` — `.update_rgba(upload_ctx, handle, ui_w, ui_h, pixels)`, ~110 lines before the
    `ctx.draw_frame(FrameInputs { … ui_texture_handle: ui_tex, … })` call at `main.rs:796`.
- **Trigger Conditions**: any frame `N ≥ 2` in which the UI overlay is visible and frame `N-1`'s
  submission has not yet retired when the CPU reaches the UI block. This is the *common* case, not
  a narrow window — the CPU normally runs a frame ahead.
- **Impact**: host writes to a descriptor a live shader invocation is reading. Consequence is
  driver-dependent (torn descriptor read → sampling a destroyed-or-mismatched image view). Note the
  rendered *content* is still correct by construction — the current frame's set gets the same
  payload through the `pending_set_writes` flush — so this failure is invisible without validation,
  which is precisely why it has survived unexamined.
- **Verification Path**: **NOT observable in `cargo test`** (no headless device assertion covers
  descriptor-in-use). Cheapest confirmation: a **release** run with `BYRO_VALIDATION=1` and `--swf`.
  The expected signal is `VUID-vkUpdateDescriptorSets-None-03047` (descriptor set in use by a
  command buffer in the pending state) firing once per frame while the overlay is visible.
  Treat the finding as confirmed only once that message is captured; the source-order argument
  above is what makes it worth spending the run on.
- **Related**: #92 (the `pending_set_writes` deferral this path bypasses); #134 (the deferred image
  destruction that *does* cover the image, see §3.6); CONC-D7-UI-03 below shares the call site.
- **Suggested Fix**: make the immediate-write path refuse to run outside a recording window — e.g.
  have `apply_descriptor_write` queue for **all** slots (including `current_slot`) when a
  `recording: bool` latch set by `begin_frame` / cleared at submit is false. Cheaper interim fix:
  move the UI tick/render/upload block from `main.rs` to inside `draw_frame` after
  `texture_registry.begin_frame`, which restores the invariant the SAFETY comment asserts.

---

### CONC-D7-UI-02: ActionScript→engine ExternalInterface queue is unbounded and never drained

- **Severity**: HIGH (`_audit-severity.md`: "Memory/resource leak per frame → HIGH")
- **Dimension**: 7 — Worker Threads (queue bounding)
- **Location**: `crates/ui/src/host.rs:127-138`, `crates/ui/src/host.rs:313-322`,
  `crates/ui/src/host.rs:221-223`
- **Status**: NEW
- **Description**: `BridgeState::calls` is a plain `VecDeque<ScaleformHostCall>` with **no capacity
  bound**. `ScaleformHostBridge::record_call` pushes one entry **unconditionally** on every
  `ExternalInterface.call` from the movie — for every `ScaleformHostDispatch` variant, including
  `Unknown` and `ImmediateResponse`, i.e. even calls that were already answered synchronously and
  have no engine-side consumer. The only drain is `drain_calls()`, and a workspace grep shows it has
  **zero callers outside `crates/ui/src/host/tests.rs`** — the engine binary never constructs an
  `UiManager::host_bridge()` consumer at all. Each entry owns two `String`s plus a
  `Vec<ScaleformValue>` of cloned arguments.
  Dimension 7's own checklist requires the analogous debug-server queue to be "bounded (no unbounded
  buffering on a slow main loop)"; the debug server complies (`MAX_QUEUED_COMMANDS = 64`, see §3.8).
  The UI bridge is the same class of producer→main-loop queue with neither a bound nor a consumer.
- **Evidence**: `host.rs:131` — `calls: VecDeque<ScaleformHostCall>,` (no bound anywhere in the
  file); `host.rs:313` — `state.calls.push_back(ScaleformHostCall { … })` outside any conditional;
  `host.rs:221` — `pub fn drain_calls(&self)`, unreferenced outside tests.
- **Trigger Conditions**: a loaded SWF that calls `ExternalInterface.call` (or, on the FO4 profile,
  any injected `BGSCodeObj.*` method — the adapter forwards *every* cataloged method through
  ExternalInterface). Bethesda HUD menus do this continuously, often per-frame.
- **Impact**: monotonic heap growth for the lifetime of the menu, proportional to AS→host call
  volume × session length. No self-limit and no diagnostic. Currently gated behind the `--swf` dev
  flag (the only wired load path, `byroredux/src/scene.rs:1135-1172`), so today it is a dev-path
  leak — but it is on the direct road to M48 shipping real menus, at which point it becomes a
  session-length leak in normal play.
- **Related**: CONC-D7-UI-04 (the same "wired in tests, not in the engine" gap on the navigator side).
- **Suggested Fix**: bound the queue (drop-oldest with a `dropped_calls` counter, mirroring
  `MAX_QUEUED_COMMANDS`), and/or drain it from the main loop next to the existing `ui.tick(dt)` call
  so the engine actually consumes what the menu is asking for.

---

### CONC-D7-UI-03: UI overlay allocates a fresh VkImage and does a blocking one-time submit every frame

- **Severity**: MEDIUM
- **Dimension**: 7 — Worker Threads (blocking work on the main loop)
- **Location**: `byroredux/src/main.rs:662-695`, `crates/ui/src/player.rs:199-227`,
  `crates/renderer/src/texture_registry.rs:1518-1567`, `crates/renderer/src/vulkan/texture.rs:114-172`
- **Status**: NEW
- **Description**: `SwfPlayer::tick` ends with an unconditional `self.dirty = true`, so
  `SwfPlayer::render` never takes its `if !self.dirty { return None; }` early exit while the overlay
  is visible — it re-renders and re-reads-back every frame regardless of whether anything on the
  Flash stage changed. `main.rs` then calls `update_rgba` on every such frame, and `update_rgba`
  does not reuse the existing image: it builds a **brand-new** `Texture` via `Texture::from_rgba` →
  `from_dds_with_mip_chain`, which runs `with_one_time_commands` — allocate a command buffer, create
  a `VkFence`, `queue_submit`, `wait_for_fences(u64::MAX)`, destroy the fence, free the command
  buffer — and pushes the previous image onto the deferred-destroy ring. The registry is created at
  `ctx.swapchain_extent()` (`scene.rs:1139-1140`), so at 1920×1080 that is an 8.3 MB readback plus an
  8.3 MB staging copy plus a fresh `VkImage` + `VkImageView` + allocator slab, per frame.
  This is a full CPU↔GPU serialisation point sitting **ahead of** `draw_frame` in the main loop.
  Dimension 1's checklist names exactly this pattern ("One-time command buffers block the main
  thread on a fence — flag if any such blocking submit runs inside the per-frame hot path rather
  than at load time"); the morning dims-1-3 run audited `with_one_time_commands_inner`'s queue-lock
  scoping but never reached this caller.
- **Evidence**: `player.rs:226` — `self.dirty = true;` at the tail of `tick`, unconditional;
  `texture_registry.rs:1542` — `Texture::from_rgba(...)` inside `update_rgba`;
  `texture.rs:141` — `with_one_time_commands(device, queue, command_pool, |cmd| { … })`;
  `texture.rs:811` — `device.wait_for_fences(&[fence], true, u64::MAX)`.
- **Impact**: a per-frame pipeline bubble plus per-frame GPU image churn whenever a menu is up.
  Not a leak — the deferred-destroy ring drains correctly (§3.6) — but it caps overlay-visible frame
  rate at whatever the round-trip costs, and it is what makes CONC-D7-UI-01 fire every frame instead
  of only on genuine content changes.
- **Related**: CONC-D7-UI-01 (same call site); CONC-D7-UI-05.
- **Suggested Fix**: two independent wins. (a) Only mark dirty when Ruffle actually re-rendered, or
  hash/compare the readback, so a static menu stops re-uploading. (b) Give the UI handle a
  persistent per-frame-in-flight image pair and record the copy into `draw_frame`'s own command
  buffer instead of a private submit+fence — that removes the stall and CONC-D7-UI-01 together.

---

### CONC-D7-UI-04: Navigator pump has two silent permanent-freeze paths

- **Severity**: MEDIUM (`_audit-severity.md`: "Missing error handling on recoverable paths")
- **Dimension**: 7 — Worker Threads (local-executor pump liveness)
- **Location**: `crates/ui/src/player.rs:199-227`, `crates/ui/src/player.rs:343-362`,
  `crates/ui/src/navigator.rs:105-116`, `crates/ui/src/navigator.rs:126-133`
- **Status**: NEW
- **Description**: two distinct latches, both of which stop the movie forever with no diagnostic
  after the first log line.
  1. **Sticky error.** `ScaleformNavigator::fail` pushes onto `NavigatorState::errors`, which is
     **never cleared** — there is no `clear`/`drain`/`take` on that field anywhere.
     `ScaleformNavigatorRuntime::first_error()` returns `errors.first().cloned()`, so once any single
     fetch fails it returns `Some` on every subsequent call. `tick` copies that into
     `self.resource_error`, and `tick`'s first statement is `if self.resource_error.is_some() { return; }`.
     Net effect: **one** missing dependency — and `fail` is invoked for the entirely routine
     `Ok(None)` "resource was not found in the configured archive" case, with the navigator holding
     exactly **one** `Rc<dyn ScaleformResourceProvider>` (one archive) — permanently freezes the whole
     menu, not just that asset.
  2. **Silent non-settle.** `drive_archive_preload` gives up after
     `MAX_ARCHIVE_PRELOAD_PASSES = 64` and returns `Ok(false)`; `tick` maps that to a bare `return`.
     No error is recorded, `dirty` is not set, and the check re-runs next frame — so an unsettled
     preload suppresses `player.tick()` indefinitely with no timeout and no diagnostic. Note the
     constructor treats the identical condition as a hard `Err` ("did not settle after … passes");
     only the per-frame path swallows it.
- **Evidence**: `navigator.rs:128` — `self.state.borrow_mut().errors.push(message.clone());`
  (only mutation of `errors`); `navigator.rs:114` — `self.state.borrow().errors.first().cloned()`;
  `player.rs:200-202` — the `resource_error.is_some()` early return; `player.rs:206` —
  `Ok(false) => return,`.
- **Trigger Conditions**: any menu whose `ImportAssets` graph references a file absent from the
  single configured archive (cross-archive font/shared-menu imports are the obvious case), or any
  preload that needs more than 64 passes.
- **Impact**: menu frozen for the rest of the session, last-uploaded frame left on screen. Currently
  **latent**: `SwfPlayer::from_resource_provider` / `UiManager::load_swf_from_resource_provider` have
  no callers outside `crates/ui` tests, so the engine's `--swf` path (which uses `SwfPlayer::new`,
  `navigator: None`) never enters either latch. Severity is stated by impact, not reachability, per
  the severity scale's opening rule — and this is the path M48 is heading for.
- **Related**: CONC-D7-UI-02 (same "test-wired, not engine-wired" gap); CONC-D7-UI-06.
- **Suggested Fix**: make a failed *dependency* fetch non-fatal (record it, keep ticking, surface it
  through a `resource_errors()` accessor) and reserve the hard latch for a failure of the root movie.
  For the non-settle path, either escalate to `Err` after N consecutive frames or expose a
  "preload stalled" state instead of an invisible `return`.

---

### CONC-D7-UI-05: Each SwfPlayer creates its own wgpu instance, adapter and device under `block_on`

- **Severity**: LOW
- **Dimension**: 7 — Worker Threads (blocking work on the main loop)
- **Location**: `crates/ui/src/player.rs:136-155`, `crates/ui/src/lib.rs:101-113`
- **Status**: NEW
- **Description**: `SwfPlayer::from_movie` calls `create_wgpu_instance(wgpu::Backends::VULKAN, …)`
  and `futures::executor::block_on(request_adapter_and_device(…))` per player, producing a **second
  live Vulkan device** alongside the engine's `VulkanContext`. Device creation is synchronous on the
  winit main-loop thread. `UiManager::install_player` assigns `self.player = Some(player)` only after
  the new player is fully built, so a menu swap transiently holds two Ruffle devices plus the
  engine's.
- **Impact**: a visible hitch on menu load, plus steady-state driver/VRAM overhead for a duplicate
  logical device. Bounded and released on `UiManager::close()` — not a leak.
- **Related**: CONC-D7-UI-03.
- **Suggested Fix**: hoist the `Descriptors` bundle to a lazily-created `UiManager`-owned singleton
  shared by successive players, so device creation happens once per process rather than once per menu.

---

### CONC-D7-UI-06: `NavigatorBackend::fetch` is async in signature only — archive I/O runs inline on the main loop

- **Severity**: LOW
- **Dimension**: 7 — Worker Threads (pump semantics)
- **Location**: `crates/ui/src/navigator.rs:146-230`
- **Status**: NEW
- **Description**: `fetch` returns `OwnedFuture<Box<dyn SuccessResponse>, ErrorResponse>`, but every
  code path performs its work **eagerly** and then wraps an already-computed value in
  `Box::pin(async move { Ok(response) })`. The synchronous work includes the full
  `provider.load(&archive_path)` archive extract (zlib/LZ4 decompress for BSA/BA2), plus for import
  assets a `swf::decompress_swf` + `swf::parse_swf` + tag-record rewrite in
  `prepare_import_asset_swf`. `fetch` is called by Ruffle from inside `player.preload()` /
  `player.tick()`, i.e. inside the engine's main loop, with no opportunity for the local executor to
  interleave.
- **Impact**: archive decompression and SWF reparse cost lands as a main-loop stall rather than
  amortised pump work; `MAX_ARCHIVE_PRELOAD_PASSES = 64` bounds the pass count but not the per-pass
  cost. Correctness is unaffected — and it is what makes the pump trivially single-threaded, which
  is why §3.2/§3.5 come out clean. Reachable only through the navigator path (test-only today, per
  CONC-D7-UI-04).
- **Suggested Fix**: no action while the path is dev-only. If archive menus ship, move the extract
  behind a real future serviced by the existing streaming worker's `Arc`-shared provider
  (`byroredux/src/streaming.rs`), whose `BsaArchive`/`Ba2Archive` already serialise `File` access via
  Mutex — note this would require replacing the navigator's `Rc<dyn ScaleformResourceProvider>` with
  an `Arc`-based one, which is a real design change, not a `s/Rc/Arc/`.

---

## 3. Verified clean

Each item below was actively attacked and could not be disproved as correct. Several are
non-obvious and worth keeping as documented guards.

1. **No threads exist in `crates/ui` at all.** `grep -n "thread::"` over the crate returns zero
   hits. All shared state is `Rc`/`RefCell` (`ScaleformHostBridge.state`, `NavigatorState`,
   `ScaleformNavigator.provider`), so `SwfPlayer`, `UiManager` and `ScaleformHostBridge` are
   `!Send` + `!Sync` **by construction** — `UiManager` structurally cannot be registered as an ECS
   `Resource` (which requires `Send + Sync`), matching the intent documented at
   `crates/ui/src/lib.rs:6-7`. Checked the obvious escape hatch: `ruffle_core` contains **no**
   `unsafe impl Send` / `unsafe impl Sync`, so nothing overrides the auto-trait computation.

2. **Single-poller invariant holds — a pending future cannot be polled from two places.**
   `NullExecutor` is a thin wrapper over `futures::executor::LocalPool`
   (*ruffle_core/src/backend/navigator.rs:334-346* — a cargo git checkout, not a repo path);
   `run()` requires `&mut self`, and `NullSpawner`
   is a `LocalSpawner` handle into that same pool. `ScaleformNavigator::spawn_future` is the only
   producer and `ScaleformNavigatorRuntime::run_until_stalled` the only consumer, reached from
   exactly two sites (`SwfPlayer::tick`, `SwfPlayer::drive_archive_preload`), both through
   `&mut SwfPlayer`. Ruffle does not separately drive navigator futures. No double-poll, no
   cross-thread poll, no orphaned pool.

3. **The player Mutex is never held across a pump.** This is the deadlock that Ruffle's design
   invites — `PlayerBuilder::build` hands the player a weak self-reference which loader futures
   upgrade and `lock()`, and `std::sync::Mutex` is not reentrant. Both pump sites scope the guard
   correctly: `tick` uses an explicit `{ let mut player = self.player.lock().unwrap(); player.tick(…); }`
   block that ends before `runtime.run_until_stalled()`, and `drive_archive_preload` binds
   `let finished = { … self.player.lock().unwrap().preload(&mut execution_limit) };` so the temporary
   guard is released at the end of that `let` statement, before the pump on the following line.

4. **AS→engine re-entrancy during a pump does not double-borrow the bridge.** `record_call` clones
   the response handler out under a `borrow()` whose temporary ends at the statement
   (`host.rs:280-284`) and only then invokes it, so a handler that calls back into
   `register_method` / `set_response` / `drain_calls` does not panic. Likewise
   `BridgeProvider::call_method` re-enters AVM via `callback.call(context, "respond", arguments)`
   **after** `record_call` has returned an owned `HostCallOutcome` and dropped every borrow — so
   Skyrim's deliberately re-entrant `respond` protocol is safe, including a nested
   `ExternalInterface.call` from inside the `respond` handler.

5. **Navigator borrow discipline.** `fetch` holds no `Ref`/`RefMut` across `self.fail()` (which
   takes `borrow_mut`) or across `self.provider.load(...)`; the `is_import_asset` read borrow ends
   at its own statement before the later `borrow_mut` sites.

6. **The replaced UI image's deferred-destroy countdown is not short by one.** `update_rgba` tags
   the retired texture with the *stale* `current_frame_id` (it runs before `begin_frame` increments
   it), which makes `should_destroy_pending` (`current - queued >= MAX_FRAMES_IN_FLIGHT`) **more**
   conservative, not less; and `tick_deferred_destroy` runs inside `draw_frame` after the both-slot
   `wait_for_fences`. The image lifetime is safe — unlike its descriptor (CONC-D7-UI-01). This was
   the first candidate finding investigated and it is disproved.

7. **Streaming worker Drop ordering (#1167) is intact.** `WorldStreamingState::shutdown`
   (`byroredux/src/streaming.rs:760-784`) `take()`s the `worker` handle first (so a later `Drop`
   short-circuits), then `take()`s `request_tx` so the worker's `recv()` errors out, then calls
   `join_with_timeout(handle, 1s)`. `impl Drop` (`streaming.rs:797-801`) delegates to the same
   `shutdown`. Field-declaration drop order is irrelevant because both fields are explicitly taken.

8. **Debug server queue is bounded and the drain does not hold the lock over World access.**
   `MAX_QUEUED_COMMANDS = 64` with an explicit over-limit rejection (`listener.rs:44,76-77`), and
   `DebugDrainSystem` takes the mutex, `std::mem::take`s the vector, drops the guard at the end of
   the `let commands = { … };` statement, and only then runs `evaluator::evaluate(world, …)`
   (`crates/debug-server/src/system.rs:135-192`). Per-client TCP threads never touch the World.

9. **Test baseline green.** `cargo test -p byroredux-ui --lib` → 16 passed, 3 ignored (require an
   installed Fallout 4 / Skyrim SE corpus), 0 failed.

---

## 4. Coverage gaps left by this run

- Dimensions 4, 5 and 6 were run by **neither** today's report. The `Resource`↔`Storage`
  unordered-pair class (Dim 5) in particular remains unswept today.
- Within `crates/ui`, this pass covers concurrency only. `avm2_host.rs` (1090 lines of ABC bytecode
  rewriting) and `catalog.rs` were read only for thread/reentrancy properties — their *correctness*
  is still unaudited and still has no owner skill.
- CONC-D7-UI-01 is source-order-provable but **runtime-unconfirmed**; per the speculative-fix
  guardrail no descriptor/barrier change should ship until the named `VUID-vkUpdateDescriptorSets-None-03047`
  signal is captured under `BYRO_VALIDATION=1` with `--swf`.

---

## 5. Suggested next step

```
/audit-publish docs/audits/AUDIT_CONCURRENCY_UI_2026-08-12.md
```

Recommended order of work: **CONC-D7-UI-01** first (it is a live spec violation on the default
overlay path and the `BYRO_VALIDATION=1 --swf` confirmation is a single cheap run), then
**CONC-D7-UI-03** — whose fix (persistent per-FIF image + copy recorded into `draw_frame`'s command
buffer) closes UI-01 as a side effect. **CONC-D7-UI-02** is independent and small.
**CONC-D7-UI-04** should be fixed before M48 wires archive-backed menus, not after.
