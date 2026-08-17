# SCR-D3-01: the .psc-vs-.pex fidelity gate does not execute in a default cargo test

**Issue**: #3017
**Severity**: MEDIUM
**Dimension**: 3 — Decompiler
**Labels**: `medium,scripting,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_SCRIPTING_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-16.md` (Dimension 3 — Decompiler: Control-Flow / Boolean / Lower).

**Location**: `crates/pex/tests/r5_fidelity.rs` (`#[ignore]`) · `crates/scripting/tests/pex_recognize_e2e.rs`:37, :80, :120 (all `#[ignore]`) · `crates/pex/examples/pex_corpus_smoke.rs`:145
**Status note**: NEW — the "no parity test" issue #1740 was closed by *adding* the ignored test; that it never runs is a distinct, unfiled gap.

## Description

The only fidelity instrument that executes in a default `cargo test` is `recognizes_da10_and_reproduces_hand_builder`, and **it never calls `decompile_script`** — it runs the `.psc` frontend.

All four tests that exercise the decompiler end-to-end are `#[ignore]`d on Skyrim SE game data.

The corpus smoke harness is not a substitute: `Ok(Ok(_)) => decompiled_ok += 1` throws away the `Script` without any shape check, so **a decompile that succeeds with a wrong AST scores as a success**.

A default `cargo test` therefore has **zero** coverage of "does the decompiler produce the right tree".

## Evidence

Re-verified 2026-08-17: `crates/pex/tests/r5_fidelity.rs`'s module docs state *"Opt-in / `#[ignore]`d: it needs the Skyrim SE script archive"*, with the documented invocation `cargo test -p byroredux-pex --test r5_fidelity -- --ignored --nocapture`.

## Impact

The `.pex` decompiler is a five-phase pipeline over untrusted binary input feeding the AST→ECS recognizer chain. Its correctness is unverified by any test that runs by default, and the one harness that does run scores success on shape-blind criteria.

The report notes **a checked-in fixture is already feasible** — the blocker is not licensing or size, it is that nobody has extracted one.

## Suggested Fix

Check in a small `.pex` fixture and a matching expected AST so at least one end-to-end fidelity assertion runs in a default `cargo test`. Keep the game-data tests `#[ignore]`d for breadth, but stop relying on them for baseline coverage.

Separately, make `pex_corpus_smoke` assert something about the returned `Script` rather than discarding it.

## Related

- #1740 (closed by adding the ignored test; this is the residual)
- #3014 (SCR-D8-2026-08-16-04 — the same green-by-construction shape in `crates/hkx`)

## Completeness Checks
- [ ] **DEFAULT-RUN**: At least one decompiler fidelity assertion runs without game data
- [ ] **SHAPE-CHECK**: `pex_corpus_smoke` inspects the `Script` instead of discarding it
- [ ] **SIBLING**: The three `pex_recognize_e2e` tests reviewed for a checked-in-fixture equivalent
- [ ] **TESTS**: The new fixture fails on a deliberately broken lowering pass

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3017 --json state` when live state is needed.*
