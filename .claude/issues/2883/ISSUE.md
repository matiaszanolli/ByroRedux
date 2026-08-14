# PHYS-D4-03

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2883

---

Found by `/audit-physics` Dimension 4 (Ragdoll Articulation — doctrine accuracy). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: LOW · **Status**: NEW
**Location**: `docs/engine/physal.md:111-113` vs `:51-54`; the omitted seam is `crates/nif/src/lib.rs:85-103`

## Trigger Conditions
Any future audit or refactor that takes §3 literally — e.g. concluding a `havok_scale` bug cannot be per-game, or that the Skyrim x69.99 row in §3's own table has no code behind it.

## Description
`docs/engine/physal.md` §3 opens:

> *"The whole per-game seam is the typed decode of two constraint CInfos (`crates/nif/src/blocks/collision/constraints.rs`)"*

§1, three sections earlier, says the source boundary *"folds every per-game Havok quirk — **constraint field order, `havok_scale`, collision-object kind** — into one canonical spec ... and the *only* place they live"* — which is the correct, three-seam statement.

§3's own compatibility table then **relies on the omitted seam** (*"`havok_scale` x69.99 applied at import via `havok_scale_for(header)`"*), so the section contradicts itself within twenty lines. `havok_scale_for` switches on `NifVariant::detect(...)` — a genuine per-game branch, correctly parse-sited, that §3's headline denies exists.

Additionally, `humanoid_skeleton_path(GameKind)` (`byroredux/src/npc_spawn.rs:200-209`) game-branches *which* skeleton feeds the ragdoll pipeline at all. That is asset resolution rather than physics translation, but the doc says nothing about it either way.

## Evidence
- `crates/nif/src/lib.rs:87-101` — the variant match, two distinct scale constants (7.0 for Morrowind/Oblivion/FO3/FNV, 69.99125 for SkyrimLE->Starfield)
- `crates/nif/src/import/collision/ragdoll.rs:32` — `let scale = scene.havok_scale;`, applied to every pivot and body translation at `:89-94`, `:311`, `:314`, `:339`, `:342`

## Impact
Doc rot in the file the `/audit-physics` doctrine check exists to test. **Left unfixed it will keep producing stale-premise findings in both directions** — auditors "discovering" `havok_scale` as a doctrine violation, and auditors dismissing real `havok_scale` drift because "§3 says there's only one seam". This repo's documented #1 audit failure mode.

Note the doctrine's **substance is intact**: this audit grepped `crates/physics/**`, `byroredux/src/ragdoll.rs`, `byroredux/src/systems/character.rs` and `byroredux/src/cell_loader/spawn.rs` and found **zero** `GameKind` / `NifVariant` / `bsver()` control flow downstream of the parse boundary. Only the wording is wrong.

## Suggested Fix
Reword §3's opening to *"The per-game seam in the **constraint graph** is the typed decode of two CInfos; the two other source-boundary seams (`havok_scale_for`, collision-object-kind dispatch) are enumerated in §1"*, and add a one-line note that skeleton-asset selection is game-branched in `npc_spawn.rs` but carries no physics semantics.
