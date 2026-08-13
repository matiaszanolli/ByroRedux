# #2715 — UI overlay rewrites a bindless descriptor on the previous, still-pending frame slot

**Severity**: HIGH · **Domain**: renderer (byroredux-renderer) + binary (byroredux)
**Location**: `byroredux/src/main.rs:662-695`, `crates/renderer/src/texture_registry.rs:1460-1488`, `crates/renderer/src/vulkan/context/draw.rs:1460`

`TextureRegistry::apply_descriptor_write` writes the new descriptor immediately into `bindless_sets[self.current_slot]`, justified by "no submitted command buffer can be reading `bindless_sets[current_slot]` concurrently" — true only while inside `draw_frame` after `begin_frame` sets `current_slot`. But `main.rs`'s UI tick/render/upload block (`update_rgba`) runs *before* `ctx.draw_frame(...)` is called, while `current_slot` still names the *previous* iteration's frame, whose command buffer is in the pending state (`MAX_FRAMES_IN_FLIGHT == 2`). The bindless layout uses `PARTIALLY_BOUND | UPDATE_AFTER_BIND` (not `UPDATE_UNUSED_WHILE_PENDING`), and the UI texture index is dynamically used by that pending command buffer (`ui.frag` samples it every frame the overlay is up) — so this is a live descriptor-in-use write (`VUID-vkUpdateDescriptorSets-None-03047`), invisible without validation layers since the content is still correct by construction (the current frame's set gets the same payload through `pending_set_writes`).

**Suggested fix**: either (a) make `apply_descriptor_write` queue for *all* slots (including `current_slot`) when a `recording: bool` latch (set by `begin_frame`, cleared at submit) is false, or (b) cheaper: move the UI tick/render/upload block from `main.rs` to inside `draw_frame` after `texture_registry.begin_frame`.

---

# #2716 — Injected AVM2 bootstrap reserves operand-stack headroom with `.max(2)`, a no-op on every real constructor

**Severity**: MEDIUM · **Domain**: ui (byroredux-ui)
**Location**: `crates/ui/src/avm2_host.rs:467-481` (`patch_root_constructor`)

The injected 3-op bootstrap (`FindPropStrict` +1, `GetLocal` +1 → peak `D+2`, `CallPropVoid`) needs `max_stack >= D + 2` where `D` is the stack depth at the insertion point. The code does `body.max_stack = body.max_stack.max(2)`, which only guarantees `max_stack >= 2` — a no-op on every real constructor (which already declares ≥2). Correct today only because the compiler happens to emit the `BGSCodeObj` init at statement level (`D == 0`). Ruffle's AVM2 verifier does not reconcile declared `max_stack` against actual, so an overflow is a Rust index-out-of-bounds **panic** inside `player.tick()` (contained to the stack subslice, not a silent OOB write — hence MEDIUM not CRITICAL) for a lifecycle constructor that initializes `BGSCodeObj` inside an expression (`D > 0`) rather than a bare statement.

**Suggested fix (one line)**: `body.max_stack = body.max_stack.saturating_add(2);` — unconditionally correct for any `D`.

---

# #2717 — Every FO4 AVM2 menu is round-tripped through a full parse_swf → write_swf re-serialization

**Severity**: MEDIUM · **Domain**: ui (byroredux-ui)
**Location**: `crates/ui/src/avm2_host.rs:54-137` (`inject_host_object_adapter`)

`inject_host_object_adapter` fully parses every SWF tag into the typed `swf` crate representation, mutates two tags, and re-serializes the *entire* movie with `write_swf` — every font/bitmap/sprite/sound/shape tag is decoded and re-encoded, not copied. The sibling rewrite in `navigator.rs::prepare_import_asset_swf` deliberately avoids this by walking `raw_tag_records` and emitting through `write_swf_raw_tags` so untouched tags pass through byte-for-byte. Evidence base for "lossless" today is 2 of 311 FO4 menus, checked only by an `#[ignore]`d test gated behind an installed-corpus requirement that doesn't run in CI. Not a demonstrated failure (issue author ran the ignored corpus tests — all 3 pass on real data) — framed as an unverified-surface finding.

**Suggested fix**: move the injection to the raw-tag-record strategy (splice `DoABC2` / patched root `DoABC` as opaque byte records), OR — failing that — widen the corpus test to sweep all 311 SWFs (parse→inject→parse) and gate on a data-present env var instead of `#[ignore]`.
