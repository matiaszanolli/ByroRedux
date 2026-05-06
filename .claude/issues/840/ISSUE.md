# SK-D5-NEW-04: Aggregator warning text references closed #615 — log noise routes investigators to a dead thread

## Description

The per-NIF aggregator at `crates/nif/src/lib.rs:563` emits the literal string "_per-block detail at debug level — under-/over-consume bugs tracked in #615_" on every NIF that hits the per-block `consumed != block_size` path. **#615 is closed.** Future maintainers chasing the warning land on a sealed issue with no actionable child links.

## Location

`crates/nif/src/lib.rs:563`

## Evidence

67+ instances of the string in a single Meshes0 parse run. `gh issue view 615` confirms the issue is CLOSED.

## Impact

Cosmetic / docs hygiene; routes investigators to a dead ticket. Compounds with the by-design WARN noise from #837 and #836 — investigators chasing real drift (e.g. BSLODTriShape, #838) get sent to a sealed issue.

## Suggested Fix

Replace the static `#615` reference with either:
- The umbrella tag introduced for #837 (BSLagBoneController) once that lands; or
- Just remove the issue number — the warning already carries the type name + `RUST_LOG=debug` instructions, which is enough for an investigator to find the actual offending parser.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Grep `crates/nif/` for other closed-issue references in log strings (e.g. "tracked in #NNN") that may have suffered the same fate
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: N/A — log-string change

## Source Audit

`docs/audits/AUDIT_SKYRIM_2026-05-05_DIM5.md` — SK-D5-NEW-04