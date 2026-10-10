# #5495: FNV-2026-10-09-D5-01: Creatures run the humanoid seating path. Sandbox, Eat and Sleep snap them onto chair and bed markers and replace their idle with the humanoid sit clip, so 59 FNV creature placements end up frozen inside furniture, Primm Slim…

**Labels**: ai, bug, game:fnv, game:fo3, gameplay, legacy-compat, medium

**Source**: `docs/audits/AUDIT_FNV_2026-10-09.md` — finding `FNV-2026-10-09-D5-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: MEDIUM. The behaviour and visuals are wrong for authored content, including two friendly creatures on main-path cells. The creature also holds the seat reservation, which takes that seat away from NPCs. No crash and no state corruption.
- **Dimension**: Ambient AI. FNV data runs through the M42 seating mechanism, which `/audit-gameplay` owns.
- **Location**:
  - `byroredux/src/systems/sandbox.rs:311-330`: `sandbox_seat_system_inner`'s per-actor loop iterates every `SandboxBehavior` actor with no actor-kind gate.
  - `byroredux/src/systems/sandbox.rs:192-240`: `apply_seat_assignments` snaps root translation and rotation and swaps `AnimationPlayer.clip_handle` for the sit clip.
  - `byroredux/src/systems/eat_sleep.rs:244-300`: `seat_at_marker` has the same missing gate for Eat and Sleep.
  - Install path: `byroredux/src/npc_spawn/resumable/runtime.rs:988-991`. Creatures reach `apply_ai_package_behavior` there.
  - `byroredux/src/npc_spawn/ai_package.rs:195-198` and `:385-386`: the Sandbox arm and its `insert_at_spawn`.
  - `byroredux/src/cell_loader/references/mod.rs:455-462`: `SandboxSitClip` is resolved for every FO3/FNV cell.
- **Status**: NEW. No open or closed issue and no report in `docs/audits/` mentions creatures and seating, sandbox or furniture together. The gap predates the baseline: Sandbox seating since M42.1/M42.2, Eat/Sleep since `00f580e09`. #5391 extended it this window by anchoring Eat/Sleep more precisely, but it did not create it.
- **Description**:
  - `CREA` records are parsed through `parse_npc`'s shared sub-record walk (`crates/plugin/src/esm/records/dispatch_actor.rs:85`), so they carry their `PKID` list. A placed `ACRE`, or an `ACHR`→`CREA` (#2567), takes the same resumable job as an NPC. Its finalize step calls `apply_ai_package_behavior` (`runtime.rs:990`), which installs `SandboxBehavior`, `EatBehavior` or `SleepBehavior` from the winning package.
  - From then on, nothing distinguishes a mantis from a settler:
    - `sandbox_seat_system_inner` picks the nearest unreserved Sit marker within the package radius. On FO3/FNV that means every furniture marker, beds included: the documented v0 legacy over-match, #5390.
    - `apply_seat_assignments` sets the root's translation *and rotation* to the marker, saves the creature's idle, and installs the per-cell `SandboxSitClip` parked at its final frame (`playing = false`).
  - The creature-spawn code already states why that clip is wrong here: "The shared per-cell pool holds the humanoid clip, whose bone names a creature rig doesn't have, so falling back to it would play nothing" (`runtime.rs:904-908`).
  - Where a creature rig does share `Bip01` channel names with the human rig, the clip instead drives those bones into a human seated pose. The sit-enter clip is the one that lowers `Bip01 NonAccum` onto the seat (`npc_spawn.rs:724-732`).
  - The result is a creature frozen at the chair or bed marker. `Seated` is the one-shot guard, so it stays there until its package changes. For hostile creatures that happens when ambient hostility starts combat; friendly creatures stay seated for the life of the cell.
  - The Eat/Sleep `seat_at_marker` path does the same for creature Eat/Sleep packages. FalloutNV.esm has 5 creature Eat references and 3 creature Sleep references, for example the Quarry Junction deathclaws' `QJDeathclawSleepPackage0x5`.
- **Evidence**: `/tmp/audit/fnv/py/crea_seat2.py` over FalloutNV.esm. The probe:
  - resolves `LVLC` members;
  - honours the `CREA` "Use AI Packages" template flag;
  - skips Initially Disabled placements;
  - takes the first resolved package as the winner, with radius from `PLDT`, else 512.

  Results:
  - 1,037 `CREA` `PKID` references name a procedure-12 (Sandbox) package.
  - **278** placed creatures resolve to a Sandbox first package.
  - **59** of them stand within that radius of a placed `FURN` in the same cell, across 14 cells:

  | Cell | Creatures (base) | Note |
  |---|---|---|
  | Vault11a / Vault11b / Vault11c | 9 / 6 / 9 × `NVCrGiantMantis` | hostile, seated until engaged |
  | RocketLabMid | 8 × `REPCONFeralGhoul` | REPCONN HQ |
  | NiptonTownHallInterior (+Floor2) | 7 + 5 (`NVCrGiantMantisNymph`, `VNiptonLegionMongrelAttack`) | |
  | SLNHPStationINT | 7 × `NVCrGiantMantisNymph` | |
  | Vault34b / Vault34c | 2 + 1 × `v34GlowingGuard*` | |
  | **VikkiAndVance** | `PrimmSlim` (Mr Handy, friendly) | packages `PrimmSlimPatrolSandbox` then `DefaultSandboxCurrentLocation1024`, both Sandbox |
  | **GibsonScrapYardInterior** | `GibsonDog06` (friendly) | `DefaultSandboxEditorLocation256` |
  | CampSearchlightSchool, JacobstownNightstalkerCave, one exterior cell | golden gecko, nightstalker, `VFSChasedRat` | |

  The same probe over Fallout3.esm finds 15 placements, in Vault101b/d, zBethSewer, CorvegaFactory02 and others.
  ```rust
  // sandbox.rs:311-318 — every SandboxBehavior actor, no actor-kind test
  for (npc, behavior) in sandbox_q.iter() {
      if seated_q.as_ref().is_some_and(|s| s.contains(npc)) { continue; }
      ...
      if let Some((seat_id, seat)) = pick_nearest_seat(npc_g.translation, &scratch.seats, &reservations.0, radius) {
  // sandbox.rs:205-224 — root snapped to the marker, idle replaced by the humanoid sit clip
  t.translation = seat.translation; t.rotation = seat.rotation;
  p.clip_handle = sit_handle; p.local_time = hold_time; p.playing = false;
  ```
- **Impact**:
  - On entering Vault 11, REPCONN HQ, the Nipton town hall or Vault 34, the hostile creatures that have not engaged yet are posed inside chairs and beds, rotated to the furniture heading and frozen.
  - Primm Slim, on the main questline path in Primm, and Gibson's dog stay frozen at a chair marker for the life of the cell.
  - Each seated creature holds a `SeatReservations` entry, so a sandboxing NPC in the same cell cannot use that seat.
  - FO3 shows the same shape (15 placements). Oblivion is untouched, because no sit clip is resolved there.
- **Related**:
  - GAME-D5-2026-10-09-03, the same seat write path reached through the stream snapshot.
  - #5390, the legacy marker over-match that makes beds count as sit markers.
  - #2567, which routes creatures through the actor job.
  - #4414 / #4816, ambient hostility suspension. It un-seats hostile creatures only once combat starts.
- **Suggested Fix**:
  - Gate seating on a humanoid actor. Stamp a marker at creature spawn: `npc.is_creature` is already in hand in `prepare_creature_state`, or derive it from `ResolvedNpc`. Then skip marked actors in both `sandbox_seat_system_inner` and `seat_at_marker`.
  - Creatures keep Sandbox's wander and idle half and never take furniture. That matches the vanilla engine, which has no creature sit idles for human furniture.
  - Pin it with a seat-system test: a creature-marked actor within radius of a free Sit marker stays unseated, and the reservation stays free.

**Cross-referenced (not re-filed):**
- **GAME-D5-2026-10-09-01**: #5376's decline leaves the Dialogue package as the active winner, so 56 FNV NPCs idle. An independent PTDT census agrees with #5376's numbers: 332 FNV proc-15 PACKs, of which 293 target the player reference 0x14. By NPC_/CREA reference count, that is 166 against 4.
- **GAME-D5-2026-10-09-02**: In-Cell, Near-Linked-Ref and unresolved Near-Reference Eat/Sleep anchors. The FalloutNV.esm PLDT census by NPC_/CREA reference:

  | Package | type 0 | type 1 | type 2 | type 3 | type 6 |
  |---|---|---|---|---|---|
  | Eat | 289 | 32 | 92 | 287 | 11 |
  | Sleep | 293 | 142 | 36 | 102 | 47 |

  `EditorPlacement` is stamped for NPC_ and CREA alike (`resumable/mod.rs:372-380`).
- **GAME-D5-2026-10-09-03**: the stream snapshot restores a bare `Seated`.
- **ESM-2026-10-09-D2-02**: the PKDD Say-To gate is inert.
- **#5471**: doc rot, the baseline's D5-03, still partly open.
- Voice residue from the baseline's D5-01: FNV DLC voice lives in `<DLC> - Main.bsa`, which no profile mounts.
  - This is now documented in `dialogue_voice.rs` and `dialogue-trees.md`, with a `--sounds-bsa` workaround.
  - `profile_archive_args` appends that flag and suppresses nothing, so the workaround composes.
  - Accepted as documented posture.

**Verified:**
- Real-master tests pass:
  - `real_stimpaks_restore_scaled_health_and_limbs_but_hardcore_only_health`
  - `real_stimpaks_survive_disk_overlay_in_normal_and_hardcore_modes`
  - `real_new_vegas_timed_restoratives_survive_disk_overlay`
  - `installed_fnv_player_base_builds_a_nonempty_template`
  - `installed_fnv_sunny_smiles_selects_the_female_body`
  - `real_master_player_seed_evaluates_the_player_only_rows`
- `hud::` tests pass, including #5280's non-finite reject.
- The #5392 force-greet hold returns early before `forcegreet_open`, so held directives are not consumed by #5376's failed-open path.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
