# SCR-D3-NEW-01: boolean.rs::collapse doesn't special-case operand_key == rejoin_key (absorbed by fail-closed catch-all, not exploitable)

**Labels**: low, tech-debt, documentation

**Severity**: LOW (informational / defense-in-depth documentation)
**Dimension**: Decompiler Control-Flow / Boolean / Lower
**Untrusted-Input**: Yes (only reachable via hand-crafted/adversarial `.pex`, never real compiler output)
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-07-16.md`

## Location
`crates/pex/src/decompile/boolean.rs:166-226` (`collapse`), consumed by `control_flow.rs:96-210`

## Description
A `CodeBlock` with equal `on_true`/`on_false` targets makes `operand_key == rejoin_key`; `collapse` unconditionally removes `operand_key` before looking up `rejoin`, so the lookup returns `None` and `current`'s edges are never updated, leaving a stale conditional block. Traced by hand and confirmed the fallout is safe: `control_flow::reconstruct` later hits the final `else` arm and fails closed (`ControlFlowFailed`) rather than panicking, hanging, or emitting a wrong AST.

Verified current: `collapse` in `crates/pex/src/decompile/boolean.rs` has no explicit guard for `operand_key == rejoin_key`; `control_flow.rs`'s fail-closed catch-all is unchanged.

## Impact
None observed. Filed so a future audit doesn't have to re-derive the trace from scratch — the fail-closed catch-all (#1732) is now load-bearing for two independent gaps, worth knowing before it's next touched.

## Suggested Fix
None required. Optional hardening: an explicit `if operand_key == rejoin_key { return Ok(false); }` guard would make the impossibility self-evident in the code.

## Completeness Checks
- [ ] **TESTS**: Optional — a regression test asserting the degenerate case still fails closed via `ControlFlowFailed` rather than panicking
