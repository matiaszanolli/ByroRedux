# #1661 — SKY-D5-01: numeric-sibling BSA auto-load skips digit-suffixed siblings
Domain: binary (byroredux).

## Resolution: CLOSED — stale finding, already fixed (no code change)
The reported early-return-on-digit-suffix (`if last.is_digit() { return }`)
no longer exists. Commit 821a425b added the series-start arm:
`numeric_sibling_paths("Skyrim - Textures0.bsa")` → Textures1..9, and
Meshes0 → Meshes1. Code moved to asset_provider/archive.rs in the #1669
split (behavior unchanged).
- SIBLING ✓ — textures + mesh call sites share `open_with_numeric_siblings`
  (asset_provider/texture.rs:129,136).
- TESTS ✓ — `siblings_skyrim_zero_start_offers_1_through_9` already pins
  the requested regression (+ mid-series / …10 / BA2 edge cases). 5 pass.

# #1663 — GetActorValue (idx 9)
Domain: ecs/scripting.

## Resolution: KEPT OPEN — re-scoped as actor-value milestone dependency
No in-scope fix. An actor's base AV is DERIVED (class+race+level
composition), not a stored field, so there is no faithful 1:1 populate like
FactionRanks/PerkList had — building the derivation IS the actor-value
milestone, and arbitrary values are the fabrication the issue forbids. The
0.0 safe-default in evaluate_function is the honest behavior and stays.
A faithful fix needs: (1) production ActorStats (base+modifier composition),
(2) base derivation from class/race/level + NPC_ skill_bonuses, (3) AVIF
FormID→key resolver (source in EsmIndex.actor_values; surface as a World
resource per the #1668 Globals pattern). Tracked for that milestone.
