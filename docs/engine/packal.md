# PACKAL — Package Abstraction Layer

**PACKAL** (Package Abstraction Layer; pronounced "PACK-al") is the
execution layer for **ambient NPC AI** — the background behavior a placed
actor runs from its own `PACK` package list, independent of any quest. It is
named after NIFAL/EXAL/PHYSAL/WATAL/CHARAL but its fork sits in a different
place than theirs: those four fold a **per-game parse-time** divergence into
one canonical representation. `PACK`'s parse-time divergence is **already
folded** — `PackRecord` (`crates/plugin/src/esm/records/misc/pack.rs`) has
carried both the FO3/FNV flat shape (`procedure_type`/`schedule`/`location`/
`target`) and the Skyrim+ tree shape (`procedures`/`package_template_form_id`/
`data_inputs`) in one struct since before this doc existed. PACKAL's fork is
at **execution time**: the same canonical `PackRecord` currently has two
independent, unequally-built consumers — a Scene-owned driver (built, narrow)
and an ambient/spawn-owned driver (built, but blind to the Skyrim+ shape
entirely).

"Abstraction" is still the brand, but the mechanism here is **shared
resolution, independent drivers** (one core that turns a package + an actor
into a resolved command; two call sites that decide *when* to run it and
*how its lifetime is tracked*) rather than `translate()`'s per-game
`Imported* → Canonical` boundary.

**Status**: ACTIVE (opened 2026-08-19, from a real-data investigation — see
§3). The first slice (§4, Skyrim+ ambient Sandbox) shipped the same day —
see §6 for what landed and the real numbers.

**Goal**: an NPC's ambient package stack resolves and executes the same way
regardless of which era authored it — FO3/FNV's flat `PROCEDURE_*` packages
(already live, M42.0–M42.9) and Skyrim+'s template/procedure-tree packages
(not live at all) both reach the same per-procedure ECS behavior components
(`SandboxBehavior`, `WanderBehavior`, …) through one selection+resolution
core, with only the *driver* (what triggers evaluation, where completion is
tracked) differing between "this actor's own spawn-time package stack" and
"a `SCEN` action currently naming this actor."

---

## 1. What's already unified vs. what isn't

| Concern | State |
|---|---|
| ESM parse (`PACK` → `PackRecord`) | **Unified.** One struct, both shapes, since before this doc. No PACKAL work needed here. |
| Package selection (condition-gated, first-eligible-wins) | **Duplicated, not unified.** `crates/plugin/src/esm/records/misc/pack.rs::active_package` (ambient, FO3/FNV-only) and `crates/scripting/src/package.rs::select_package` (Scene, shape-agnostic) are two independent implementations of the same idea. |
| Data-input / target / template resolution | **Exists once, reachable from one driver only.** `procedure_inputs`, `input_destination`, `input_target_entity`, `alias_position` (`package.rs`) already do this generically — but only `scene_package_system` calls them. |
| Leaf-type coverage (which procedure names actually *do* something) | **Split down the middle, for different reasons — plus one Skyrim+ leaf now covered (§4).** Ambient (`npc_spawn/ai_package.rs`) covers FO3/FNV Sandbox/Wander/Travel/Follow/Escort/Guard/Patrol (indefinite or slow-completing, fits a continuously-running actor) **and**, as of the §4 slice, Skyrim+ `"Sandbox"` leaves. Scene (`package.rs::resolve_command`) covers Travel/Patrol/Escort/FollowTo (move-to) and Activate/Acquire/Shout/Sit/UseIdleMarker/UseWeapon (interaction) — finite, completable actions, because a Scene phase has to *end*; it has no Sandbox/Sleep/Eat handling at all (Scenes don't run indefinite actions). Skyrim+ Sleep/Eat and every other leaf type remain uncovered by both drivers. |
| Runtime behavior systems (`sandbox_seat_system`, `wander_system`, …) | **Already shape-agnostic.** They read `SandboxBehavior`/`WanderBehavior`/etc., not `PackRecord` — whichever driver populates those components, the systems don't care. No PACKAL work needed here either. |

The gap is narrow but total: nothing anywhere calls the ambient
spawn path with a Skyrim+-shaped package and gets a behavior component out
the other end.

---

## 2. Two drivers, one core

```
                    resolve (shared)                          drive (per-context)
  PackRecord + ─────────────────────────▶  resolved command  ───────────────────▶  ECS behavior
  actor + world      select_package             (leaf type +                         component /
                      + procedure_inputs          destination/target/                 running action
                      + template chase             radius/etc.)
                      + data-input resolve
```

- **Shared core** (exists — `crates/scripting/src/package.rs`'s
  `select_package`/`procedure_inputs`/`input_destination`/
  `input_target_entity`/`resolve_command`): turns `(package_ids, actor,
  world)` into a resolved leaf + its inputs. Shape-agnostic already — it
  reads `PackRecord.procedures`/`data_inputs`/`package_template_form_id`.
  In practice the ambient driver's first slice (§4) needed only
  `procedure_inputs` made `pub` — Sandbox resolves a scalar radius with no
  `world`/`actor`/`SceneActorBindings` dependency at all; a future leaf type
  that needs a resolved *destination* (not just a radius) will need
  `input_destination` exposed too, at which point its `SceneActorBindings`
  dependency (alias targets only) becomes a real question to answer, not a
  hypothetical one.
- **Scene driver** (exists — `scene_package_system`): discovers work from
  `SceneEvent::ActionStarted`, tracks state in `ActiveScenePackageAction`
  (hard-required `scene_form_id`), reports completion through
  `SceneActionCompletionBatch`. Built for finite, choreographed actions.
- **Ambient driver** (exists for FO3/FNV since M42; extended to one
  Skyrim+ leaf type by §4): discovers work from an actor's own
  `NpcRecord.ai_packages` at spawn and at bounded re-evaluation points
  (`AmbientPackageRuntime`/`ambient_ai_package_system`, M42.9), tracks state
  per-actor (no scene to key off), and never "completes" for indefinite
  procedures the same way Sandbox/Wander don't today. §4's fallback branch
  is the first crack in "FO3/FNV shape only" — the driver itself didn't
  need to change, only what it recognizes as a valid package.

The two drivers stay separate — forcing ambient evaluation through
`scene_package_system`'s Scene-shaped event/state model would be a worse fit
than a second, thinner driver reusing the same core. This mirrors how M42's
seven FO3/FNV procedures are seven separate systems sharing `step_toward`,
not one mega-system.

---

## 3. Real-data findings (2026-08-19, `Skyrim.esm`)

Via `pack_ambient_shape_survey` (`crates/plugin/examples/`), cross-referencing
every `NPC_.ai_packages` (PKID) entry against `PackRecord` shape and against
every `SCEN` action's package list:

- 5,118 `NPC_` records; 2,052 carry ≥1 PKID entry.
- 2,315 distinct packages referenced by some NPC's own PKID list; 1,586
  distinct packages referenced by some `SCEN` action. Overlap is small (39
  packages referenced both ways) — ambient and Scene packages are, as
  expected, mostly disjoint sets.
- Of the 2,276 **ambient-only** packages (never named by any `SCEN` action —
  the ones an ambient driver would actually need to run): **100% are Skyrim+
  tree/template-shaped, 0% are FO3/FNV flat-shaped.** Not a majority, not a
  long tail — the FO3/FNV flat shape does not appear in Skyrim's ambient
  package authoring at all.
- Top ambient-only packages by NPC-reference count are entirely
  `DefaultSandbox*`/`DefaultSleep*`/`DefaultEat*` template instances
  (`DefaultSandboxEditorLocation512` alone: 307 NPCs). This matches
  `ai_packages_procedures.md`'s "Package Templates" description and confirms
  Sandbox-family behavior is the highest-value first target here, same as it
  was for FO3/FNV (M42.0, 56% of FNV NPCs).
- Checked separately: none of these packages' legacy `procedure_type` byte
  (still parsed unconditionally — `PKDT` decode isn't version-gated) collide
  with the seven values the ambient selector already checks
  (`PROCEDURE_SANDBOX`/`WANDER`/`TRAVEL`/`FOLLOW`/`ESCORT`/`GUARD`/`PATROL`).
  So today's selector doesn't silently misfire on Skyrim content — it finds
  nothing, cleanly, rather than something wrong.

**What this doesn't tell us:** Oblivion's `PACK` shape. `pack.rs`'s doc
comment cites the FO3/FNV xEdit spec only; nothing in this codebase has
verified whether Oblivion packages ride the same flat layout or diverge a
third way. Per the no-guessing policy, treat that as open until checked
against real `Oblivion.esm` — don't assume the FO3/FNV path covers it.

---

## 4. First slice: Skyrim+ ambient Sandbox (shipped 2026-08-19)

Scoped the same way as M42.3 (Wander) — one procedure, real data behind the
choice. Turned out smaller than planned once the real code was in front of
me rather than sketched from the investigation alone — two corrections
worth recording:

- **No `bsver` gate needed.** `AmbientBehavior::from_package`
  (`byroredux/src/npc_spawn/ai_package.rs`) was already 100% shape-driven,
  not game-driven — its seven existing branches (`is_sandbox()`/etc.) all
  read the FO3/FNV flat `procedure_type` byte, and FO3/FNV packages never
  set `package_template_form_id`. So a single `else` fallback — "none of
  the flat checks matched, try the Skyrim+ tree shape" — is both correct
  and simpler than threading a version check through. FO3/FNV packages are
  provably unaffected: for them `template` always equals `package` (no
  template ref to chase), so the fallback's `template.procedures` is always
  empty and it always returns `None`, same as before this change.
- **No `SceneActorBindings` dependency to route around.** The concern was
  real for the executor's *destination*-resolving paths
  (`input_destination`/`input_target_entity`), but Sandbox only needs a
  search *radius* — a scalar sitting directly on a `PackDataValue::Location`
  input, resolvable with zero `world`/`actor` context. Only
  `procedure_inputs` (`crates/scripting::package`, index-matching a leaf's
  `data_input_indexes` against a package's `data_inputs`) needed to move
  from private to `pub` — a one-line visibility change, same precedent as
  `resolve_entity_by_global_form_id`.
- **No new flag needed either.** Every `AmbientBehavior` variant already
  attaches unconditionally at spawn; only the *consuming* system
  (`sandbox_seat_system`) is opt-in, gated by the pre-existing
  `BYRO_SANDBOX_SIT` in `boot.rs`. Attaching `SandboxBehavior` to more
  actors doesn't need a new gate — the existing one already controls
  whether anything acts on it.

What shipped: `AmbientBehavior::from_package` gained a `template: &PackRecord`
parameter (both call sites already had a packages catalog in scope to chase
`package_template_form_id` against — `EsmIndex.packages` at spawn,
`PackageRegistry` in the M42.9 reevaluation system) and a
`from_skyrim_procedure_tree` fallback: find a `template.procedures` leaf
with `procedure_type == "Sandbox"` (confirmed the literal authored string
against real `Skyrim.esm` before writing any dispatch code — see the
resolved-radius trace in §3), resolve its inputs against the *instance's*
`data_inputs` via the newly-`pub` `procedure_inputs`, and pull a radius out
of the first `PackDataValue::Location` found. Feeds the *existing*
`SandboxBehavior` component unchanged — `sandbox_seat_system` doesn't know
or care which shape produced its radius.

Real-data verification (`real_skyrim_esm_ambient_packages_now_resolve_for_
previously_blind_npcs`, `#[ignore]`d, run against real `Skyrim.esm`): of
2,052 package-carrying NPCs, **855 now resolve some ambient behavior, 722 of
those a Sandbox with a real radius** — all zero before this change.

Explicitly **not** in this slice: Sleep/Eat leaves (same shape of work,
follow-on — the 855-vs-2,052 gap is largely these), any other Skyrim+
procedure type, per-frame re-evaluation of the Skyrim+ shape specifically
(the M42.9 `ambient_ai_package_system` reevaluation path already covers it
for free, since it calls the same `AmbientBehavior::from_package`), and
quest-alias-sourced ambient packages (Radiant Story injection onto arbitrary
actors — a real Bethesda mechanic, `ai_packages_procedures.md` §Packages, but
a separate resolution path from PKID-authored ones).

---

## 5. What stays out of scope

- **The Scene driver.** `package.rs`/`scene_package_system` don't change
  shape — PACKAL's ambient driver is a second consumer of the shared core,
  not a rewrite of the first.
- **The 10 FO3/FNV procedures still with no runtime** (Find/Eat/Sleep/
  Accompany/UseItemAt/Ambush/FleeNotCombat/CastMagic/Dialogue/UseWeapon) —
  blocked on subsystems (combat/magic/dialogue/item-use) that don't exist,
  same as before this doc.
- **Oblivion's `PACK` shape** — unverified (§3); a future slice, not this
  one.
- **CHARAL overlap.** CHARAL stamps `ActorValues`/`Level`/`Perks` at spawn;
  PACKAL stamps behavior components at spawn. Same entity, same moment,
  disjoint concerns — no shared boundary needed between them.

---

## 6. Rollout order

1. ~~Skyrim+ ambient Sandbox~~ — **done (2026-08-19).** §4. `procedure_inputs`
   made `pub`; `AmbientBehavior::from_package` gained the shape-fallback
   branch. 4 unit tests (real-data-shaped fixtures) + 1 `#[ignore]`d
   real-`Skyrim.esm` integration test, all green. Zero changes to
   `sandbox_seat_system`, `crates/scripting::package`'s Scene driver, or any
   FO3/FNV branch.
2. Skyrim+ ambient Sleep + Eat — same shape of work as step 1, once demand
   justifies it (855-vs-2,052 in §4's real-data check is largely this gap).
3. Skyrim+ ambient Wander/Travel/Follow/Escort/Guard/Patrol equivalents, as
   real-data demand justifies each (mirroring how M42.3–M42.8 shipped one at
   a time against evidence, not speculatively).
4. Oblivion `PACK` shape verification — settles §3's open question one way
   or the other before any Oblivion-specific ambient work is scoped.

Each step ships independently behind `cargo test`; nothing here touches the
Vulkan render-pass / pipeline.

---

## 7. Tooling

- `crates/plugin/examples/pack_ambient_shape_survey.rs` — cross-references
  PKID/SCEN package references against `PackRecord` shape; rerun against
  Update.esm/Dawnguard.esm/other titles to extend §3's coverage.
- `byroredux::npc_spawn::ai_package::tests::
  real_skyrim_esm_ambient_packages_now_resolve_for_previously_blind_npcs`
  (`#[ignore]`d — `cargo test -p byroredux -- --ignored`) — real-`Skyrim.esm`
  regression guard on the §4 slice, prints the live resolved/total counts.
- A `pack.ambient` byro-dbg command (proposed, not built) — dump a live
  actor's resolved ambient package + leaf, the runtime analogue of `ragdoll
  <id>` / `env.dump` in the sibling layers.
