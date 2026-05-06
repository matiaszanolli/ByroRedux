# Investigation — #793

**Domain**: binary (npc_spawn)

## Code path

`humanoid_body_path(game, gender) -> Option<&'static str>` returns one canonical body NIF path; only caller is `spawn_npc_entity` at L395 that loads + parents that single mesh under the placement_root. Hands ship as separate NIFs (`lefthand.nif`, `righthand.nif`) in `Fallout - Meshes.bsa` per the issue's listbsa output, so they go unloaded.

## Approach

Change signature to `humanoid_body_paths(...) -> &'static [&'static str]` and iterate at the call site. Each path goes through the same `load_nif_bytes_with_skeleton` + parent-to-placement_root + add_child sequence as the existing upperbody load.

Empty slice `&[]` replaces `None` for SSE/FO4+ — `for _ in &[]` is a no-op so the behavior is preserved.

## Per-game path list (vanilla, conservative)

Only adding paths verified-present by the issue's listbsa output:

- FO3 / FNV (both genders): `_male\upperbody.nif`, `_male\lefthand.nif`, `_male\righthand.nif`
- Oblivion: same three paths — issue says "needs verification" but the existing single-path code already routes Oblivion through the `_male\` directory same as FO3/FNV. Keeping consistent with existing behavior; if Oblivion has a different layout the hand-load will silently miss (debug-log only) like every other modded path.
- Female routing keeps the existing `_male\upperbody.nif` (existing source comment at L99-103 documents the FNV `_female\` directory absence; not changing that). Hands likely apply to both genders since the existing Female code already used the male upperbody.

Foot meshes from the audit completeness check — deferred. The issue notes "likely" and "verify via listbsa" but doesn't include a verified path. Speculating risks the same null-load that we're already paying.

## Sibling check

Foot meshes — explicitly deferred above.

## Test strategy

A unit test on `humanoid_body_paths` pinning the slice contents per game/gender. The integration test the audit suggests (assert `bip01 r hand` has rendered descendants in Goodsprings) requires real BSA + scene-loading test infra that doesn't exist in the repo today.

## Scope

1 file: `byroredux/src/npc_spawn.rs`
