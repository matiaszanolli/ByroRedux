# SUBSYS-2026-08-16-02: NiTimeController timing envelope is parsed and discarded

**Issue**: #3097
**Severity**: MEDIUM
**Labels**: `medium,animation,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-16.md` (subsystem-gap sweep).

**Location**: `crates/nif/src/blocks/controller/mod.rs`:36-45 (`NiTimeControllerBase` parses `flags` / `frequency` / `phase` / `start_time` / `stop_time`) · `crates/nif/src/anim/entry.rs`:255-260 (the merged embedded clip)

## Description

The `NiTimeController` timing envelope is **parsed and discarded**. Every mesh-embedded animation is forced to Loop at rate 1.0 with no phase.

## Evidence

`NiTimeControllerBase` decodes the full envelope — `flags` (which carry the cycle type), `frequency`, `phase`, `start_time`, `stop_time`.

The consumer throws all of it away:
```rust
// crates/nif/src/anim/entry.rs:255-260 (re-verified 2026-08-17)
name: "embedded".to_string(),
duration: 0.0,
cycle_type: CycleType::Loop,     // <- hardcoded
frequency: 1.0,                  // <- hardcoded
weight: 1.0,
accum_root_name: None,
```

## Impact

Every embedded animation in every game plays as a 1.0-rate loop regardless of what the NIF authored. Clamped and reverse cycle types become loops; authored playback rates are ignored; phase offsets are lost, so instances of the same mesh animate in lockstep instead of staggered.

`duration: 0.0` alongside the hardcoded values suggests the envelope was never wired rather than deliberately overridden.

## Suggested Fix

Carry the parsed `flags`/`frequency`/`phase`/`start_time`/`stop_time` through to the merged clip: map the cycle-type bits from `flags` to `CycleType`, use the authored `frequency`, and derive `duration` from `stop_time - start_time`.

`CycleType` already models the variants (`crates/core/src/animation/types.rs`), so the canonical sink exists.

## Related

- `crates/core/src/animation/types.rs` (`CycleType` — the existing canonical model)
- #3087 (the animation-adjacent doc rot in the same sweep)

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: The envelope is translated at import, never re-derived at playback
- [ ] **ALL-FIELDS**: `cycle_type`, `frequency`, `phase` and `duration` all carried, not just one
- [ ] **SIBLING**: The `.kf` import path checked — it may already do this correctly
- [ ] **NO-GUESSING**: Cycle-type bit mapping comes from nif.xml, not inference
- [ ] **TESTS**: A regression test asserts a Clamp-authored embedded clip does not loop

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3097 --json state` when live state is needed.*
