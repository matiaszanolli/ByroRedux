# #1815: SCR-D2-01: Decompiler boolean-collapse pass has no recursion-depth cap — stack overflow from untrusted .pex

**Severity**: HIGH
**Dimension**: Decompiler Control-Flow / Boolean / Lower
**Untrusted-Input**: Yes
**Location**: `crates/pex/src/decompile/boolean.rs:110-145` (`BoolPass::rebuild`)
**Status**: NEW

**Description**: `BoolPass::rebuild` recurses on `self.rebuild(block.on_true(), block.on_false)` / `self.rebuild(block.on_false, block.on_true())` for each conditional block whose fall-through edge is a short-circuit operand. Unlike every other decompiler tree walk that consumes untrusted structure, it carries **no depth guard**: `control_flow.rs::reconstruct` was capped with `MAX_REBUILD_DEPTH = 1024` (#1729 / SAFE-2026-06-23-02), and the `.psc` parser has `MAX_EXPR_DEPTH` / `MAX_STMT_DEPTH`. The boolean pre-pass — which runs on the same untrusted CFG (`lower.rs::decompile_body` line 216) *before* the capped control-flow pass — was missed.

**Evidence**: `grep -c depth crates/pex/src/decompile/boolean.rs` → `0`. The recursion at lines 127/131 has no `depth` parameter and no `MAX_*` check; `mod.rs::DecompileError` has a `RecursionLimit` variant used only by `control_flow.rs`.

**Impact**: A hostile/corrupt `.pex` in a modded `--scripts-bsa` archive with deeply-chained `&&`/`||` short-circuit conditionals recurses one frame per nesting level with no bound. A sufficiently deep chain overflows the stack — an **uncatchable abort** (`catch_unwind` does not catch a stack overflow), taking the whole engine down during cell load. Same bug class as the already-fixed #1729, one pass upstream.

**Related**: #1729 (the control-flow-pass sibling, fixed); SCR-D5-NEW-02 (companion finding in this same report — see linked issue for the missing `catch_unwind` net on the live decompile boundary).

**Suggested Fix**: Thread a `depth: usize` through `BoolPass::rebuild` (and `collapse`, which calls back into `rebuild`), return `DecompileError::RecursionLimit` past the same `MAX_REBUILD_DEPTH` cap the control-flow pass uses. Add a pathological-nesting regression test mirroring `control_flow`'s `rebuild_rejects_excessive_recursion_depth`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other decompiler tree walks — `control_flow.rs`, `lift.rs`, `.psc` parser stmt/expr)
- [ ] **TESTS**: A regression test pins this specific fix

---

# #1816: SCR-D5-NEW-02: translate_pex decompiles untrusted .pex without catch_unwind — a decompiler panic aborts the cell loader

**Severity**: MEDIUM
**Dimension**: Recognizer-Chain Soundness
**Untrusted-Input**: Yes
**Location**: `crates/scripting/src/translate/mod.rs:88-110` (`translate_pex`)
**Status**: NEW

**Description**: `translate_pex` handles `parse`/`decompile_script` **`Err`** gracefully (`log::debug` + `None`) but does not guard against a **panic** from `decompile_script`. The decompiler carries internal `.expect()`/`.unwrap()` invariants (`cfg.rs::split_block` `"split target block exists"`, `control_flow.rs` `"conditional block has a condition"`, `lift.rs` `"non-final node has a result"`, the boolean-pass `.expect`s). The corpus-smoke harness wraps `decompile_script` in `std::panic::catch_unwind` *specifically because* a bad `.pex` can trip one; the live attach boundary omits that net.

**Evidence**: `grep -c catch_unwind crates/scripting/src/translate/mod.rs` → `0`; `crates/pex/examples/pex_corpus_smoke.rs:144` wraps the same call in `catch_unwind`. The module doc claims "never a panic escaping into the cell loader" — true for `Err`, not for `panic!`.

**Impact**: A hostile/corrupt `.pex` that trips a decompiler `expect` panics through `attach_vmad_scripts` and aborts cell load. Vanilla content is clean (0/26 640 corpus panics), so blast radius is modded/corrupt archives — hence MEDIUM not HIGH. (A stack overflow via SCR-D2-01 is *not* caught by `catch_unwind` regardless; that path stays HIGH.)

**Related**: SCR-D2-01 (companion finding in this same report — the un-capped `boolean.rs` recursion); the `translate_pex_on_*_is_a_clean_none` tests cover `Err`, not panic.

**Suggested Fix**: Wrap the `decompile_script` call in `std::panic::catch_unwind(AssertUnwindSafe(...))`, mapping a caught panic to the same `log::debug` + `None` the `Err` arm uses — matching the corpus harness's own defense. Add a garbage-`.pex`-that-panics-decompile regression once such an input is characterized.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (corpus-smoke harness already does this; confirm no other live decompile call sites are missing the net)
- [ ] **TESTS**: A regression test pins this specific fix
