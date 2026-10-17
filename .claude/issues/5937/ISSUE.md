# PHYS-D2-2026-10-16-02: `contact_event_position` falls to the old origin midpoint when the narrow-phase pair is already gone, skipping its own dynamic-side fallback

Labels: low,physics,bug
Source: docs/audits/AUDIT_PHYSICS_2026-10-16.md

- **Severity**: LOW
- **Dimension**: Step & Sync (contact events)
- **Location**: `crates/physics/src/world/mod.rs:748-785` (the dynamic-body loop at 770-778 sits inside `if let Some(pair) = ... contact_pair(...)`; the origin-midpoint tail at 779-786)
- **Status**: NEW (partial gap in #5675's fix; code-read only)
- **Trigger Conditions**: a `started` force-crossing recorded in an earlier substep of a multi-substep frame (catch-up frames), the body then bounces away far enough that the broad phase drops the pair before `impact_audio_system` resolves the event, with one parent body parked at an origin far from the contact (static kit ghost, exterior terrain tile).
- **Description**: the doc says the order is deepest solver contact, then the dynamic side's translation, then the origin midpoint. In code the dynamic-side fallback is only reachable while `contact_pair` still returns the pair. If the pair has been removed, control goes straight to the origin midpoint, which is exactly the #5675 symptom (impact sound ~150 m away for a terrain tile).
- **Evidence**: structure of the function: `if let Some(pair) = ... { manifold loop; if deepest {return}; for handle in [a,b] { if dynamic {return translation} } }` then `pa/pb` midpoint.
- **Impact**: rare mis-placed (inaudible) impact sound on catch-up frames with fast, high-restitution bounces. Cosmetic.
- **Related**: #5675, `contact_event_position_is_the_contact_not_the_body_origins` (single-substep drop, does not cover the removed-pair case).
- **Suggested Fix**: move the dynamic-side loop out of the `if let Some(pair)` block so it precedes the origin midpoint unconditionally.



Severity summary: 0 CRITICAL / 0 HIGH / 0 MEDIUM / 2 LOW (both non-behavioural: a test gap and doc rot). No new solver, seed, or seam defect survived disproof.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling producer or consumer
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

_Filed from `docs/audits/AUDIT_PHYSICS_2026-10-16.md`. Fix with `/fix-issue <number>`._
