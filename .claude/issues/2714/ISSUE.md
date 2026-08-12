# #2714: Scaleform host bridge has no engine consumer: the ExternalInterface call queue is unbounded and never drained

**Found independently by 3 audits in the same `ui-deep` suite run** — merged here.

### SAFEUI-01 — SAFETY_UI view

*`ScaleformHostBridge`'s call queue is unbounded and has zero consumers — every ActionScript host call is retained for the life of the menu*

- **Severity**: HIGH
- **Dimension**: 3 (memory & resource leaks)
- **Location**: [`crates/ui/src/host.rs`](../../crates/ui/src/host.rs):131, 221-223, 313-322 · [`byroredux/src/main.rs`](../../byroredux/src/main.rs):663-675
- **Status**: NEW
- **Description**: `BridgeState::calls` is a `VecDeque<ScaleformHostCall>`.
  `record_call` pushes one entry for **every** `ExternalInterface` call a menu
  makes. The only thing that removes entries is `ScaleformHostBridge::drain_calls`.
  A workspace-wide grep (`byroredux/`, `crates/`, `tools/`) finds **no caller of
  `drain_calls` outside `crates/ui`'s own tests** — the engine holds a
  `UiManager`, ticks it every frame, and never touches the bridge. The queue is
  therefore monotonic for the lifetime of a loaded menu.
- **Evidence**:
  ```rust
  // crates/ui/src/host.rs:313 — the only push
  state.calls.push_back(ScaleformHostCall {
      sequence, profile: self.profile,
      transport_method: transport_method.to_string(),   // String
      method: normalized.method.clone(),                // String
      host_object: normalized.host_object,
      request_id: normalized.request_id,
      arguments: normalized.arguments,                  // Vec<ScaleformValue>
      dispatch,
  });
  ```
  ```rust
  // byroredux/src/main.rs:665 — the per-frame driver; no drain anywhere
  if let Some(ref mut ui) = self.ui_manager {
      ...
      ui.tick(dt);
  ```
  `UiManager` exposes `host_bridge()` but nothing in `byroredux/` calls it
  either — `grep -rn "drain_calls\|host_bridge" byroredux/` is empty.
- **Impact**: Three heap allocations minimum per host call (two `String`s plus
  an argument `Vec`, each `ScaleformValue::String` argument adding another),
  never reclaimed until the whole `SwfPlayer` is dropped. Growth is
  content-driven rather than strictly one-per-frame — a Bethesda HUD or Pip-Boy
  menu calls the host on interaction and on state change, so a long session
  behind an open menu accumulates without bound. There is no cap, no ring, and
  no eviction. Blast radius is limited today because the SWF overlay is opt-in
  (`--swf <path>`, `byroredux/src/scene.rs`:1135) and only one player exists at
  a time — but the design's intended consumer is simply not wired, so the leak
  is structural rather than incidental. Note that the same missing wiring means
  no host method is ever `register_method`-ed or given a response, so **every**
  FO4/Skyrim call currently returns `Null` and lands in the queue as
  `Dispatch::Unknown` or `Queued` — i.e. the worst-case fill rate is the
  live one.
- **Related**: the three sibling `BTreeSet`s (`known_methods`,
  `unknown_methods`, `unanswered_methods`) are bounded by the count of distinct
  method names and are **not** part of this finding.
- **Suggested Fix**: Either drain the bridge once per frame in the engine's UI
  tick (the design intent — `let calls = ui.host_bridge().map(|b| b.drain_calls())`),
  or bound `BridgeState::calls` with a capacity + drop-oldest policy plus a
  one-shot warn, so an unwired consumer degrades instead of growing.

---

---

### CONC-D7-UI-02 — CONCURRENCY_UI view

*ActionScript→engine ExternalInterface queue is unbounded and never drained*

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

---

### TD8-2026-08-12-03 — TECH_DEBT view

*The entire R4/M48 Scaleform host bridge has no engine consumer, and its call queue is unbounded [UI]*

- **Severity**: MEDIUM
- **Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
- **Location**: `crates/ui/src/host.rs:131` + `:221` + `:313`, `crates/ui/src/lib.rs:63-150`, `crates/ui/src/navigator.rs`
- **Status**: NEW
- **Description**: `byroredux/` never calls `UiManager::host_bridge()`,
  `ScaleformHostBridge::drain_calls()`, `UiManager::invoke_callback()`,
  `UiManager::load_swf_with_profile()`, or
  `UiManager::load_swf_from_resource_provider()`. The binary's entire use of
  `crates/ui` is `UiManager::new` + `load_swf` + `tick`/`render` +
  `handle_input`/`set_mouse_in_stage`/`has_input_focus`. Every ActionScript →
  engine call the M48 work exists to deliver is recorded and then never read.
- **Evidence**:
  - `grep -rn "host_bridge\|drain_calls\|invoke_callback" byroredux/src` → the
    only hits are `ui_manager.handle_input` / `set_mouse_in_stage` /
    `has_input_focus`; zero bridge hits.
  - `crates/ui/src/host.rs:313` — `state.calls.push_back(ScaleformHostCall { … })`
    on every `ExternalInterfaceProvider::call_method`, into a
    `VecDeque<ScaleformHostCall>` (`:131`) whose only drain is the
    never-called `drain_calls()` (`:221`).
  - `byroredux/src/scene.rs:1135` — `--swf <path>` is a real, documented flag
    (`docs/engine/game-loop.md:39`, `README`-level usage in `docs/engine/ui.md:321`).
  - `crates/ui/src/navigator.rs` (564 LOC) is reachable only through
    `SwfPlayer::from_resource_provider`, whose only non-test caller is
    `UiManager::load_swf_from_resource_provider` — itself uncalled. So the
    archive-backed navigator is, in the shipped binary, unreachable code.
- **Impact**: Two distinct costs. (a) **Unbounded growth**: a menu loaded via
  `--swf` that calls `GameDelegate.call` / `BGSCodeObj.*` per frame accumulates
  one heap-allocated `ScaleformHostCall` (several `String`s + a
  `Vec<ScaleformValue>`) per call for the process lifetime. Bounded in practice
  only by how long a dev leaves the flag on. (b) **Un-exercised surface**: the
  response/handler API (`set_response`, `set_response_values`,
  `set_response_handler`, `register_method`) and the whole navigator have no
  production caller, so nothing but the crate's own tests would notice them
  regressing.
- **Related**: Same class as today's **#2712** (uploaded-but-never-sampled
  data) — data produced through a full pipeline that no consumer reads —
  reached here from the opposite end (the *engine* never reads what the *UI*
  produces, rather than the shader never sampling what the CPU uploads).
  Distinct code, distinct subsystem: this is a new finding, not a re-file.
- **Suggested Fix**: Short term, cap `BridgeState::calls` (drop-oldest with a
  warn counter) so the flag is safe to leave on; the queue is explicitly
  documented as drain-based, so a bound is a behavior-preserving guard. Medium
  term, drain it in the same main-loop block that already calls
  `ui.tick(dt)` / `ui.render()` and log unhandled methods — that also turns
  `unknown_methods()` into a live diagnostic instead of a test-only one.
  Do **not** delete the navigator or response API: they are the substrate the
  remaining M48 slices are specified against (`ROADMAP.md:628`).
- **Effort**: small (bound) / medium (wire the drain)

---
**Sources**: `docs/audits/AUDIT_SAFETY_UI_2026-08-12.md` (SAFEUI-01), `docs/audits/AUDIT_CONCURRENCY_UI_2026-08-12.md` (CONC-D7-UI-02), `docs/audits/AUDIT_TECH_DEBT_2026-08-12.md` (TD8-2026-08-12-03)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan/wgpu objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (prefer a default-suite test, not `#[ignore]`d)

