# SCR-D3-02: decompile/mod.rs's pipeline docstring is first-commit-era, wrong pass order

**Issue**: #3019
**Severity**: LOW
**Dimension**: 3 — Decompiler
**Labels**: `low,scripting,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_SCRIPTING_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-16.md` (Dimension 3 — Decompiler).

**Location**: `crates/pex/src/decompile/mod.rs`:7-14
**Status note**: NEW — distinct from #2542, which is `docs/feature-matrix.md`.

## Description

`decompile/mod.rs`'s pipeline docstring is **first-commit-era** and states the wrong pass order: it describes pass 1 as "this commit" and passes 2–4 as "(next)", although all four shipped long ago.

## Evidence

```rust
//! Pipeline, built up across commits:
//!
//! 1. **`cfg`** — basic-block control-flow graph (this commit). …
//! 2. *opcode → node-tree lifting + copy-propagation* (next).
//! 3. *control-flow + boolean-operator reconstruction* (next).
//! 4. *lower the node tree → `byroredux_papyrus::ast::Script`* (next).
```

Re-verified 2026-08-17. The live pipeline is the five-phase CFG → node-lift+copy-prop → control-flow recon → AST lower+fidelity gate → short-circuit booleans described in `.claude/commands/_audit-common.md`, and the module directory (`cfg, lift, control_flow, lower, boolean, node, event_names`) reflects that.

## Impact

The module's own entry-point documentation misdescribes both the phase count and their status to anyone reading the decompiler for the first time — including the next auditor. The docstring is the natural orientation point for a five-phase pipeline that is otherwise hard to follow.

## Suggested Fix

Rewrite the docstring to the shipped five-phase order, drop the "(this commit)" / "(next)" scaffolding, and name the modules that implement each phase.

## Related

- #2542 (the same M47.2 pass-order error, in `docs/feature-matrix.md` — different file, same underlying drift)
- #3017 (SCR-D3-2026-08-16-01 — the same module's coverage gap)

## Completeness Checks
- [ ] **SIBLING**: `docs/engine/scripting.md` and `m47-2-design.md` checked for the same stale order
- [ ] **PHASE-COUNT**: The docstring names five phases, matching the module layout
- [ ] **NO-COMMIT-SCAFFOLD**: "this commit" / "(next)" language removed
- [ ] **PATH-GATE**: `.claude/commands/_audit-validate.sh` still passes

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3019 --json state` when live state is needed.*
