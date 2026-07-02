# SCR-D5-NEW-02: translate_pex decompiles untrusted .pex without catch_unwind — a decompiler panic aborts the cell loader

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1816
**Source report**: docs/audits/AUDIT_SCRIPTING_2026-07-02.md
**Labels**: medium, safety, bug

- **Severity**: MEDIUM
- **Dimension**: Recognizer-Chain Soundness
- **Untrusted-Input**: Yes
- **Location**: `crates/scripting/src/translate/mod.rs:88-110` (`translate_pex`)
- **Status**: NEW

**Description**: `translate_pex` handles `parse`/`decompile_script` **`Err`** gracefully (`log::debug` + `None`) but does not guard against a **panic** from `decompile_script`. The decompiler carries internal `.expect()`/`.unwrap()` invariants (`cfg.rs::split_block` `"split target block exists"`, `control_flow.rs` `"conditional block has a condition"`, `lift.rs` `"non-final node has a result"`, the boolean-pass `.expect`s). The corpus-smoke harness wraps `decompile_script` in `std::panic::catch_unwind` *specifically because* a bad `.pex` can trip one; the live attach boundary omits that net.

**Evidence**: `grep -c catch_unwind crates/scripting/src/translate/mod.rs` → `0`; `crates/pex/examples/pex_corpus_smoke.rs:144` wraps the same call in `catch_unwind`. The module doc claims "never a panic escaping into the cell loader" — true for `Err`, not for `panic!`.

**Impact**: A hostile/corrupt `.pex` that trips a decompiler `expect` panics through `attach_vmad_scripts` and aborts cell load. Vanilla content is clean (0/26 640 corpus panics), so blast radius is modded/corrupt archives — hence MEDIUM not HIGH. (A stack overflow via SCR-D2-01 is *not* caught by `catch_unwind` regardless; that path stays HIGH.)

**Related**: SCR-D2-01 (#1815); the `translate_pex_on_*_is_a_clean_none` tests cover `Err`, not panic.

**Suggested Fix**: Wrap the `decompile_script` call in `std::panic::catch_unwind(AssertUnwindSafe(...))`, mapping a caught panic to the same `log::debug` + `None` the `Err` arm uses — matching the corpus harness's own defense. Add a garbage-`.pex`-that-panics-decompile regression once such an input is characterized.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (corpus-smoke harness already does this; confirm no other live decompile call sites are missing the net)
- [ ] **TESTS**: A regression test pins this specific fix
