# TD3-002: Deleted Material::classify_pbr named as a live per-frame entry point

_Filed as #1624 from `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md`. Immutable snapshot as-filed; GitHub is authoritative for live state._

**Severity**: LOW · **Dimension**: Stale Documentation · **Effort**: trivial
**Source audit**: `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md` (TD3-002)
**Status**: NEW (same class as CLOSED #1321 / #1522, new site)

## Description
`classify_legacy_pbr`'s doc (`crates/nif/src/import/material/mod.rs:986-990`) says it stays "in lockstep" with "the per-frame draw build's `Material::classify_pbr`." That method was **deleted** in the NIFAL refactor — PBR resolves at the parse-time `translate_material` boundary; there is no per-frame classifier. `grep "fn classify_pbr\b"` → zero hits. The sibling doc at `crates/core/src/ecs/components/material.rs` correctly calls it "(deleted)".

## Evidence
`import/material/mod.rs:988-990` — `/// the per-frame draw build's /// Material::classify_pbr and this importer-side translation /// stay in lockstep.` The single source of truth is `byroredux_core::ecs::components::material::classify_pbr_keyword`; `Material::classify_pbr` no longer exists.

## Impact
A reader follows a dangling reference to a deleted method and may conclude a per-frame PBR classifier still exists, contradicting the NIFAL "resolve-once at translate boundary" invariant.

## Suggested Fix
Reword to reference the live `classify_pbr_keyword` free fn and note `Material::classify_pbr` was removed in the NIFAL refactor.

## Related
#1321, #1522 (both CLOSED — same deleted-`classify_pbr` doc class).

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: The reworded doc keeps `classify_pbr_keyword` as the single PBR classifier; no implication of a per-frame/render-time classifier survives
- [ ] **SIBLING**: No other doc names the deleted `Material::classify_pbr` as live
