# #7035: m43-crafting smoke: no workbench is loaded at Skyrim grid 2,-4 (pre-existing, regression since 2026-10-30)

`docs/smoke-tests/m43-crafting.sh` fails at its first live step: `craft list` answers `craft: no workbench is loaded`, so the gate never reaches `craft make` or the CRFT Story Manager event it exists to check.

## Reproduce

```bash
docs/smoke-tests/m43-crafting.sh      # Skyrim SE installed; builds release itself
# smoke[m43-crafting]: FAIL -- no bench near the player: ... "Error: craft: no workbench is loaded"
```

The launch is `--game skyrim_se --grid 2,-4 --radius 2 --player --bench-frames 30 --bench-hold`, then a 15 s settle before `craft list`.

## What is known

- **Not caused by #6653.** The same failure reproduces on a clean worktree at `deb31c4d3` (HEAD before #6653's `3ef0a2cb2`), with the same smoke script.
- **The gate has been blind since #6716.** `6413d542b` (2026-11-06) moved `Installed N crafting recipes` / `Installed Story Manager tree` into `byroredux::scene::scene_runtime`, which the smoke's `RUST_LOG` filtered out, so the gate timed out at its first wait and never got as far as `craft list`. `3ef0a2cb2` adds `byroredux::scene::scene_runtime=info` to the filter. Since then both install lines appear (`Installed 592 crafting recipes`, `Story Manager tree: 571 nodes`) and the failure moves to the bench check. So the last known-good run is the one from when the gate was written (`851636a13`, 2026-10-30), and the regression lies somewhere in 2026-10-30 .. 2026-11-16.
- The grid is ambiguous: `Grid (2,-4) ±2 is present in multiple worldspaces: tamriel, sovngarde, blackreach, …, whiterunworld, …; applying the deterministic preferred/size tie-break`. The smoke's header says it boots "onto the Whiterun exterior grid".

## Candidates (unverified)

1. **`e9c861dea` (2026-11-15, Fix #6539): "benches skip disabled references."** If the bench near the player is an initially-disabled or enable-parented reference, the bench lookup now skips it. `9b9b2806d` (same day) pins the bench lookups.
2. **The worldspace tie-break** picks a worldspace other than the one with the bench, or the player spawns away from it. `f9f3eb831` (#6591, one parse for interior and exterior sessions) and `ea5417dc7` (#6725, EXAL region work) touched exterior loading in the window.
3. **The bench is not resident within 15 s** at radius 2 (streaming).

## To settle it

- Log which worldspace the tie-break chose and the player's spawn cell.
- List the crafting `FURN` references in the loaded cells with their initially-disabled / enable-parent state, then check whether `e9c861dea`'s filter rejects them.
- Bisect `851636a13..deb31c4d3` with the fixed `RUST_LOG`.

## Done when

`m43-crafting.sh` passes (`craft list` names recipes, `craft make` crafts, and a CRFT-started quest is logged). It is part of the M48 regression floor (PRD `game-ui-integration` §7.2 scenario 1).

