# PACKAL — Package Abstraction Layer (PROPOSED)

**PACKAL** (Package Abstraction Layer; pronounced "PACK-al") is the proposed
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

**Status**: PROPOSED (opened 2026-08-19, from a real-data investigation — see
§3). No production code has moved yet;
`crates/plugin/examples/pack_ambient_shape_survey.rs` is the one artifact
this investigation left behind, and it's analysis tooling, not part of the
layer itself.

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
| Leaf-type coverage (which procedure names actually *do* something) | **Split down the middle, for different reasons.** Ambient (`npc_spawn.rs`) covers Sandbox/Wander/Travel/Follow/Escort/Guard/Patrol (indefinite or slow-completing, fits a continuously-running actor). Scene (`package.rs::resolve_command`) covers Travel/Patrol/Escort/FollowTo (move-to) and Activate/Acquire/Shout/Sit/UseIdleMarker/UseWeapon (interaction) — finite, completable actions, because a Scene phase has to *end*. **Neither covers the other's list**, and Sandbox/Sleep/Eat — the dominant ambient behaviors on real Skyrim data (§3) — are in neither Skyrim+ dispatcher. |
| Runtime behavior systems (`sandbox_seat_system`, `wander_system`, …) | **Already shape-agnostic.** They read `SandboxBehavior`/`WanderBehavior`/etc., not `PackRecord` — whichever driver populates those components, the systems don't care. No PACKAL work needed here either. |

The gap is narrow but total: nothing anywhere calls the ambient
spawn path with a Skyrim+-shaped package and gets a behavior component out
the other end.

---

## 2. Two drivers, one core (proposed shape)

```
                    resolve (shared)                          drive (per-context)
  PackRecord + ─────────────────────────▶  resolved command  ───────────────────▶  ECS behavior
  actor + world      select_package             (leaf type +                         component /
                      + procedure_inputs          destination/target/                 running action
                      + template chase             radius/etc.)
                      + data-input resolve
```

- **Shared core** (mostly exists — `crates/scripting/src/package.rs`'s
  `select_package`/`procedure_inputs`/`input_destination`/
  `input_target_entity`/`resolve_command`, minus their `SceneActorBindings`
  dependency for alias targets): turns `(package_ids, actor, world)` into a
  resolved leaf + its inputs. Shape-agnostic already — it reads
  `PackRecord.procedures`/`data_inputs`/`package_template_form_id` and would
  work unchanged for an ambient caller.
- **Scene driver** (exists — `scene_package_system`): discovers work from
  `SceneEvent::ActionStarted`, tracks state in `ActiveScenePackageAction`
  (hard-required `scene_form_id`), reports completion through
  `SceneActionCompletionBatch`. Built for finite, choreographed actions.
- **Ambient driver** (doesn't exist for the Skyrim+ shape): would discover
  work from an actor's own `NpcRecord.ai_packages` at spawn (mirroring
  today's FO3/FNV `active_package_is_sandbox`-style checks in `npc_spawn.rs`),
  track state per-actor (no scene to key off), and never "complete" for
  indefinite procedures the same way Sandbox/Wander don't today.

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

## 4. Proposed first slice: Skyrim+ ambient Sandbox

Scoped the same way as M42.3 (Wander) — one procedure, one opt-in flag, real
data behind the choice:

1. Extract `select_package` + the data-input resolvers out of
   `crates/scripting/src/package.rs` into a form callable without
   `SceneActorBindings` (the one real Scene dependency, used only for
   quest-alias targets — punt alias resolution for ambient callers in this
   slice, same v0-scoping call M42 already makes elsewhere).
2. At `npc_spawn.rs`, alongside the existing `active_package_is_sandbox`
   (FO3/FNV) check: for `bsver >= SKYRIM_SE` (or wherever the real gate
   turns out to belong — Skyrim LE's bsver needs checking), walk the actor's
   `ai_packages` through the extracted core, chase `package_template_form_id`
   to find a `Sandbox`-leaf (`PackProcedure.procedure_type == "Sandbox"`,
   pending confirmation that's the literal authored string — verify against
   a real decoded tree before trusting it), and resolve its `Location`-typed
   `PackDataInput` the same way `sandbox_seat_system` already wants one.
3. Feed the result into the *existing* `SandboxBehavior` component — no
   change to `sandbox_seat_system` itself, since it's already shape-agnostic
   (§1).
4. Gate behind a new flag (`BYRO_SKYRIM_SANDBOX` or folded into the existing
   `BYRO_SANDBOX_SIT`, decide at implementation time), mirroring every other
   M42 opt-in.
5. Verify against real `Skyrim.esm` load: NPCs that previously got no
   `SandboxBehavior` at all (any of the 307 `DefaultSandboxEditorLocation512`
   holders) now do.

Explicitly **not** in this slice: Sleep/Eat leaves (same shape of work,
follow-on), any other Skyrim+ procedure type, per-frame re-evaluation
(ambient FO3/FNV packages don't have it either — not a new gap), and
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

1. Core extraction (§4, step 1) — mechanical, behavior-preserving; Scene path
   keeps passing its existing tests untouched.
2. Skyrim+ ambient Sandbox (§4, steps 2–5) — the reference slice.
3. Skyrim+ ambient Sleep + Eat — same shape of work as step 2, once the
   pattern is proven.
4. Skyrim+ ambient Wander/Travel/Follow/Escort/Guard/Patrol equivalents, as
   real-data demand justifies each (mirroring how M42.3–M42.8 shipped one at
   a time against evidence, not speculatively).
5. Oblivion `PACK` shape verification — settles §3's open question one way
   or the other before any Oblivion-specific ambient work is scoped.

Each step ships independently behind `cargo test`; nothing here touches the
Vulkan render-pass / pipeline.

---

## 7. Tooling

- `crates/plugin/examples/pack_ambient_shape_survey.rs` — cross-references
  PKID/SCEN package references against `PackRecord` shape; rerun against
  Update.esm/Dawnguard.esm/other titles to extend §3's coverage.
- A `pack.ambient` byro-dbg command (proposed, not built) — dump a live
  actor's resolved ambient package + leaf, the runtime analogue of `ragdoll
  <id>` / `env.dump` in the sibling layers.
