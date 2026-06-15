# AUD-2026-06-14-03: Crate Future-phases docstring lists Phase 4/5/6 as unshipped, colliding with shipped-phase sections

- **Issue**: #1614
- **Severity**: LOW
- **Labels**: low, tech-debt, documentation
- **Dimension**: Manager Lifecycle (docstring integrity)
- **Location**: `crates/audio/src/lib.rs:107-113`
- **Source report**: `docs/audits/AUDIT_AUDIO_2026-06-14.md`

## Description
The module docstring documents Phases 4/5/6 as shipped ("this commit", lines 65-105), but the trailing `# Future phases (not in this commit)` block (107-113) still lists Phase 4 REGN, Phase 5 MUSC routing, Phase 6 reverb zones as future. Phase numbers were never renumbered when those phases shipped, so a reader cross-referencing "Phase 6" gets contradictory answers (shipped reverb send vs. unshipped reverb zones).

## Evidence
- Lines 90-105 (`# Phase 6 (this commit)` — reverb send, shipped) vs. line 112 (`Phase 6: Reverb zones`, listed as future). Re-confirmed 2026-06-15.
- `docs/feature-matrix.md:101` marks Phases 1–6 complete; FOOT/REGN the only ✗ rows.

## Impact
Doc-accuracy only. No runtime impact.

## Related
#1615 (AUD-...-04, same docstring family); `docs/feature-matrix.md` is authoritative and correct.

## Suggested Fix
Renumber the `# Future phases` block to drop shipped-phase numbers — list remaining work by name (FOOT/3.5b material sounds, REGN ambient, MUSC routing, occlusion attenuation, per-cell acoustic reverb) without reusing 4/5/6.
