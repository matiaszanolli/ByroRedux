# #3725 — ESM-2026-08-30-D3-03: parse_esm_with_load_order's docstring says the CLI is single-plugin only; it has been multi-plugin since M46.0

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: LOW (doc-rot) · **Dimension**: FormID & Load Order
**Location**: `crates/plugin/src/esm/records/mod.rs` (`parse_esm_with_load_order` docstring, ~:165-175)
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D3-03)

## Description

The docstring states, present tense:

> *"The current CLI entry point (`--esm <path>`) only wires a single plugin, so both paths produce the same output for vanilla content. The multi-plugin wiring is tracked as follow-up work — this function exists so downstream code can opt in without another parse-layer refactor when the CLI grows multi-plugin support."*

This has been false since M46.0:

- `--master` is repeatable
- `byroredux/src/cell_loader/load_order.rs:361` builds a real `FormIdRemap` per plugin and `:369` passes it to **this very function**
- `CLAUDE.md`'s usage section documents `--master Skyrim.esm --master Update.esm --esm Dawnguard.esm`

## Impact

The stale note is precisely what makes a reader dismiss the remap as not-yet-live — the reasoning that leaves the un-swept remap sites (the HIGH `parse_armo`/`parse_arma`/`parse_race` finding and its allowlist sibling) unfixed.

## Suggested Fix

Replace the paragraph with the live state: multi-plugin is wired, `load_order.rs` is the producer, and every embedded FormID read in `records/` must go through `remap_fid`.

## Completeness Checks
- [ ] **SIBLING**: `docs/engine/plugin-loading.md` and the `/audit-esm` SKILL checked for the same stale "single-plugin only" premise
- [ ] **TESTS**: n/a (documentation)
