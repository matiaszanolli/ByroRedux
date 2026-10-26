# EXT-D4-2026-10-26-03: Duplicate `#[cfg(test)]` attribute on `mod cloud_scroll_vectors_tests`

Labels: low,terrain-exterior,tech-debt,bug
Source: docs/audits/AUDIT_EXTERIOR_2026-10-26.md

- **Severity**: LOW
- **Dimension**: 4 (tech-debt)
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Location**: `byroredux/src/systems/weather.rs:1545-1546`
- **Status**: NEW (introduced by 4f2596bf6, which inserted `cloud_drift_cap_tests` plus a stray `#[cfg(test)]` above the existing one)
- **Description**: Two consecutive `#[cfg(test)]` lines gate one module. Harmless at runtime, but it trips `clippy::duplicated_attributes`, and it is a merge artefact that suggests the hunk was spliced by hand.
- **Evidence**: `sed -n 1545,1547p` -> `#[cfg(test)]` / `#[cfg(test)]` / `mod cloud_scroll_vectors_tests {`.
- **Impact**: None functional; adds one lint site to the workspace clippy backlog.
- **Related**: 4f2596bf6.
- **Suggested Fix**: Delete one of the two lines.

_Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-26.md` (HEAD 3e6f6b66a). Static trace; not confirmed on screen unless stated._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader copies, other sun/lighting sites, other rings)
