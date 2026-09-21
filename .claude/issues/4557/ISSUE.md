# NIFAL-D3-2026-09-21-03: #3987 narrative and its test message still say for_legacy_projection(false) = "ARCHITECTURE only" — stale since 3ce970a5a broadened it to ARCHITECTURE|DYNAMIC_ACTOR

**Labels**: low, nifal, documentation, doc-rot

**Severity**: LOW · **Dimension**: Skinning/Lights (ESM boundary documentation) · **Tier Violated**: — · **Game Affected**: Starfield (narrative), all legacy-fill lights (the constant)
**Location**: `byroredux/src/systems/light_anim.rs:151-156` and `:1022-1030`; constant at `crates/core/src/lighting.rs::VisibilityMask::for_legacy_projection`
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
`3ce970a5a` (2026-09-17) broadened `for_legacy_projection(false)` from `ARCHITECTURE` to `ARCHITECTURE | DYNAMIC_ACTOR` so legacy fill lights cast dynamic-actor contact shadows. The #3987 rationale block still derives its punchline from the old value ("props, actors, foliage, glass and effects cast no shadow from any of them"), and the test's assertion message (:1029) states the old mapping in the present tense.

### Evidence
`git show 3ce970a5a -- crates/core/src/lighting.rs` (mask broadened with an updated doc comment); `light_anim.rs:1027-1029`.

### Impact
The next person re-verifying #3987 (or the strict-animation/permissive-shadow asymmetry) finds doc and constant disagreeing and must re-derive which is real.

### Related
#3987, 3ce970a5a

### Suggested Fix
Date the historical narrative and update the test message to name the current mask (`ARCHITECTURE | DYNAMIC_ACTOR`), or "the conservative legacy mask, not FULL".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **TESTS**: A regression test pins this specific fix
