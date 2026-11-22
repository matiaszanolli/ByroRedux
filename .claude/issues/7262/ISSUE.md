# #7262: Sandbox and Patrol ignore a Near Editor Location PLDT and anchor on where the actor stands

**Severity**: MEDIUM · **Found by**: /audit-sync 2026-11-22 (follow-up to #7225 / #7254)

## Description
#7225 and #7254 made Travel, Wander, Guard and Ambush honour a PLDT of *Near Editor Location* (type 3) by anchoring on the actor's `EditorPlacement` through `travel::package_home`. Sandbox and Patrol still drop the location type: `AmbientBehavior::Sandbox` carries only `search_radius` and `AmbientBehavior::Patrol` only `patrol_radius` + `actor_form_id` (`byroredux/src/npc_spawn/ai_package.rs`, the `is_sandbox()` / `is_patrol()` arms of the behaviour builder), and `byroredux/src/systems/sandbox.rs` / `patrol.rs` read no location. Both therefore centre on wherever the actor currently stands.

## Evidence
- Vanilla census (measured during #7254): `FalloutNV.esm` has **424** Sandbox and **61** Patrol packages whose PLDT is type 3.
- `ai_package.rs`: `Self::Sandbox { search_radius: location_radius }`, `Self::Patrol { patrol_radius: location_radius, actor_form_id }` — no `near_editor_location`, unlike the Wander/Travel/Guard/Ambush arms beside them.

## Impact
An NPC whose Sandbox is authored around its editor spot drifts: after any displacement (a scripted move, combat, a Travel that ends elsewhere, a load after it moved) it sandboxes around the new position instead of returning to its authored area. Same for a Patrol radius.

## Suggested Fix
Carry `near_editor_location` (from `location_anchor(package.location)`) on `Sandbox` and `Patrol` as the other four do, and resolve the centre through `travel::package_home` in `sandbox.rs` / `patrol.rs`. Check the GECK *Sandbox* / *Patrol* procedure pages for how each uses its location before wiring Patrol (it walks a marker chain; the radius may be the only PLDT consumer). Re-measure FO3 counts.

## Completeness Checks
- [ ] **SIBLING**: every AmbientBehavior variant that reads a PLDT radius checked for the location type
- [ ] **TESTS**: a Near Editor Location Sandbox (and Patrol, if applicable) anchors on the editor placement after the actor is displaced
