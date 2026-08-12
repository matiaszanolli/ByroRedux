# AUDIT — SAFETY (UI-FOCUSED) · 2026-08-12

## Scope line

**This is the UI-focused safety run.** It exists to cover the one subsystem the
morning's full-coverage safety audit explicitly skipped. For general safety
coverage — `unsafe` census, FFI, Vulkan spec, leaks, drop ordering, material
layout, IOR/glass, NPC/anim spawn, NIFAL NaN, egui teardown — read
[`docs/audits/AUDIT_SAFETY_2026-08-12.md`](AUDIT_SAFETY_2026-08-12.md) (11:56).
That report's own scope table records `crates/ui` as **"CENSUS ONLY —
effectively SKIPPED"**; this run closes that gap and does not re-report any of
its findings.

`crates/ui/` (Scaleform/SWF, R4 + M48) is an **un-owned subsystem** — there is
no `/audit-ui` skill, and `.claude/commands/_audit-common.md` lists it first in the "Un-owned
subsystems" table. It is therefore treated as **explicitly in scope** here, at
file granularity:

| File | Depth |
|---|---|
| [`crates/ui/src/player.rs`](../../crates/ui/src/player.rs) | full — readback lifetimes, mapped-memory windows, GPU/CPU ordering |
| [`crates/ui/src/avm2_host.rs`](../../crates/ui/src/avm2_host.rs) | full — ABC injection, constant-pool indexing, branch/exception rewriting |
| [`crates/ui/src/host.rs`](../../crates/ui/src/host.rs) + [`crates/ui/src/host/tests.rs`](../../crates/ui/src/host/tests.rs) | full — bridge state growth, `RefCell` re-entrancy |
| [`crates/ui/src/navigator.rs`](../../crates/ui/src/navigator.rs) | full — path confinement, malformed-SWF bounds |
| [`crates/ui/src/lib.rs`](../../crates/ui/src/lib.rs), [`crates/ui/src/catalog.rs`](../../crates/ui/src/catalog.rs), [`crates/ui/src/input.rs`](../../crates/ui/src/input.rs), [`crates/ui/src/profile.rs`](../../crates/ui/src/profile.rs) | full |
| Engine-side consumers: [`byroredux/src/main.rs`](../../byroredux/src/main.rs), [`byroredux/src/scene.rs`](../../byroredux/src/scene.rs), [`byroredux/src/ui_input.rs`](../../byroredux/src/ui_input.rs) | UI paths only |
| Vendored Ruffle (`~/.cargo/git/checkouts/ruffle-…/0dde981`) | read as evidence for the readback-sync and stack-frame questions; not first-party, not audited |

**Not covered by this run:** everything outside the UI surface. No general
safety sweep was repeated. Nothing was found incidentally outside `crates/ui`
that the morning report does not already carry, so **every finding below is
UI-scoped**.

Per project policy, no Vulkan/wgpu render-pass, pipeline, or barrier change is
proposed. The engine binary was not launched; evidence is read-only analysis
plus `cargo test`.

---

## 1. Executive summary

**8 findings: 1 HIGH · 3 MEDIUM · 4 LOW. All NEW** (dedup against the 400-issue
baseline at `/tmp/audit/issues.json` returned exactly one UI-adjacent issue,
#2428 `TD8-003`, CLOSED and unrelated; none of today's #2693–#2713 texture-role
issues touch this surface).

`crates/ui` contains **zero real `unsafe`** — confirming the morning census —
so there is no memory-corruption or UB class here at all. The risk in this crate
is a different shape: it **rewrites attacker-controlled-shaped binary content
(ABC bytecode, SWF tag streams) and hands it to an interpreter**, and it
**accumulates unbounded engine-side state with no consumer**.

The two things the prompt flagged as prime suspects both came back **clean**:

- **The wgpu readback is correctly synchronized.** `SwfPlayer::render` →
  *capture_frame* → *buffer_to_image* → *capture_image* performs
  `map_async` + `device.poll(Wait { submission_index: None })` + channel
  receive before touching the mapped range, and *TextureTarget::submit*
  enqueues the `copy_texture_to_buffer` in the **same** queue submit as the
  frame's command buffers. There is no assumption that the copy has landed —
  it is waited on. The mapped range is dropped and the buffer unmapped before
  the function returns, so no mapped-memory validity window survives the call.
- **The archive navigator's sandbox holds.** Menu-controlled `ImportAssets`
  URLs cannot reach the filesystem or the network.

The finding that actually matters is **SAFEUI-01**: the entire
ActionScript→engine command channel is a queue that is filled every frame and
**drained by nobody**.

---

## 2. Findings

### HIGH

#### SAFEUI-01: `ScaleformHostBridge`'s call queue is unbounded and has zero consumers — every ActionScript host call is retained for the life of the menu
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

### MEDIUM

#### SAFEUI-02: injected AVM2 bootstrap reserves operand-stack headroom with `.max(2)`, which is a no-op — an under-declared `max_stack` is an interpreter panic, not a verify error
- **Severity**: MEDIUM
- **Dimension**: 2 (memory corruption / UB — panic facet)
- **Location**: [`crates/ui/src/avm2_host.rs`](../../crates/ui/src/avm2_host.rs):467-481
- **Status**: NEW
- **Description**: `patch_root_constructor` splices a three-op bootstrap into
  the Fallout 4 lifecycle class's constructor, immediately after the op that
  initializes `BGSCodeObj`, then adjusts the method body's declared operand
  stack with `body.max_stack = body.max_stack.max(2)`. That is the wrong
  quantity. The injected sequence needs **two slots above the stack depth `D`
  at the insertion point**, i.e. `max_stack >= D + 2`. `.max(2)` only
  guarantees `max_stack >= 2`, and every real AS3 constructor already declares
  at least 2 (an `initproperty` alone consumes three operands) — so the
  statement is a **no-op on every input it will ever see**. It is correct today
  only because the ActionScript compiler happens to emit the `BGSCodeObj`
  initialization at statement level, where `D == 0`.
- **Evidence**:
  ```rust
  // crates/ui/src/avm2_host.rs:467
  let injection = write_ops(&[
      Op::FindPropStrict { index: install },   // +1
      Op::GetLocal { index: 0 },               // +1  -> peak D+2
      Op::CallPropVoid { index: install, num_args: 1 },
  ])?;
  body.code.splice(insertion_offset..insertion_offset, injection.iter().copied());
  body.max_stack = body.max_stack.max(2);     // <-- not D + 2
  ```
  Ruffle does not catch this. Ruffle's *core/src/avm2/verify.rs* at the pinned revision
  (`0dde9813`) contains **no reference to `max_stack`** — the verifier does not
  reconcile declared depth against actual. The frame is sized once, in
  *Stack::get_stack_frame*, as `max_stack + num_locals`, and *StackFrame::push*
  writes through a plain bounds-checked slice index into that subslice. An
  overflow is therefore a Rust **index-out-of-bounds panic inside the AVM2
  interpreter**, raised from `player.tick()` on the main loop. (Good news: it
  is a panic, **not** a silent write into the neighbouring frame — the subslice
  bound contains it. This is the reason this is MEDIUM and not CRITICAL.)
- **Impact**: A Fallout 4 menu whose lifecycle constructor initializes
  `BGSCodeObj` inside an expression rather than as a bare statement — legal
  ABC, producible by hand-written or obfuscated bytecode and by mod-authored
  menus — panics the engine on load. Crash-from-content, in a subsystem whose
  whole job is to run untrusted game data.
- **Related**: SAFEUI-04 (only 3 of 311 FO4 menus are ever exercised, so this
  shape would not be caught by the existing corpus test).
- **Suggested Fix**: One line — `body.max_stack = body.max_stack.saturating_add(2);`.
  Unconditionally correct for any `D`, costs two stack slots, and removes the
  dependence on a compiler emission detail.

#### SAFEUI-03: every Fallout 4 AVM2 menu is round-tripped through a full `parse_swf` → `write_swf` re-serialization, and only 3 of 311 menus have ever been shown to survive it
- **Severity**: MEDIUM
- **Dimension**: 2 (data integrity on the content boundary)
- **Location**: [`crates/ui/src/avm2_host.rs`](../../crates/ui/src/avm2_host.rs):54-137
- **Status**: NEW
- **Description**: `inject_host_object_adapter` decompresses the SWF, **parses
  every tag into the `swf` crate's typed representation**, mutates two of them,
  and then re-serializes the whole movie with `write_swf`. Every tag in the
  file — fonts, bitmaps, sprites, sounds, shapes — is decoded and re-encoded,
  not copied. Any imperfection in the `swf` crate's write path for a tag that
  Bethesda's authoring tool emitted becomes silent content corruption in a
  menu that still "loads".
  The contrast inside this very crate is the tell: the sibling rewrite in
  `prepare_import_asset_swf` ([`crates/ui/src/navigator.rs`](../../crates/ui/src/navigator.rs):383-424)
  deliberately avoids this, walking `raw_tag_records` and emitting through
  `swf::write::write_swf_raw_tags` so untouched tags pass through byte-for-byte.
  The injection path does not take that care.
- **Evidence**:
  ```rust
  // crates/ui/src/avm2_host.rs:56  — full typed parse of the entire movie
  let mut movie = parse_swf(&decompressed).map_err(...)?;
  ...
  // :134 — full typed re-encode of the entire movie
  let mut patched = Vec::new();
  write_swf(movie.header.swf_header(), &movie.tags, &mut patched)
  ```
  Coverage: `Fallout4 - Interface.ba2` holds **1101 files, 311 of them `.swf`**
  (BA2 GNRL name-table walk). The only test that drives a real menu through the
  injection path, `installed_fallout4_representative_menus_obey_host_object_lifecycle`
  ([`crates/ui/src/host/tests.rs`](../../crates/ui/src/host/tests.rs):348), covers
  **three** paths, two of which are AVM2-injected, and is `#[ignore]`d behind
  "requires an installed Fallout 4 corpus" — so it does not run in CI.
- **Impact**: Not a demonstrated failure — I ran the ignored corpus tests and
  all three pass on real Fallout 4 data (see §4), which is why this is MEDIUM
  and framed as an unverified-surface finding rather than a bug. But the
  evidence base for "re-serializing every FO4 menu is lossless" is 2 menus out
  of 311, checked by a test nothing runs automatically. A silently corrupted
  glyph table or sprite is exactly the failure this shape produces, and it
  would surface as an unexplained rendering defect far from its cause.
- **Related**: SAFEUI-04 shares the coverage root cause.
- **Suggested Fix**: Move the injection to the same raw-tag strategy the
  navigator already uses (`raw_tag_records` + `write_swf_raw_tags`), splicing
  the adapter `DoABC2` and the patched root `DoABC` as opaque byte records so
  no untouched tag is ever re-encoded. Failing that, widen the corpus test to
  sweep all 311 SWFs asserting parse→inject→parse succeeds, and gate it on a
  data-present environment variable rather than `#[ignore]`.

#### SAFEUI-04: the Fallout 4 host-method catalog is validated against 3 of 311 menus, and an uncataloged method is a hard AVM2 error rather than a degraded call
- **Severity**: MEDIUM
- **Dimension**: 5 (content-driven failure modes)
- **Location**: [`crates/ui/src/catalog.rs`](../../crates/ui/src/catalog.rs):192-331 · [`crates/ui/src/avm2_host.rs`](../../crates/ui/src/avm2_host.rs):599-656, 989
- **Status**: NEW
- **Description**: The generated adapter installs **exactly one forwarder per
  catalog entry** onto the movie's `BGSCodeObj` object — 138 for
  `Fallout4Avm2`. Any `BGSCodeObj.Foo(...)` the menu makes that is not in the
  catalog therefore resolves to an absent property on a dynamic object, which
  in AVM2 is a call on `undefined` (`Error #1006`), not a no-op. The catalog is
  a hand-transcribed inventory of a third-party reconstruction of the vanilla
  ActionScript sources, and the guard against it being incomplete is
  `installed_fallout4_host_calls_are_cataloged`
  ([`crates/ui/src/avm2_host.rs`](../../crates/ui/src/avm2_host.rs):989) — which
  inspects **three** movies and is `#[ignore]`d.
- **Evidence**: the install loop emits one `SetProperty` per cataloged method
  and nothing else:
  ```rust
  // crates/ui/src/avm2_host.rs:755
  for (helper, property) in helper_multinames.iter().zip(&method_property_multinames) {
      install_ops.extend([
          Op::GetLocal { index: 2 },
          Op::GetLex { index: *helper },
          Op::SetProperty { index: *property },
      ]);
  }
  ```
  There is no catch-all forwarder, no dynamic-proxy interception, and no fallback that turns an
  unknown method into a recorded `Unknown` dispatch — the `unknown_methods`
  bookkeeping in [`crates/ui/src/host.rs`](../../crates/ui/src/host.rs) only ever sees calls that *reached* the bridge,
  which an absent property never does.
- **Impact**: 308 of 311 shipped menus are unverified against the catalog. Each
  miss aborts the executing ActionScript frame handler at the call site, so the
  symptom is a menu that renders but stops responding — with the true cause
  (one missing string in a Rust table) invisible from the failure.
- **Related**: SAFEUI-03.
- **Suggested Fix**: Give the adapter a fallback path so an uncataloged method
  degrades into a recorded `ScaleformHostDispatch::Unknown` returning `null`,
  rather than throwing — this is also the only way `unknown_methods()` can ever
  become a useful diagnostic. Separately, widen
  `installed_fallout4_host_calls_are_cataloged` from 3 movies to the full
  archive sweep.

---

### LOW

#### SAFEUI-05: `SwfPlayer::render` clears `dirty` and returns the buffer even when no frame was captured
- **Severity**: LOW
- **Dimension**: 3 (error handling on the readback path)
- **Location**: [`crates/ui/src/player.rs`](../../crates/ui/src/player.rs):247-283
- **Status**: NEW
- **Description**: Three failure paths — the `downcast_mut` returning `None`,
  `capture_frame()` returning `None`, and the `rgba.len() != pixel_buffer.len()`
  mismatch — all fall through to `self.dirty = false; Some(&self.pixel_buffer)`.
  The caller in `byroredux/src/main.rs`:675 treats a `Some` as a fresh frame and
  uploads it, so a failed capture is published as a real frame (stale content,
  or all-zero on the first frame) and, because `dirty` was cleared, is never
  retried until the next `tick`.
- **Evidence**: the early-return-free structure at
  [`crates/ui/src/player.rs`](../../crates/ui/src/player.rs):263-282 — the `if let
  Some(image)` has no `else`, and `self.dirty = false` sits outside every branch.
- **Impact**: **LOW because all three paths are currently unreachable**, and I
  confirmed each: the renderer is always the concrete
  `WgpuRenderBackend<TextureTarget>` by construction; *TextureTarget::new*
  always populates `buffer: Some(..)`, so *capture_frame* never returns `None`
  for this target type; and `pixel_buffer` and the target share one immutable
  `(width, height)` fixed at construction, so the length can never mismatch.
  The finding is that the code takes three branches whose stated purpose is to
  handle failure and then behaves identically to success.
- **Suggested Fix**: Return `None` (and leave `dirty` set) on any of the three
  paths, so a real failure re-tries rather than publishing a stale frame.

#### SAFEUI-06: the archive-preload stall branch is silent — an unsettled preload is indistinguishable from a hang
- **Severity**: LOW
- **Dimension**: 3 (error handling)
- **Location**: [`crates/ui/src/player.rs`](../../crates/ui/src/player.rs):199-214, 343-362
- **Status**: NEW
- **Description**: `tick` runs `drive_archive_preload` first when a navigator
  runtime exists. `Ok(false)` — meaning `MAX_ARCHIVE_PRELOAD_PASSES` (64)
  passes elapsed without `preload` settling — returns from `tick` with no log,
  no `resource_error`, and no state change. The same condition is a hard error
  at construction (`from_resource_provider` line 118 turns it into an
  `anyhow!`), but at tick time it is swallowed.
- **Impact**: A menu that triggers a mid-playback `ImportAssets` fetch that
  never settles freezes with no diagnostic; `resource_error()` reports `None`
  and `current_frame()` simply stops advancing. Retrying next frame is the
  right behaviour — the problem is that it is invisible.
- **Suggested Fix**: One-shot `log::warn!` on the first `Ok(false)`, with a
  consecutive-stall counter promoted into `resource_error` past some threshold.

#### SAFEUI-07: the UI overlay's viewport is pinned to the swapchain extent at scene setup and never follows a resize; `UiManager::close` has no caller
- **Severity**: LOW
- **Dimension**: 10 (overlay lifecycle)
- **Location**: [`byroredux/src/scene.rs`](../../byroredux/src/scene.rs):1139-1162 · [`crates/ui/src/lib.rs`](../../crates/ui/src/lib.rs):51-59, 211-216
- **Status**: NEW
- **Description**: `UiManager::new(w, h)` captures `ctx.swapchain_extent()`
  once, and `SwfPlayer::from_movie` fixes both the Ruffle viewport and the
  offscreen `TextureTarget` at that size. Nothing in `byroredux/src/main.rs`
  updates `ui_manager.width/height`, re-registers the UI texture, or resizes the
  Ruffle target when the swapchain is recreated. Separately, `UiManager::close`
  is dead code — nothing calls it, and even if it did, `App::ui_texture_handle`
  would keep the RGBA texture registered.
- **Impact**: Visual only. Explicitly **not** a Vulkan hazard: `update_rgba` is
  called with `ui.width`/`ui.height`, which are the same values the texture was
  registered with and the same values `pixel_buffer` was sized from, so the
  `assert_eq!(pixels.len(), width * height * 4)` in `Texture::from_rgba`
  ([`crates/renderer/src/vulkan/texture.rs`](../../crates/renderer/src/vulkan/texture.rs):79)
  cannot fire and no staging copy can over-read. The overlay simply stretches.
- **Suggested Fix**: On swapchain recreate, drive
  Ruffle's *set_viewport_dimensions* and re-register the UI texture, or document
  the fixed-extent behaviour. Give `close()` a caller or delete it.

#### SAFEUI-08: the generated adapter's constant pool is hand-indexed by position, with eight dead entries still emitted into every patched menu
- **Severity**: LOW
- **Dimension**: 4 (discipline — the no-`unsafe` analogue in this crate)
- **Location**: [`crates/ui/src/avm2_host.rs`](../../crates/ui/src/avm2_host.rs):516-580
- **Status**: NEW
- **Description**: `build_adapter_abc` builds a 27-entry string pool and a
  17-entry multiname pool as literal `vec![...]`, then refers to their members
  through seventeen hand-written `Index::new(N)` constants whose only tie to the
  pool is a trailing comment. Inserting, removing, or reordering a single pool
  entry silently shifts every later index and produces a **valid but wrong**
  ABC — the adapter would install forwarders under the wrong names, or register
  callbacks under the wrong strings, with no parse error anywhere. Eight of the
  entries are already dead: the strings `LoaderInfo`,
  `getLoaderInfoByDefinition`, `addEventListener`, `target`, `content`,
  `complete`, `flash.utils`, `setTimeout` and multinames 2, 6, 7, 8, 9, 15 are
  never referenced by any emitted op — leftovers from an abandoned
  `LoaderInfo`-based install strategy, shipped inside every patched FO4 menu.
- **Evidence**: I cross-checked all seventeen constants against the literal pool
  positions and **all are currently correct** (see §3) — this is a fragility and
  dead-weight finding, not a live mis-index. The structural test
  `generated_adapter_is_valid_abc_with_one_helper_per_method`
  ([`crates/ui/src/avm2_host.rs`](../../crates/ui/src/avm2_host.rs):934) counts ops
  and pins exactly one string index (`callback_names == [22]`); it would not
  catch a shift in the other sixteen.
- **Suggested Fix**: Build both pools through the existing `add_string` /
  `add_multiname` helpers so every index is derived rather than transcribed,
  and drop the eight unused entries.

---

## 3. Verified intact — PASS, not findings

These are the checks that *could* have been findings and were disproved. They
are recorded because `crates/ui` has never been audited before, so "we looked
and it holds" is itself new information.

### GPU readback and the wgpu↔Vulkan boundary

1. **The pixel readback is fully synchronized.** *TextureTarget::submit*
   appends the `copy_texture_to_buffer` encoder to the **same** `queue.submit`
   as the frame's command buffers; *capture_image* then does `map_async` →
   `device.poll(PollType::Wait { submission_index: None, timeout: None })` →
   blocking channel `recv` → `get_mapped_range`. `Wait` with a `None`
   submission index waits on *all* outstanding work. The CPU cannot read before
   the copy retires. **The prompt's primary hypothesis is disproved.**
2. **No mapped-memory validity window outlives the call.** *capture_image*
   drops the map and calls `buffer.unmap()` before returning; the crate never
   holds a `MappedRange` across a frame or a lock.
3. **No cross-API resource sharing exists.** `SwfPlayer::from_movie`
   ([`crates/ui/src/player.rs`](../../crates/ui/src/player.rs):137-155) creates its
   own wgpu instance, adapter, and device, entirely disjoint from the engine's
   `ash` `VulkanContext`. Frames cross the boundary as **CPU bytes**, never as a
   shared image or an exported memory handle. Consequence: there is no
   drop-ordering hazard between the Ruffle device and the engine device — the
   `AllocatorResource`-before-`VulkanContext` class of bug (Dimension 3 of the
   morning report) has no analogue here, because the two devices share nothing.
4. **The staging upload is bounds-guarded.** `TextureRegistry::update_rgba` →
   `Texture::from_rgba` asserts `pixels.len() == (width * height * 4) as usize`
   ([`crates/renderer/src/vulkan/texture.rs`](../../crates/renderer/src/vulkan/texture.rs):79)
   before any copy. `SwfPlayer::pixel_buffer` and the registered texture derive
   from one immutable `(width, height)`, so the assert is structurally
   unreachable rather than merely untriggered.
5. **`pixel_buffer`'s `(width * height * 4) as usize` cannot overflow `u32`.**
   The allocation happens *after* `TextureTarget::new` has rejected any
   dimension above `max_texture_dimension_2d`; at the 16384 ceiling the product
   is 2³⁰.

### Navigator sandbox — menu content cannot reach the filesystem or the network

6. **Non-`file` schemes are rejected.** An `ImportAssets` URL that resolves
   absolute (`http://…`) is refused by `archive_path_from_url`
   ([`crates/ui/src/navigator.rs`](../../crates/ui/src/navigator.rs):324-328).
7. **Percent-encoded traversal is caught.** `Url::join` normalizes literal
   `..` segments away and **clamps at the root**, so it cannot escape above
   `file:///`. It does **not** normalize `%2e%2e`, which is precisely what the
   `Component::ParentDir | Component::Prefix` rejection arm
   ([`crates/ui/src/navigator.rs`](../../crates/ui/src/navigator.rs):342-347)
   exists to catch — that arm is reachable and load-bearing, not dead defensive
   code. I initially wrote this up as a defense-in-depth gap and then
   disproved it.
8. **No network.** `connect_socket` unconditionally answers
   `ConnectionState::Failed`; `navigate_to_url` is a debug log and nothing else;
   non-GET fetches are refused. There is no HTTP backend behind
   `ScaleformResourceProvider` at all.
9. **Malformed SWF tag walking is bounded.** `raw_tag_records` reads headers via
   `.get(..)` (never a panicking slice), advances the cursor by at least 2 per
   iteration (no infinite loop), and length-checks with
   `cursor.checked_add(len).filter(|end| *end <= data.len())` — no overflow, no
   out-of-bounds, no hang on hostile input.
10. **The import-asset rewrite is deliberately fidelity-preserving.**
    `prepare_import_asset_swf` splices a two-byte synthetic `ShowFrame`
    (`0x40 0x00` — tag code 1, length 0) into the **raw** tag stream and emits
    through `write_swf_raw_tags`, so untouched tags pass through byte-for-byte.
    (This is the correct pattern that SAFEUI-03 asks the injection path to adopt.)

### AVM2 injection correctness

11. **Branch-crossing validation is sound.** `patch_root_constructor` rejects
    the patch outright if any control transfer crosses the insertion point. It
    correctly treats `Jump`/`If*` offsets as **end-relative** (`end + offset`)
    and `LookupSwitch` offsets as **start-relative** (`start + offset`),
    matching the AVM2 spec. Non-crossing branches need no rewrite because the
    relative distance between two same-side points is invariant under the
    insertion — this is correct reasoning, not an oversight.
12. **Exception-table offsets are rewritten.** `from_offset`, `to_offset` and
    `target_offset` are each shifted by the inserted length when `>=` the
    insertion point.
13. **The insertion point is after, not before, the `BGSCodeObj` initialization.**
    `insertion_offset` is computed as `body.code.len() - reader.as_slice().len()`
    *after* `read_op` consumed the `InitProperty`/`SetProperty`, so the bootstrap
    observes a populated object.
14. **All ABC constant-pool indexing is 1-based and correct.** `add_string` and
    `add_multiname` return `Index::new(len)` after pushing (position `len-1` →
    ABC index `len`), matching the AVM2 convention where index 0 means "any". I
    cross-checked every one of the seventeen hardcoded `Index::new(N)` constants
    in `build_adapter_abc` against the literal pools: **all seventeen are
    correct** (see SAFEUI-08 for the fragility that survives).
15. **The injected `FindPropStrict` resolves.** The patched constructor's
    `QName(Package(""), "__byro_fallout4_install")` and the adapter's script
    trait `qname(1, 16)` both land in the public namespace; the corpus test
    confirms the resolution empirically.
16. **The adapter tag is inserted *before* the root ABC** and carries
    `DoAbc2Flag::empty()` (not the lazy-initialize flag), so its script traits are in
    the domain before the patched constructor references them.
17. **Every non-test `expect`/`unreachable!` in the crate is genuinely
    unreachable.** The two `unreachable!("root ABC index must reference an ABC
    tag")` arms re-match an index that was produced by an ABC-tag match; the two
    `expect("… requires a host-object profile")` sites are guarded by an earlier
    `catalog.host_object().is_none()` early return. `abc.method_bodies[body_index]`
    is direct-indexed only after a `.get(body_index).ok_or_else(…)` on the same
    index.

### Host bridge

18. **No `RefCell` borrow is ever held across an AVM re-entry.** `record_call`
    clones the `Rc<ResponseHandler>` out of a temporary borrow that ends at the
    statement, then invokes the handler with **no** borrow active; and
    `BridgeProvider::call_method` calls `record_call` to completion before
    re-entering ActionScript via the `respond` callback. A response handler that
    calls back into `register_method` / `set_response` / `drain_calls`, and a
    menu whose `respond` handler issues another `ExternalInterface.call`, both
    work — no `already mutably borrowed` panic. This is a non-obvious property
    and the code gets it right.
19. **`ScaleformHostCatalog::find`'s binary search is safe.** Both static tables
    were verified programmatically as byte-wise sorted with no duplicates — 74
    Skyrim entries, 138 Fallout 4 entries, matching the counts pinned in
    `skyrim_catalog_is_pinned_sorted_and_profile_specific`.
20. **`numeric_request_id` rejects non-integral, negative, infinite and NaN
    request IDs** before the `as u64` cast, so the Skyrim `GameDelegate`
    normalization cannot produce a saturating-cast surprise.
21. **`Drop for SwfPlayer` cannot double-panic.** It uses `if let Ok(mut player)
    = self.player.lock()` rather than `unwrap`, so a drop during unwinding from
    a panic that poisoned the player mutex degrades to skipping the destroy
    callback instead of aborting. (The ~10 `lock().unwrap()` sites elsewhere in
    [`crates/ui/src/player.rs`](../../crates/ui/src/player.rs) are single-threaded and only poisonable by a panic that has
    already terminated the process.)

### `unsafe` census

22. **`crates/ui/src` contains zero real `unsafe` blocks or `unsafe fn`.** The
    single grep hit is prose — a log string at
    [`crates/ui/src/navigator.rs`](../../crates/ui/src/navigator.rs):344 reading
    `"unsafe Scaleform archive path resolved from URL"`. This independently
    confirms correction **S-3** in the morning report (SKILL.md lines 21-22 list
    `crates/ui` as carrying "one `unsafe`"; it does not). The Dimension-4
    unsafe-discipline sweep has **nothing to audit** in this crate.

---

## 4. Tests run

```
cargo test -p byroredux-ui
    16 passed · 0 failed · 3 ignored

cargo test -p byroredux-ui -- --ignored --test-threads=1
    3 passed · 0 failed
      avm2_host::tests::installed_fallout4_host_calls_are_cataloged
      host::tests::installed_fallout4_representative_menus_obey_host_object_lifecycle
      host::tests::installed_skyrim_hudmenu_loads_with_avm1_profile
```

The three `#[ignore]`d corpus tests were run manually against the installed
Fallout 4 and Skyrim Special Edition data and **all pass** — real
`hudmenu.swf` / `pipboymenu.swf` survive adapter injection, install their
forwarders, fire the readiness and destruction callbacks, and report no
uncataloged methods; `atomiccommand.swf` is correctly classified `NotPresent`.
This is what keeps SAFEUI-03 and SAFEUI-04 at MEDIUM rather than HIGH: the
mechanism is unproven at scale, not observed broken.

Corpus measurement, for the coverage claims: `Fallout4 - Interface.ba2` is a
BTDX v8 GNRL archive holding **1101 files, 311 of them `.swf`**.

---

## 5. Prioritized fix order

1. **SAFEUI-01** (HIGH) — drain or bound the host-call queue. Unbounded growth
   with a completely unwired consumer; also the cheapest fix here.
2. **SAFEUI-02** (MEDIUM) — `saturating_add(2)`. One line, removes a
   crash-from-content path, no behavioural risk.
3. **SAFEUI-04** (MEDIUM) — add an uncataloged-method fallback so 308 unverified
   menus degrade instead of throwing.
4. **SAFEUI-03** (MEDIUM) — move injection to raw-tag splicing; larger change,
   do it after 2 and 3, or at minimum promote the corpus test out of `#[ignore]`.
5. **SAFEUI-05/06** (LOW) — error-path honesty in [`crates/ui/src/player.rs`](../../crates/ui/src/player.rs); both are small.
6. **SAFEUI-08** (LOW) — derive the adapter's pool indices, drop the eight dead
   entries.
7. **SAFEUI-07** (LOW) — resize handling / `close()` wiring; visual only.

---

## 6. Coverage statement

Complete for `crates/ui/src` — all eight source files plus
[`crates/ui/src/host/tests.rs`](../../crates/ui/src/host/tests.rs) were
read in full, and every engine-side consumer of the crate was traced. The
vendored Ruffle checkout was read only as evidence for the readback-sync
(§3.1-2) and stack-frame (SAFEUI-02) questions; it is a third-party dependency
and was not audited as first-party code.

Not covered, deliberately: everything outside the UI surface. This run does
**not** supersede or repeat
[`docs/audits/AUDIT_SAFETY_2026-08-12.md`](AUDIT_SAFETY_2026-08-12.md), which
remains the general safety report for today. Of the six un-owned subsystems in
`.claude/commands/_audit-common.md`, this run covers exactly one — Scaleform/SWF UI. The other
five (physics/PHYSAL, character/CHARAL, plugin/ESM records, FSR3, and the
brand-new `crates/mod-runtime` trust boundary) remain unexamined by any
safety-class audit.

---

*Report generated 2026-08-12 · `/audit-safety` (UI-focused preset, `ui-deep` suite)*
