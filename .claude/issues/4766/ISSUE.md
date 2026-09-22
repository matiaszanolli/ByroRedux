# TD9-2026-09-22-01: adaptation_alpha_is_a_saturating_ramp silently dropped from the test registry — duplicate #[test] orphaned by Fix #4590

**Labels**: medium, tech-debt, bug, test-gap

Filed via /audit-publish from docs/audits/AUDIT_TECH_DEBT_2026-09-22.md.

**Severity**: MEDIUM · **Dimension**: 9 — Test Hygiene
**Location**: `crates/renderer/src/vulkan/exposure.rs:296-347`

**Status**: NEW
**Verified against**: HEAD `c3f298a24` — re-read the file and re-ran `cargo test -p byroredux-renderer --lib exposure:: -- --list` live; `per_slot_adaptation_runs_at_the_authored_tau` lists twice, `adaptation_alpha_is_a_saturating_ramp` is absent from the registry. No commit in `ee6d3fb39..c3f298a24` touched this file.

## Description

`5fea8f437` (Fix #4590, "adapt each exposure slot over its real update interval") inserted a new test function's doc-comment + `#[test]` attribute between the pre-existing doc-comment and `#[test]` belonging to `adaptation_alpha_is_a_saturating_ramp` and that function's signature. The new function (`per_slot_adaptation_runs_at_the_authored_tau`) ends up with a duplicated `#[test]` (harmless) and a doc comment that describes the OLD function, not itself; `adaptation_alpha_is_a_saturating_ramp` loses its `#[test]` entirely and becomes dead code.

Same-night precedent, fixed elsewhere, not here: `ad96380be` fixed the identical duplicate-`#[test]` shape in `crates/pex/src/decompile/cfg.rs` 19 minutes before `5fea8f437` reproduced it here; no equivalent cleanup followed.

## Impact

The disabled test pinned `adaptation_alpha`'s basic shape (0 at dt=0, monotonic, saturates at 1, non-positive tau snaps) — a property the new #4590 test does not independently re-check. A future change to `adaptation_alpha` could silently break monotonicity/boundedness with all other exposure tests still green.

## Related

`ad96380be` (same class, same evening, fixed in a sibling file), #4590 (introduced this instance), the CI-gate blind spot (`#[cfg(test)]` code invisible to the non-`--all-targets` clippy gate).

## Suggested Fix

Delete the duplicate `#[test]` + orphaned doc comment at line ~296-305; restore `#[test]` immediately above `fn adaptation_alpha_is_a_saturating_ramp()`. Trivial.

Source: docs/audits/AUDIT_TECH_DEBT_2026-09-22.md (TD9-2026-09-22-01)
