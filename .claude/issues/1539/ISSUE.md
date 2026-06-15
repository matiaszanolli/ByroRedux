# D7-02: Non-Ragdoll/non-LimitedHinge constraints dropped silently — can fragment a ragdoll chain with no telemetry

**Issue**: #1539 · **Severity**: MEDIUM (HIGH if a load-bearing FNV joint authors one of these types) · **Labels**: medium, nif-parser, legacy-compat, bug
**Source**: AUDIT_FNV_2026-06-14 (D7-02) · **Status when filed**: NEW, CONFIRMED

## Location
- `crates/nif/src/import/collision.rs:316-320` (the `BhkConstraintData::Other => continue` arm in `extract_ragdoll`)

## Description
`extract_ragdoll` only surfaces constraints decoded as `Ragdoll` or `LimitedHinge`. Any `bhkHingeConstraint`, `bhkBallAndSocketConstraint`, `bhkPrismaticConstraint`, or `bhkStiffSpringConstraint` — all decoded as `BhkConstraintData::Other` (`constraints.rs:379-386`) — hits `=> continue` and is dropped with no log line (unlike the FO4-NP / phantom stubs in the same file, which `log::debug!` their drops).

## Evidence
Match arm literally `BhkConstraintData::Other => continue,` with no `log::*` (`collision.rs:319`). Downstream `orient_tree` BFS-walks surviving edges; a dropped sole-link edge → disconnected forest → `build_ragdoll` builds each component as an independent free-floating multibody → detached limb free-falls.

## Impact
Visually broken ragdoll with no diagnostic — the silent drop is the dangerous part. FNV humanoids are Ragdoll+LimitedHinge-dominated (probably fine); creatures (radscorpion, cazador, deathclaw) and modded skeletons mix in hinge / ball-and-socket.

## Suggested Fix
At minimum `log::warn!` when a constraint linking two ragdoll bodies is dropped as `Other` (name type + bones). Better: after `orient_tree`, log/assert on a forest result. Long-term: decode plain hinge / ball-and-socket into the canonical joint set (limitless hinge = `LimitedHinge { min: -PI, max: PI }`).
