# Batch 5355–5358 (fetched 2026-10-09)

## #5355 — PHYS-D2-2026-10-05-03: `PhysicsWorld::articulation_joints` never pruned (LOW, physics)
- `build_ragdoll` pushes one handle per joint (~17/humanoid) into `articulation_joints`.
  Field doc claims stale handles "are skipped, so the vector never needs sweeping" —
  skipped yes, removed never. Nothing prunes: not `remove_ragdoll`, `remove_body`, restore, detach.
- Cost: one `MultibodyJointSet::get_mut` miss per lifetime ragdoll joint, per substep.
- Fix: `retain` live joints at top of `clamp_explosive_velocities` (like dynamic_bodies
  compaction), or drop ragdoll's `joints` in `remove_ragdoll`. Sibling of #5272 (closed).
- Locations: `crates/physics/src/world.rs:348-353` (doc), `:1075`; `ragdoll.rs:529` (push),
  `:863-867` (`remove_ragdoll` does not prune).

## #5356 — PHYS-D2-2026-10-05-04: `clamp_explosive_velocities` doc rot + over-counting detaches (LOW, physics, doc-rot, test-gap)
- Fn doc (`world.rs:949-955`) still describes pre-#5246 semantics (consecutive-substep
  parking / set cleared on clean substep); actual design is a lifetime count.
- `>= 3` arm (`:1032-1048`): calls `remove_multibody_articulations`, bumps
  `explosive_detaches_total`, logs "detached articulation" on 3rd burst AND every burst
  after, for any dynamic body incl. ones with no articulation (removal = no-op).
- Guard test `the_third_burst_detaches_the_articulation` drives a plain dynamic body (no
  joint) and asserts `explosive_detaches_total() == 1` — pins a detach that detached nothing.
- Fix: rewrite fn doc to lifetime ladder; count detach only when
  `multibody_joints.rigid_body_link(handle).is_some()` before removal (or only on
  transition to offence 3); test must use a real articulation.

## #5357 — PHYS-D5-2026-10-05-02: swim drift scales marker current by fraction; dynamic path uses 1.0 (LOW, water/physics)
- `byroredux/src/systems/character.rs:1120-1137` (`player_current_drift`), `:1255-1275`
  (#4691 vector composition); compare `crates/physics/src/water.rs:1050-1079`
  (`current_force(…, 1.0, …)`).
- Dynamic path: plane drag × fraction + marker drag × 1.0. Not swimming: marker × 1.0 (matches).
  Swimming: `player_water_state` composes plane+marker into ONE vector; `player_current_drift`
  scales the whole vector by `state.fraction` (~0.675 at swimlevel threshold → marker drift
  drops ~32% until fully submerged). Guard test `swimming_drift_uses_the_composed_column_flow_only`
  pins the composite × fraction form.
- Player ignoring plane flow while wading = documented design (`:1111-1113`), NOT filed.
- Fix: compose `plane × fraction + marker × 1.0` in the swim arm (mirror dynamic path), or
  document intended scaling in `docs/engine/watal.md` and keep test.

## #5358 — SKY-D3-2026-10-05-01: Skyrim `BODT` never decoded → reconcile hides Draugr hair/beards (MEDIUM)
- `BOD2` = biped flags u32 + armor type u32. `BODT` = biped flags u32 + general flags u8 +
  3 pad + optional armor-type u32 (12-byte form). Same first u32 (xEdit `wbBODTBOD2`).
- Decode gaps: `crates/plugin/src/esm/records/items.rs:540` (ARMO reads BMDT/BOD2 only);
  `crates/plugin/src/esm/records/misc/equipment.rs:103` (ARMA reads BMDT/BOD2 only).
  BODT falls to `_ => {}` → `biped_flags == 0`.
- Census: Skyrim.esm ARMO 2752 BOD2 / 10 BODT; ARMA 0 BOD2 / 766 BODT. Update.esm ARMA 10 BODT;
  Dawnguard ARMA 150 BODT. All 10 BODT ARMOs have non-zero masks (0x04 Body skins ×6,
  0x02 Hair ×2, 0x10 Beard ×2).
- Chain: hair/beard root = non-intrinsic `NpcEquipmentPart`; form in `Inventory` but not in
  `EquipmentSlots.occupants` (zero mask → no set bits) → `reconcile_worn_gear`
  (`byroredux/src/npc_spawn/loot_appearance.rs:507-562`) stamps `NpcAppearanceHidden`.
  Fires from `reference_state::restore` (cell return) + save-load.
- #3408's zero-mask retain exemption was built on misreading BODT ARMOs as `BOD2 == 0`
  (`npc_spawn.rs:1478-1490`); #3411 comment premise false (`equip.rs:250-266`).
- With BODT decoded, `equip.rs` would skip 80 redundant same-slot addons on 75 Skyrim ARMOs
  (68 KhajiitRaceVampire circlets etc.). Vanilla same-slot winner rule unsourced — DO NOT GUESS.
- Fix: 1) decode BODT in both parsers + fixtures (both widths) + real-master census guard
  (10 ARMO / 766 ARMA BODT in Skyrim.esm); 2) correct #3408/#3411 comments; 3) re-measure
  zero-mask exemption coverage; 4) winner rule needs sourced reference (leave first-wins).
- Note: NPC_.WNAM decode (SKY-D3-2026-10-05-02) already landed as #5359 (edb5fbdfe).

Domain targets: #5355/#5356 → `byroredux-physics`; #5357 → `byroredux` (bin) + physics ref;
#5358 → `byroredux-plugin` + `byroredux` consumers.
