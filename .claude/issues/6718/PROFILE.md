# #6718 — scheduler profile, 2026-11-06

**Build:** release, `ByroRedux` at `27dcf26db` (working tree clean). **Host:** RTX 4070 Ti + 7950X.
**Method:** `BYRO_PROFILE=1`, `--player --bench-frames 900`, one run per scene, headless under
`xvfb-run` with `WAYLAND_DISPLAY` unset (so no window reached the desktop session). The
metrics sampler (~2 Hz) logs the scheduler's top 8 systems; the table averages those samples after
dropping the first fifth as warm-up. A system outside the top 8 in a sample is not counted there, so
the means are lower bounds for systems near the 8th place (about 0.06 to 0.12 ms).

| Scene | Command (data from `/mnt/data/SteamLibrary/steamapps/common`) |
|---|---|
| FNV interior | `--game fnv --cell GSProspectorSaloonInterior` |
| Skyrim SE exterior | `--grid 2,-4 --radius 3` (Whiterun), Meshes0 + Meshes1, Textures0 |
| FO4 interior | `--cell ShamrockTaphouse01`, Meshes + MeshesExtra, Textures1, Materials |

## Frame, from the `bench:` line

| Scene | wall ms | `systems_ms` (all systems) | share | brd ms | draw ms (of which `submit`) | entities |
|---|---|---|---|---|---|---|
| FNV Prospector Saloon | 33.80 | 1.47 | 4.3 % | 0.29 | 31.59 (30.75) | 2,284 |
| Skyrim Whiterun | 39.05 | 4.76 | 12.2 % | 1.38 | 32.33 (30.45) | 17,809 |
| FO4 Shamrock Taphouse | 40.40 | 3.34 | 8.3 % | 1.55 | 35.20 (33.72) | 13,025 |

The wall time is dominated by `submit`, which here includes the present wait under Xvfb. Treat the wall
and `draw` figures as unrepresentative of a real display. `systems_ms` is a CPU timer and is valid.

## Top systems (mean ms per frame, share of samples in the top 8 in parentheses)

| System | Registration | FNV | Whiterun | FO4 |
|---|---|---|---|---|
| `physics_sync_system` | `add_to_with_access`, Physics stage | 0.126 | **2.220** (max 9.71) | 0.345 |
| `extension_custom_event_dispatch_system` | `add_exclusive_with_access`, Late | 0.059 | 0.458 | 0.116 |
| `interaction_system` | `add_exclusive_with_access`, Update | 0.169 | 0.256 | 0.446 |
| `make_world_bound_propagation_system` | `add_exclusive_with_access`, PostUpdate | 0.083 | 0.364 | 0.243 |
| `make_transform_propagation_system` | `add_to_with_access`, PostUpdate (already parallel) | 0.062 | 0.260 | 0.074 |
| `trigger_detection_dispatch` | exclusive, Update | not in top 8 | 0.230 | 0.024 |
| `scene_dialogue_system` | exclusive, Update | not in top 8 | 0.108 | 0.234 |
| `make_animation_system` | exclusive, Update | 0.119 | 0.127 | 0.167 |
| `make_travel_system` | exclusive, Update | 0.175 | not in top 8 | not in top 8 |
| `player_controller_system` | exclusive, Update | 0.105 | 0.028 | 0.071 |

## Reading

* **The whole scheduler costs 1.5 to 4.8 ms a frame.** Even a perfect, free parallelisation of
  everything but the largest system could save well under that, and a saving only reaches the frame if the
  frame is CPU-bound. Here the CPU side (systems + `brd` + `ssbo` + TLAS) is 2 to 7 ms of a 34 to 40 ms
  wall, so it is not.
* **The only system over 0.5 ms is `physics_sync_system`, and it is already parallel-registered.** It is
  the only system in the Physics stage, so parallelism cannot shorten it: a parallel phase takes as long as
  its slowest member. Its Whiterun spikes (max 9.71 ms) are Rapier step cost, a physics question, not a
  scheduling one.
* **The other candidates are small, and where a conflict is declared it is real.**
  `make_world_bound_propagation_system` (0.36 ms at most) is exclusive and declares a write to
  `GlobalTransform`, the same type the already-parallel transform propagation writes: a declared
  write/write pair, and it must run after that system anyway. `interaction_system` (up to 0.45 ms) is
  exclusive with a large declared access set; the analyzer never pairs exclusives
  (`interaction.rs`'s own comment), so whether it conflicts was **not** analysed. At up to 0.45 ms in a frame
  that is not CPU-bound it is below any threshold worth the analysis. Animation, extension dispatch,
  trigger and dialogue systems were likewise judged on cost only (each at most 0.46 ms); their
  access was not checked against `access_report()`.

## Outcome

No system qualifies for promotion, so no registration changed and `known_conflict_count()` is unchanged.
Per the issue ("If nothing qualifies, close this with the data") it is closed with this record. Revisit if
a scene shows `systems_ms` above about 8 ms on a real display, or if the frame becomes CPU-bound.
