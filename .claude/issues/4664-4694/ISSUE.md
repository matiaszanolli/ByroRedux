=== #4664 ===
# null: PAR-D3-2026-09-21-01: BGSM/BGEM accept any version with the newest layout and never check for unconsumed bytes, so layout drift is silent [OPEN]

## Description
`crates/bgsm/src/base.rs:170-171`, `crates/bgsm/src/bgsm.rs:158-335`, `crates/bgsm/src/bgem.rs:96-190`: there is no version ceiling anywhere in `parse_after_magic`/`BgsmFile::parse`/`BgemFile::parse`, and neither `parse` function checks `Reader::remaining()` before returning `Ok`. An unknown future layout, or a version-gating bug in this crate, decodes to wrong field values with no signal at all. The reference implementation (Material-Editor `BaseMaterialFile.cs:179-234`) also has no version ceiling, so a warning rather than a hard `Err` matches the format's own precedent.

Verified unchanged at HEAD `ee6d3fb39`: neither `parse` function reads `remaining()`, and there is no `version > N` rejection anywhere in the three cited files.

## Evidence
Probe `bgsm-scan`:
- Vanilla uses only v2 (FO4: 6,616 BGSM + 283 BGEM) and v22 (FO76: 25,888 + 4,101).
- 0 of 36,888 vanilla files leave a byte unconsumed; the probe re-parsed each file with its last byte removed to confirm the check would be zero-noise.

## Impact
Silent wrong material fields on mod or future (post-v22) content, with no diagnostic distinguishing "parsed correctly" from "parsed against the wrong layout".

## Related
PAR-D4-2026-09-21-03 (the sibling finding that notes FO76 BGSM has no dedicated sweep)

## Suggested Fix
After parse, `warn!` (with the path) when `remaining() > 0` or `version > 22`. Add both conditions to the FO4/FO76 sweep.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D3-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix
=== #4665 ===
# null: PAR-D4-2026-09-21-03: Sweep gaps: no FO76 BGSM or BA2 sweep, no FNV MenuXml corpus, no Oblivion EGM test, and CDB asserts only non-zero counts [OPEN]

## Description
- **FO76 BGSM** (`crates/bgsm/tests/parse_all.rs:255-268`). Never swept. This audit measured 29,989/29,991 OK; the 2 failures are vanilla `.bgsm` files that are Material-Editor JSON text (`materials\atx\setdressing\atx_plushie_mr.fuzzy_valentinesday\*.bgsm`), rejected with `BadMagic`. Whether FO76's own runtime reads JSON BGSM is unverified.
- **FO76 BA2** (`crates/bsa/tests/ba2_real.rs`). No parser-crate test; the DX10 path is never exercised on FO76 content, which now holds 40 `.ba2` files after the 2026-09-20 archive rewrite.
- **FO3 BSA**. The v104 BSAs have no test.
- **FNV MenuXml**. The HUD profile ships (`hud.rs:148`, 121 menu XMLs) with zero tests.
- **Oblivion FaceGen**. 141 EGMs, no test. The FNV/FO3 EGM test (`crates/facegen/tests/parse_real_facegen.rs:178-227`) prints the non-finite count instead of asserting it, which would have caught PAR-D5-2026-09-21-01 far earlier.
- **CDB** (`crates/sfmaterial/tests/real_cdb.rs:83-90`). Asserts only non-zero counts; the measured 97 classes / 1,438,780 values could be pinned exactly.

Verified unchanged at HEAD `ee6d3fb39`: none of the six gaps has a new test at any of the cited locations.

## Evidence
See the Gate Matrix in the source report; probe logs `probe_bgsm_scan.log` and `probe_dup_scan.log` back the FO76 BGSM/BA2 counts.

## Impact
Format branches with real vanilla content in the installed corpora but no regression pin, across five titles.

## Related
PAR-D4-2026-09-21-01 (no CI lane runs any of these even if they existed), #3466

## Suggested Fix
Add FO76 BGSM (with a JSON-form allowlist) and FO76 BA2 GNRL/DX10 sweeps, an FNV MenuXml corpus test, and an Oblivion EGM case. Pin the CDB counts exactly rather than just asserting non-zero.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D4-2026-09-21-03)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
=== #4666 ===
# null: PAR-D4-2026-09-21-04: archive_with_payload leaks one temp BSA per test per run [OPEN]

## Description
`crates/bsa/src/archive/tests.rs:289-316`: the `archive_with_payload` test helper writes `byroredux-bsa-#352-<pid>-<entry>.bsa` into `temp_dir()` and never removes it; the path is not returned to the caller so nothing downstream can clean it up either. The sibling `write_temp_v105` helpers in the same file do call `remove_file` after use.

Verified unchanged at HEAD `ee6d3fb39`: `archive_with_payload` still ends with `BsaArchive { file: Mutex::new(file), ... }` and no cleanup of `path`.

## Evidence
This audit's own unit run (`TMPDIR=/mnt/data/tmp`, 20:07) added 6 such files. 24 sat in `/mnt/data/tmp` from 4 prior runs (Sep 14 x3, Sep 21) at the time of the audit.

## Impact
Temp-dir litter that accumulates on the default tmpfs `/tmp` every `cargo test -p byroredux-bsa` run.

## Related
#352 (the regression this helper exists to cover)

## Suggested Fix
Return the path (or a guard that deletes on drop) and remove it after `BsaArchive` construction; the open file handle keeps the data readable on Unix even after unlink.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D4-2026-09-21-04)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
=== #4667 ===
# null: PAR-D5-2026-09-21-03: Renderer-relevant BGSM/BGEM fields authored in vanilla are dropped without a documented deferral [OPEN]

## Description
`byroredux/src/asset_provider/material/merge.rs:913-927`'s #2704 comment documents 11 BGSM scalars as "deferred: no consumer", and #2642 separately documents distance-field alpha. Several other authored, non-default fields are also dropped but are undocumented in either ledger:

- FO76 `lum_emittance != 0`: 25,813 BGSM.
- FO76 `use_adaptive_emissive`: 3,272. BGEM `adaptive_emissive_final_exposure_max`: 4,101. Together these are the v22 emissive model.
- FO76 `base.depth_bias`: 230; `base.mask_writes != ALL`: 57.
- FO4 `decal_no_fade`: 356; `dissolve_fade`: 23; `glowmap`: 154 (FO4) / 289 (FO76).
- BGEM `falloff_color_enabled`: 2 / 107; `envmap_min_lod`: 11 / 19.
- `cast_shadows=false`: 327 / 778. The NIF-side `Cast_Shadows` equivalent is also unread.

Correctly ignored, for contrast: `receive_shadows=false` appears on 6,552/6,616 FO4 and 25,888/25,888 FO76 BGSMs, so it cannot plausibly mean "unshadowed" and is rightly left alone.

Verified unchanged at HEAD `ee6d3fb39`: the #2704 comment at `merge.rs:905-927` still lists only the original eleven scalars.

## Evidence
Field counts from probe `bgsm-fields` (`probe_bgsm_fields.log`).

## Impact
No per-field visual claim is made here (runtime semantics of each field are unverified against a reference renderer). The gap is that the "not yet wired vs overlooked" ledger #2704 created omits these fields entirely, so the next completeness sweep can't tell which bucket they belong to.

## Related
#2704 (the ledger this extends), #2642, #1077, PAR-D5-2026-09-21-02 (specular_enabled — a related but distinct BGSM-forwarding gap in the same file), #4430 (open — tracks the `glowmap` flag specifically from a different angle: NIF-flag unreliability, not the dropped-field documentation gap this finding is about)

## Suggested Fix
Extend the #2704 comment (or a doc table) with these fields and their vanilla counts, or wire the ones with clear renderer sinks (`depth_bias`, `mask_writes`).

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D5-2026-09-21-03)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix
=== #4678 ===
# null: CHAR-2026-09-21-D4-03: Player is half-populated — CharacterLevel/Background withheld on a justification the #4458 stamp falsified, deferred to two closed issues [OPEN]

**Severity**: LOW
**Dimension**: Population Boundary
**Game**: FO3 / FNV / FO4 / Skyrim

## Description

The note updated by `eb3784309` keeps `CharacterLevel` and `Background` off the player on two grounds, and neither holds:
- It says "`Background` has no honest value until the player has a real race/class". Yet the same commit derives the player's `ActorValues` *from* the Player record's class, race and level 1. That is exactly the provenance `Background` records.
- It defers population to "#3004 / #2986". Both issues are CLOSED and concern NPC AVIF prefixes / NPC Health derivation. No open issue tracks player level/background.

Meanwhile the absent level reads differently depending on the consumer:
- 0 in `GetLevel`, in `GetActorValue`'s derived rows and in `GetXPForNextLevel`. For FNV that last one gives `150*0+50` = 50 XP, versus 200 at L1.
- 1 in `melee_damage_charal_bonus` and in container leveled loot.

All of this is for an actor whose Health was derived at level 1.

## Evidence

Verified at HEAD `ee6d3fb39`. `byroredux/src/scene.rs`: "`CharacterLevel` and `Background` remain deliberately absent ... `Background` has no honest value until the player has a real race/class. Populating those is CHARAL work (#3004 / #2986)". `gh issue view`: #3004 CLOSED ("auto-calc derivation has no Health term"), #2986 CLOSED ("AVIF EditorIDs are AV-prefixed").

`GetLevel` (`crates/scripting/src/condition.rs`): `world.get::<CharacterLevel>(entity).map_or(0.0, ...)`. `GetXPForNextLevel`: `world.get::<CharacterLevel>(entity).map_or(0, ...)` feeding `rs.leveling.xp_to_next(level)`. `byroredux/src/combat.rs`'s `melee_damage_charal_bonus`: `world.get::<CharacterLevel>(aggressor).map_or(1, ...)`. `byroredux/src/cell_loader/references/attach.rs`: player level `.map_or(1, |level| level.level.max(1))`.

## Impact

- Player-level CTDA gates read 0.
- XP-to-next is computed for level 0.
- The deferral has no live tracker, so it cannot be scheduled.

## Related

CHAR-2026-09-21-D4-01, #4458, #3158; `/audit-gameplay` noted the level-1 loot bootstrap as documented policy (unfiled).

## Suggested Fix

Stamp `CharacterLevel { level: effective_actor_level(player) }` and `Background` from the resolved Player record beside `ActorValues`. Otherwise, re-point the deferral at an open issue with a reason that still holds.

Source: docs/audits/AUDIT_CHARACTER_2026-09-21.md (CHAR-2026-09-21-D4-03)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix

=== #4679 ===
# null: CHAR-2026-09-21-D1-02: Native HUD's per-game pool roster is a GameKind match in a CHARAL consumer [OPEN]

**Severity**: LOW
**Dimension**: Ruleset Seam
**Game**: all

## Description

Which pools a game's character has is per-game ruleset data: Health/Magicka/Stamina, Health/Magicka/Fatigue, or HP/AP. Here it is expressed as a consumer-side `GameKind` match, because `CharacterRulesProfile`/`CharacterRuleset` expose no pool roster. This is the shape #4447 moved off a consumer for body conditions. No FO3-vs-FNV discrimination is needed, so behaviour is correct.

The Oblivion arm is unreachable in production. `actor_value_form_id` resolves only AVIF records, and `Oblivion.esm` authors none. The test feeds synthetic AVIFs.

## Evidence

Verified at HEAD `ee6d3fb39`, `byroredux/src/inventory.rs`:
```rust
fn vital_bar_candidates(game: GameKind) -> &'static [(&'static str, &'static str)] {
    match game {
        GameKind::Skyrim => &[("Health", "Health"), ("Magicka", "Magicka"), ("Stamina", "Stamina")],
        GameKind::Oblivion => &[("Health", "Health"), ("Magicka", "Magicka"), ("Fatigue", "Fatigue")],
        GameKind::Fallout3NV | GameKind::Fallout4 | GameKind::Fallout76 => &[("HP", "Health"), ("AP", "ActionPoints")],
        GameKind::Starfield => &[("HP", "Health"), ("O2", "O2")],
    }
}
```

## Impact

A future FO3/FNV divergence or new family requires a consumer edit invisible to the profile table; the Oblivion arm gives false coverage confidence.

## Related

#4447 (precedent, moved `body_condition_base` onto the profile the same way), CHAR-2026-09-21-D1-01.

## Suggested Fix

Put the pool roster on `CharacterRulesProfile`, as editor-id/label pairs, and have `build_player_vitals` read it. Drop the Oblivion arm, or mark it blocked on #3768's pre-AVIF resolver.

Source: docs/audits/AUDIT_CHARACTER_2026-09-21.md (CHAR-2026-09-21-D1-02)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)

=== #4690 ===
# null: PHYS-D4-2026-09-21-02: The documented M42.10 sweep contract ("own bones masked", "shove clutter") is not what the query does [OPEN]

### PHYS-D4-2026-09-21-02: The documented M42.10 sweep contract ("own bones masked", "shove clutter") is not what the query does

- **Severity**: LOW
- **Dimension**: Character & NPC Controller
- **Location**:
  - `crates/physics/src/world.rs:101-111` (`actor_move_interaction_groups` doc), `:862-871` (`CharacterMoveParams::filter_groups` doc), `:1306-1366` (`move_character`)
  - `byroredux/src/systems/locomotion.rs:100-118`
- **Status**: NEW

**Description**:
- **All actors' bones are masked, not just the walker's own.** Bone colliders get membership `ACTOR_BONE_GROUP` (`sync.rs:1048-1055`) and the walker's filter is `Group::ALL & !ACTOR_BONE_GROUP` (`world.rs:98`). A group mask cannot tell "own" bones from anyone else's, so every live actor's ~18 bones and every FO4 fallback capsule are invisible to every walker.
- **Dynamic clutter acts as an immovable wall.** `move_character` passes a no-op collision callback and never calls `solve_character_collision_impulses` (re-confirmed at HEAD: `grep -n solve_character_collision_impulses crates/physics/src/world.rs` finds no hit), so nothing is pushed. Autostep uses `include_dynamic_bodies: false` (confirmed at `world.rs:1313`), which makes rapier's `handle_stairs` refuse a dynamic "stair" (`control/character_controller.rs:658-670`). The main sweep keeps dynamic bodies (`:264`), so a walker is blocked by clutter but can neither push it nor step over it.

**Evidence**: code and rapier source as cited, re-verified at HEAD `ee6d3fb39`. The `actor_move_interaction_groups` doc (`world.rs:101-111`) still says "an NPC should be blocked by walls and shove clutter", which the `move_character` implementation does not do.

**Impact**: walking NPCs pass through each other and through FO4 shape-less actors. A dynamic floor item tall enough to exceed the 50° climb limit on the r=20 capsule (about 7–9 BU, offset included) blocks a walker outright. Wander and Patrol recover through the 2.5 s re-pick; Travel, Follow, Escort and Guard have none (`locomotion.rs:92-98`). That gameplay-visible behaviour is routed to `/audit-gameplay`; this finding is the doc-vs-code contract mismatch and the underlying query gap. Not a pre-M42.10 regression for NPC-vs-NPC contact, since NPCs ghosted through everything before.

**Related**: #2873, M42.10 (`913fd39d8`).

**Suggested Fix**: fix the three doc sites. If NPC-vs-NPC blocking is wanted, exclude only the walker's own bones with a `QueryFilter` predicate keyed on `ActorColliderOwner`. Decide whether walkers should apply character-collision impulses to dynamic bodies.

Source: docs/audits/AUDIT_PHYSICS_2026-09-21.md (PHYS-D4-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: The same `ACTOR_BONE_GROUP` wholesale mask is used by the ground probes (`ground_probe_groups`) — check whether an own-bones-only filter is needed there too
- [ ] **TESTS**: A regression test with two overlapping walker capsules asserts the intended contact behaviour once decided (either genuine blocking with an own-bones exclusion, or an explicit ghosting-by-design doc update)

=== #4691 ===
# null: PHYS-D5-2026-09-21-01: #3974 made the player's current "plane wins"; the dynamic path it mirrors adds both drags [OPEN]

### PHYS-D5-2026-09-21-01: #3974 made the player's current "plane wins"; the dynamic path it mirrors adds both drags

- **Severity**: LOW
- **Dimension**: Water / Buoyancy
- **Location**:
  - player side: `byroredux/src/systems/character.rs:1093-1120` (`plane_flow.or_else(marker_flow)`), `:286-299` (one drift term)
  - dynamic side: `crates/physics/src/water.rs:843-858` (marker lookup), `:971-980` (plane drag × submerged fraction), `:1040-1081` (marker drag × 1.0, applied after the plane), `:1008` (`WaterContact.flow` = plane flow only)
- **Status**: NEW (partial close of #3974, not a regression)

**Description**: the dynamic path applies the plane's drag and then the marker's drag, and the forces add. Its own comment says why: "so a co-located water plane's force does not discard the marker's current" (`water.rs:1035-1038`). The player sampler consults the marker only when the plane has no `WaterFlow`. The #3974 commit's comment on the player side cites "the dynamic path's `current_flow.or(plane flow)` resolution", which does not exist in `water.rs` — re-grepped at HEAD, `water.rs` has no `.or(` flow resolution anywhere.

**Evidence**: river WATR planes get a `WaterFlow` (`env_translate.rs:868-884`, `cell_loader/water.rs:699-701`), and XWCU markers are separate synthesized volumes (`cell_loader/references/synth_child.rs:36-49`), so plane+marker co-location occurs in real content (e.g. Skyrim rapids).

**Impact**: in rapids, a barrel feels plane drag plus marker drag while the swimmer beside it feels only the plane. `water.contacts` shows the same flow for both, which hides the difference. This is a gameplay parity issue only.

**Related**: #3974, #3114, #3268.

**Suggested Fix**: choose one composition for both samplers. Extend the #3974 test with a flowing-plane + marker case that asserts the two samplers agree.

Source: docs/audits/AUDIT_PHYSICS_2026-09-21.md (PHYS-D5-2026-09-21-01)

## Completeness Checks
- [ ] **TESTS**: Extend the #3974 regression test with a flowing `WaterPlane` overlapped by a `WaterCurrentVolume` marker, asserting the player and dynamic samplers compose the two flows identically
- [ ] **SIBLING**: Check other water-adjacent samplers (e.g. audio/`systems/water.rs` wave sampling per #4183) for the same plane-vs-marker precedence assumption

=== #4692 ===
# null: PHYS-META-2026-09-21-01: audit-physics/SKILL.md drift found while running it [OPEN]

### PHYS-META-2026-09-21-01: `audit-physics/SKILL.md` drift found while running it

- **Severity**: LOW
- **Dimension**: Queries & Diagnostics (audit infrastructure; cross-dimension)
- **Location**: `.claude/commands/audit-physics/SKILL.md` — the known-open register, Dim 1 guards, the Dim 2 recovery bullet, Dim 3 last bullet, Dim 4 NPC and player bullets, the Dim 5 samplers bullet, and the Dim 6 cost bullet
- **Status**: NEW

**Description**: eight statements in the skill are stale, re-verified at HEAD `ee6d3fb39`:
1. `SKILL.md:22` lists #4407 as open, but `gh issue view 4407` shows CLOSED (`726a2e930` fixed it).
2. `SKILL.md:43` lists `non_finite_cuboid_extent_clamps_instead_of_reaching_rapier` as a default-lane guard. It is release-only (PHYS-D1-2026-09-21-01).
3. "Multibody joints are restored with their bodies": by design the recovery detaches them (`world.rs:258-263`, `playable-vertical-slice.md:1386-1390`).
4. `SKILL.md:72` "Writeback drives `Transform`": re-confirmed it writes `GlobalTransform` (`byroredux/src/ragdoll.rs:10,475-496`).
5. `snap_character_body_to_camera` "takes `&mut World`": re-confirmed it takes `&World` by documented design (`character.rs:763`).
6. `SKILL.md:79` "NPCs shove clutter": re-confirmed nothing pushes clutter (PHYS-D4-2026-09-21-02).
7. "Plane flow wins over the marker in both": re-confirmed the two samplers disagree (PHYS-D5-2026-09-21-01).
8. "Budget from the rebuild": re-confirmed the rebuild is avoidable (PHYS-D6-2026-09-21-02), and the snapshot walk adds a per-substep cost (PHYS-D2-2026-09-21-01).

**Evidence**: each item verified at `ee6d3fb39` at the cited lines, per the citations above.

**Impact**: the next `/audit-physics` run would start from wrong premises on three live mechanisms (recovery, NPC sweep, sampler parity).

**Related**: ECS-2026-09-21-D2-01 (#4575, same audit-skill-drift class), NIF-D3-2026-09-21-04 (#4630, same class).

**Suggested Fix**: update the skill and run `.claude/commands/_audit-validate.sh`.

Source: docs/audits/AUDIT_PHYSICS_2026-09-21.md (PHYS-META-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: Re-run `_audit-validate.sh` after the skill edit to catch any other drifted path/symbol reference in the same file

=== #4693 ===
# null: GAME-D4-2026-09-21-01: A dead combatant's corpse root is written to world origin by npc_combat_ai_system, and the zeroed Transform is saved [OPEN]

## Description

When an `AiCombatState` actor's owner dies, `npc_combat_ai_system`'s read pass pushes a `Decision` with `new_translation: Vec3::ZERO` and `state: None` for the dead attacker. The write pass then unconditionally applies `transform.translation = decision.new_translation` for every decision, including that one — so the dead actor's placement-root `Transform` is written to world origin on the very next tick after death.

`AiCombatState` is not part of the death teardown anywhere else in the codebase (`reconcile_dead_actor` in `combat.rs` and `clear_ambient_behavior` in `npc_spawn/ai_package.rs` both have zero references to it — confirmed by grep). The system cleans its own `AiCombatState` row up in the same pass (removes it when `decision.state` is `None`), which is why the branch only fires once per actor — but the zeroed-transform write still happens on that one frame regardless.

Since `Transform` is a `MUTABLE_DELTA_COLUMNS` entry (`byroredux/src/save_io.rs:84-89`), a save taken after the kill captures the corpse root at world `(0,0,0)`. On load, `reconcile_dead_actor_runtime_state` propagates the hierarchy from that root and `activate_ragdoll` seeds the ragdoll bodies around the origin — the corpse is gone from where it fell.

## Evidence

```rust
// byroredux/src/systems/combat_ai.rs — dead-attacker decision (read pass)
if world.get::<Dead>(entity).is_some() {
    decisions.push(Decision {
        entity,
        new_translation: Vec3::ZERO,
        new_rotation: None,
        state: None,
        strike: None,
    });
    continue;
}
```

```rust
// byroredux/src/systems/combat_ai.rs — write pass, unconditional for every decision
if let Some(mut transforms) = world.query_mut::<Transform>() {
    for decision in &decisions {
        if let Some(transform) = transforms.get_mut(decision.entity) {
            transform.translation = decision.new_translation;
            ...
```

`grep -n "AiCombatState" byroredux/src/combat.rs byroredux/src/npc_spawn/ai_package.rs` returns zero matches — confirmed neither the death-reconcile path nor the ambient-package teardown ever touches this component.

The existing regression test `clears_combat_state_when_attacker_dies` spawns the attacker at `Vec3::ZERO`, so the wrong write is unobservable from that test.

## Impact

- Within the session, skinned bodies keep rendering from the ragdoll's own world-space palette, but the root's `GlobalTransform`, `WorldBound` and any non-bone attachments jump to the origin, and the skinned-mesh bound merge stretches from the origin to wherever the body actually is.
- A save taken after the kill stores the corpse root at `(0,0,0)`. On load, the corpse respawns at/near the world origin instead of where it fell — inside geometry or in an unrelated/unloaded exterior cell.

## Trigger

Skyrim MQ101 keep-escape (or any `Effect::StartCombat` fragment); kill the armed NPC; save; load.

## Related

- GAME-D5-2026-09-21-02 (the same actor is also driven by its ambient package)
- #4605 (CONC-D3-2026-09-21-02, the same system's lock-hold-stack fix, now closed and in place — not a regression of it)
- #3708 (the precedent that grew the death teardown for `AmbientPackageRuntime`)

## Suggested Fix

- Remove `AiCombatState` in `reconcile_dead_actor` (`byroredux/src/combat.rs`), the same place `AmbientPackageRuntime` and other per-actor runtime state are torn down.
- In `npc_combat_ai_system`, either carry the actor's *current* translation forward for a `state: None` decision, or skip the `Transform` write entirely for those decisions.
- Update the dead-attacker test to start away from the origin so the wrong write would be caught.

Source: docs/audits/AUDIT_GAMEPLAY_2026-09-21.md (GAME-D4-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: Every other per-actor runtime component added since the #3708 precedent is checked against the death teardown (`reconcile_dead_actor` / `clear_ambient_behavior`)
- [ ] **TESTS**: A regression test spawns the dead attacker away from the origin and asserts its `Transform` is unchanged (or intentionally preserved) after `npc_combat_ai_system` runs

=== #4694 ===
# null: GAME-D5-2026-09-21-01: PlayerRef (0x14) never resolves through the shared FormID resolver — every package, condition and fragment targeting the player misroutes [OPEN]

## Description

`crates/scripting/src/condition.rs`'s `resolve_entity_by_global_form_id` matches an entity whose `FormIdComponent` resolves (through the `FormIdPool`) to a pair whose `local` equals the requested `form_id`. It is the shared resolver behind Follow/Escort/Travel/Guard package targeting, `GetDistance`, `RunOn::Reference`, fragment object properties, and `quest_stages.rs`.

The player body's `FormIdComponent` is stamped with `PLAYER_FORM_ID_PAIR` (`crates/core/src/form_id.rs:149-152`), whose `local` is `LocalFormId(1)`, not `0x14`. No cell places an actual `0x14` ACHR (this is Bethesda's conventional "PlayerRef" sentinel, not a real placed reference), so `resolve_entity_by_global_form_id(world, 0x14)` always returns `None` — the player can never be resolved through this shared path.

`crates/scripting/src/package.rs` already special-cases `0x14 → player` locally (lines 365 and 436), which is why package-selection logic that goes through `package.rs` works. But the ambient movers (`follow.rs`, `escort.rs`, `travel.rs`, and Guard via `resolve_near_reference_target`) call `resolve_entity_by_global_form_id` directly with no such special-case, so:

- **Follow(player)** goes terminal-idle (no retry): the target never resolves, so the follower never starts.
- **Travel(NearReference player)** and **Escort-lead** fall back to `pick_wander_target`, a deterministic pseudo-random point within 512 units of home.
- **Guard(player)** falls back to its home anchor.

`GetDistance PlayerRef` (`condition.rs:542`) reads as `f32::MAX` when the player can't be resolved — infinitely far — so every package/dialogue/quest CTDA gated on player proximity always evaluates false.

## Evidence

```rust
// crates/scripting/src/condition.rs — resolve_entity_by_global_form_id
pub fn resolve_entity_by_global_form_id(world: &World, form_id: u32) -> Option<EntityId> {
    ...
    let q = world.query::<FormIdComponent>()?;
    let pool = world.try_resource::<FormIdPool>()?;
    let found = q
        .iter()
        .find(|(_, fid)| pool.resolve(fid.0).is_some_and(|p| p.local.0 == form_id))
        .map(|(eid, _)| eid);
    found
}
```

```rust
// crates/core/src/form_id.rs
pub const PLAYER_FORM_ID_PAIR: FormIdPair = FormIdPair {
    plugin: PluginId(u128::MAX),
    local: LocalFormId(1),   // not 0x14
};
```

Census with the engine's own parser (`/tmp/audit/gameplay/lvlprobe`, `playerpk` binary) of PACK records whose target or location is PlayerRef:
- FNV: 75 packages (Follow 52, Escort 8, Travel 13, Guard 2), listed directly by 29 `NPC_`. Examples: `FollowersCassFollowPlayerDEFAULT`, `FollowersRexFollowPlayerDEFAULT`, `VMS18TedFollowPlayer`.
- FO3: 66 packages (Follow 39, Escort 23, Travel 4) on 40 `NPC_`. Examples: `MQ05Stage80LiEscortPlayerToTunnel`, `MQ11SarahFollowPlayerEndgame`.
- Oblivion: 12 Travel packages on 4 `NPC_` (`MQ12LichAttack`, `SE32FindPC`).

`follow.rs:82-84`, `escort.rs:103-105`, and `travel.rs:99-127` (and Guard through `resolve_near_reference_target`) all call `resolve_entity_by_global_form_id` directly with no `0x14` special-case, confirmed by grep.

## Impact

- Companion and escort packages never actually follow or escort the player, on FNV, FO3 and Oblivion (the games with playable ambient-package companions).
- Travel-to-player packages send NPCs to arbitrary nearby points with no diagnostic.
- Package, dialogue and quest CTDA gated on player proximity always evaluate as out-of-range.
- `PlayerRef` object properties in fragments decline silently.

## Trigger

Any FO3/FNV/Oblivion NPC whose winning package targets the player (companions, MQ05 Li, MQ11 Sarah), any `GetDistance PlayerRef` gate, any game.

## Related

- #3099 (closed; the `0x14`-vs-`0x7` base-record confusion — a related but distinct confusion, in a different function)
- #1664 (GetDistance resolver)
- GAME-D2-2026-09-21-04 (the same `0x14`/`0x7` confusion pattern, in the theft rule)

## Suggested Fix

Resolve PlayerRef inside `resolve_entity_by_global_form_id` itself: special-case `0x14` → the `PapyrusPlayerEntity` / `PlayerEntity` body, the same identity `package.rs` already special-cases locally. Then delete the two local special-cases in `package.rs` now that the shared resolver covers it. Add a follow-the-player regression test that exercises the ambient path (not just `package.rs`'s own selection).

Source: docs/audits/AUDIT_GAMEPLAY_2026-09-21.md (GAME-D5-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: Every caller of `resolve_entity_by_global_form_id` (condition CTDA, `RunOn::Reference`, fragment object properties, `quest_stages.rs`, the ambient movers) resolves the player correctly after the fix, not just Follow/Escort/Travel/Guard
- [ ] **TESTS**: A follow-the-player test exercises the ambient path end-to-end (not `package.rs`'s local special-case)

