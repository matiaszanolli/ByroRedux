# PERF-D0-01: two of audit-performance SKILL.md's own Dimension checklists now cite superseded constants

**Issue**: #3143 — https://github.com/matiaszanolli/ByroRedux/issues/3143
**Labels**: `low,performance,documentation`
**Filed**: 2026-08-20 · comprehensive audit suite
**Report**: `docs/audits/AUDIT_PERFORMANCE_2026-08-20.md`

---

**Severity**: LOW
**Dimension**: Telemetry & Origin Cost / skill hygiene
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-20.md` (PERF-D0-01)

## Location

- `.claude/commands/audit-performance/SKILL.md` — Dimension 7 (`STREAMING_APPLY_BUDGET` "4 ms", `:126`) and Dimension 5 (`froxel_xy_divisor` "default 12", `:109`)
- Ground truth: `byroredux/src/app_step.rs:33` (`Duration::from_millis(16)`) and `crates/renderer/src/vulkan/upscaling.rs:115` (`froxel_xy_divisor: 4`)

## Description

Two numeric claims in this skill's dimension text no longer match the code an auditor is told to "verify intact":

**1. Dimension 7 states `STREAMING_APPLY_BUDGET` is 4 ms.** `687e0a67` (2026-08-16, inside this delta) raised it to **16 ms**, deliberately and with a documented rationale at `app_step.rs:22-32` (*"Four milliseconds proved counterproductive in the FO4 boundary gate"*). The 2026-08-16 audit verified 4 ms correctly; the change landed immediately after.

**2. Dimension 5 states the volumetrics `froxel_xy_divisor` default is 12.** It is **4** (`VolumetricsConfig::default`), and has been since Session 62 — a **9× difference in froxel count**.

## Evidence

Confirmed at HEAD:
```
byroredux/src/app_step.rs:33:    const STREAMING_APPLY_BUDGET: Duration = Duration::from_millis(16);
crates/renderer/src/vulkan/upscaling.rs:115:            froxel_xy_divisor: 4,
crates/renderer/src/vulkan/upscaling.rs:417:        assert_eq!(config.froxel_xy_divisor, 4);
```
against `.claude/commands/audit-performance/SKILL.md:126` (*"`STREAMING_APPLY_BUDGET` (4 ms, `byroredux/src/app_step.rs`)"*) and `:109` (*"render extent / `froxel_xy_divisor` (default 12)"*).

Both live values are **documented design decisions, not drift in the code** — the skill text is what drifted.

## Impact

An auditor following Dimension 7 literally would report the 16 ms budget as a **regression** (it is not), wasting a finding slot and casting doubt on a deliberate tuning decision.

An auditor following Dimension 5 would size the froxel grid **9× low** — which is exactly what made the Dimension 3 volumetrics VRAM ledger error easy to under-weight in this very sweep.

This is the same failure mode #2691 (`PERF-DOC`, CLOSED) was filed for.

## Suggested Fix

Update both figures in `.claude/commands/audit-performance/SKILL.md` and re-run `.claude/commands/_audit-validate.sh`. **Prefer phrasing that names the constant without transcribing its value**, so the next tuning change cannot invalidate the skill text again.

## Related

- #2691 (CLOSED — same failure mode)
- #3063 (`PERF-D0-01` from the 08-16 sweep, OPEN — the bench-of-record staleness; a **separate** item, now 369+ commits out of its own 30-commit gate)
- The `/audit-renderer` skill carries the same stale `froxel_xy_divisor` figure (filed separately) — fix both together

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — sweep every audit SKILL.md for transcribed constants that have a live source of truth
- [ ] **TESTS**: `.claude/commands/_audit-validate.sh` passes after the edit
