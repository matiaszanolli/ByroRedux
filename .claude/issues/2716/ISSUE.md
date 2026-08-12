# #2716: Injected AVM2 bootstrap reserves operand-stack headroom with `.max(2)`, a no-op on every real constructor

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

---
**Source**: `docs/audits/AUDIT_SAFETY_UI_2026-08-12.md` (finding `SAFEUI-02`)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan/wgpu objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (prefer a default-suite test, not `#[ignore]`d)

