# FNV-D7-03: Ragdoll colliders have no interaction-group exclusions — self-collision at rest, HavokFilter parsed but dropped

Source: `docs/audits/AUDIT_FNV_2026-08-03.md`, Dimension 7 (PHYSAL Ragdoll — FNV Reference Slice), finding FNV-D7-03.
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2338
Labels: medium, legacy-compat, bug

**Severity**: MEDIUM
**Dimension**: Dimension 7 — PHYSAL Ragdoll (FNV reference slice; PHYSAL-wide, not FNV-specific)
**Location**: `crates/physics/src/ragdoll.rs:159-176` (collider insertion in `build_ragdoll` — no `collision_groups`/`solver_groups` set anywhere in the crate); `crates/nif/src/blocks/collision/rigid_body.rs:19,65` (`havok_filter` field, parsed but never read outside test fixtures); `crates/nif/src/import/collision/ragdoll.rs:78-88` (translate boundary — field dropped, never threaded through to `ImportedRagdollBody`)

## Description

Ragdoll colliders are inserted with Rapier's default interaction groups — a
repo-wide grep for `collision_groups`/`solver_groups`/`InteractionGroups` in
`crates/physics/src/ragdoll.rs` finds none. Rapier's multibody defaults enable
both self-contacts and contacts between directly-jointed links, so every
ragdoll body pair (adjacent and non-adjacent) can collide with every other.

The authored Havok `HavokFilter` field is genuinely parsed —
`BhkRigidBody.havok_filter: u32` (`crates/nif/src/blocks/collision/rigid_body.rs:19`,
read at line ~65) — but it is dropped at the ragdoll translate boundary: the
only other occurrences of `.havok_filter` in the whole tree are
`havok_filter: 0` in unit-test fixtures — it is never read from a real parsed
body and never threaded into `ImportedRagdollBody` or the Rapier collider
build.

## Evidence

A probe against the (FNV-D7-01-corrected) FNV body poses found 8 of 17
constrained pairs interpenetrating at rest, up to ~10 units deep (e.g.
`Spine2 <-> Spine1` gap −10.3).

## Impact

Interpenetrating jointed/adjacent bodies fight the constraint solver with
separation impulses from the moment a ragdoll activates, on top of (and
independent from) the FNV-D7-01/FNV-D7-02 pose bugs — this would still cause
visible jitter/explosion even after those are fixed.

## Suggested Fix

Give ragdoll colliders a per-actor `InteractionGroups` that excludes other
members of the same ragdoll (or disable multibody self-contacts / set
per-edge `contacts_enabled(false)` for jointed pairs). Surfacing the authored
`HavokFilter` group through the translate boundary is the fidelity-correct
version — thread it from `BhkRigidBody.havok_filter` into
`ImportedRagdollBody` and from there into the Rapier collider build.

## Validation

CONFIRMED — verified directly (grep confirms no interaction-group calls
anywhere in the crate, and `havok_filter` has zero real-data readers) and
independently re-confirmed by a background validation pass. No open-issue
duplicate found.
