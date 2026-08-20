# Scripting Subsystem Audit — 2026-08-20

Thirteenth full pass over the M30/M47 Papyrus / `.pex` / ECS scripting domain
(prior reports: `AUDIT_SCRIPTING_2026-06-23.md` … `_08-12.md`, `_08-16.md`).
Run as part of the `comprehensive` audit-suite sweep, single-agent (no
sub-agents, per the shared briefing), covering `crates/pex/`,
`crates/papyrus/`, `crates/scripting/`, `crates/hkx/` (Dim 8 — still the only
owner that crate has), and the engine-side attach / runtime-install path
(`byroredux/src/cell_loader/references/`, `byroredux/src/cell_loader/load.rs`,
`byroredux/src/cell_loader/exterior.rs`,
`byroredux/src/asset_provider/{script,animation}.rs`,
`byroredux/src/boot.rs`, `byroredux/src/interaction.rs`).

**No cargo was run** (briefing rule 4 — a concurrent process holds the target
lock). Every verdict below is static: source reads, greps and commit archaeology
against `bb0b92f2` plus the working tree. Where a claim would need a compile or a
live cell, it is called out as such rather than asserted.

**Dedup baseline**: `/tmp/audit/issues.json` (400 issues, all states, spanning
`#2671`–`#3103`), `docs/audits/AUDIT_SCRIPTING_2026-08-16.md`, and
`git log --since=2026-08-16` over the four in-scope crates and the attach path.
Issue numbers below `#2671` are carried on the prior report's word, per the
briefing's note that they cannot be re-queried.

## What changed since 2026-08-16

Session 70 was water-dominated. Actual churn in this domain, scoped by
`git log --since=2026-08-16 -- crates/scripting/ crates/pex/ crates/papyrus/ crates/hkx/`
plus the attach path:

| Commit | Effect on this domain |
|---|---|
| `2fa2e351` | #3011 fix — `MAX_TRANSFORM_SAMPLES = 16_000_000` bounds `transform_count × num_frames` before `Vec::with_capacity` |
| `0eaea646` | #3013 fix — an out-of-range `track_to_bone` entry now logs instead of silently dropping the track |
| `eca04dce` | #3012 fix — `frags.is_empty()` moved **above** the destructive `poll_quest_events` |
| `b766327d` | #3015 fix — the trigger-volume spawn branch is gated on `is_primary_synth` |
| `3be6c9f1` | #3016 fix — every synthetic child's own base-record script attaches |
| `585fd872` | #3017 fix — checked-in decompiler fidelity test + shape check in `pex_corpus_smoke` |
| `a605ee93` | #2940 fix — `HasPerk` reads `byroredux_core::character::Perks` instead of the dead `PerkList` |
| `14a80fe8` | PACKAL first slice — `procedure_inputs` made `pub`; no behavioural change inside `crates/scripting` |
| `1e9723ab` | #3098 — REFR `XLOC` parsed, new `Locked` component gates activation (upstream of `ActivateEvent`) |
| `36fb9e78` | `SplashEvent` / `RippleEvent` added to the scripting event set and to `event_cleanup_system` |

**Six of the ten 2026-08-16 findings were fixed and closed** (#3010, #3011,
#3012, #3013, #3015, #3016, #3017). Each was re-verified in source this pass;
none has regressed. Four remain open and are **not re-filed**: #3014, #3019,
plus the older #2671 / #2672 / #2289 / #2290 / #2540 / #2541 / #2542 / #2668 /
#2669 / #2670 / #2267 / #2153 / #2270 set.

**#3010 was fixed differently from the suggested fix** and that difference is
the source of two of this pass's findings — see SCR-D7-2026-08-20-02/-03.

## Executive Summary

**Shipped and re-confirmed live**: M30.2 `.psc` parser; M47.0 event hooks; M47.1
condition eval; M47.2 `.pex` reader + 5-phase decompiler + recognizer chain +
dynamic attach path + XPRM trigger volumes + fragment lowerer + QUST VMAD
property table + `AddItem`/`MoveTo` object targeting; the MQ101
PACK/SCEN/DIAL/two-state-activator/player-control/HKX-cinematic runtime; M47.3
quest-lifecycle effects and the quest-alias fill-and-apply runtime.

**Deferred, correctly, not flagged as defects**: Obscript/SCTX frontend (Phase
5); M47.3 Phase 4+ (Created Object alias spawn, Story Manager event fills, true
`LCTN` traversal, reference-collection aliases, unloaded-world Find-Matching
search, injected packages/spells/keywords overlay families); Havok
behavior-graph execution — `crates/hkx` still decodes only, and re-reading both
files this pass confirms nothing walks a behavior graph.

**Findings this pass: 4 new — 0 CRITICAL / 0 HIGH / 3 MEDIUM / 1 LOW.**
Per dimension: **Dim 1 — 0. Dim 2 — 0. Dim 3 — 0. Dim 4 — 0. Dim 5 — 1 MEDIUM.
Dim 6 — 1 MEDIUM. Dim 7 — 2 (1 MEDIUM, 1 LOW). Dim 8 — 0.**

The theme this pass is **fixes that landed but did not reach their stated
effect, and gates that structurally cannot fail**. Not one of the four is a logic
bug inside the decompiler or the parser; all are wiring, coverage or
reach.

**Untrusted-input robustness verdict — CLEAN for `.pex`, `.psc` and now
`.hkx`.** Re-verified at HEAD: every `.pex` primitive read funnels through
`take()` (`checked_add` + `<= data.len()`); every `Vec::with_capacity` in
`reader.rs` is fed by a `u16` (≤ 65535) except the var-arg vec, which is still
the plain `Vec::new()` + `push` loop #1710 installed; `OpCode::from_u8`'s
`transmute` is still `byte >= MAX_OPCODE` (51) over `#[repr(u8)]` contiguous
`0..=50` ending at `TryLockGuards`; all four recursion caps
(`MAX_REBUILD_DEPTH = 1024` in **both** `control_flow.rs` and `boolean.rs`,
`MAX_EXPR_DEPTH`/`MAX_STMT_DEPTH = 256`) are present and threaded;
`translate_pex` still wraps `decompile_script` in `catch_unwind`
(`crates/scripting/src/translate/mod.rs:112`). The `.hkx` hole that made this
verdict NOT CLEAN on 2026-08-16 is closed by `2fa2e351`, and a fresh sweep of
all eleven `Vec::with_capacity` / `vec![]` sites in `crates/hkx` found no
second one: `bones` is allocated *after* the `data_slice` calls that bound
`bone_count` by file length, `blocks`/`block_tracks` are `≤ 4096`, and
`control_count` is `u16`-derived and additionally capped at 4096 by
`read_spline_header`.

**The 99.996% decompile-rate claim — HONEST, and no longer robustness-only.**
`585fd872` added `expected_top_level_item_count` to
`crates/pex/examples/pex_corpus_smoke.rs`, so an `Ok` decompile whose body is
the wrong length now tallies as `decompiled_shape_mismatch` instead of scoring
identically to a correct one. The predicate is derived from
`decompile_script`'s own documented item-production rule (one item per
non-synthetic variable / property / auto-state function / named state) — it is
a shape check, not a fidelity check, but it is a real one and closes the
"discards the `Script`" criticism the last three reports carried.

**The `.psc`-vs-`.pex` fidelity gate — now partially executes.** #3017's fix
added `decompiles_the_handbuilt_fo4_pex_to_the_expected_ast`
(`crates/pex/src/lib.rs`), which reuses `build_sample()`'s hand-built `.pex`
and pattern-matches the exact resulting `Script` — the first non-`#[ignore]`d
test in the tree that calls `decompile_script` and inspects the tree.
The **recognizer-level** half is still ignored: all three
`crates/scripting/tests/pex_recognize_e2e.rs` tests and
`crates/pex/tests/r5_fidelity.rs` remain `#[ignore]`-gated on Skyrim SE game
data. The fix commit explicitly names that as a deliberate scope boundary
("a checked-in fixture there would need to hand-craft bytecode matching that
exact recognized pattern"), so it is carried here as an acknowledged remainder
rather than re-filed.

## Decompiler Soundness Matrix

| Pass | Bounds-safe | Terminates | Total (no panic) | Fidelity-tested |
|------|:---:|:---:|:---:|:---:|
| Reader (`reader.rs`) | Yes | Yes | Yes | Yes (5 negative-path tests) |
| CFG (`cfg.rs`) | Yes | Yes | Yes | Yes |
| Lift + copy-prop (`lift.rs`) | Yes | Yes (#2024 linear chain intact) | Yes (#2666 fail-closed `Err` intact) | Yes |
| Boolean (`boolean.rs`) | Yes | Yes (`MAX_REBUILD_DEPTH`, own `pass` tag) | Yes (#2667 local-decline + self-referential-edge guard intact) | Partly — `falls_through_to_rejoin` carries it structurally; the executing end-to-end gate is now `decompiles_the_handbuilt_fo4_pex_to_the_expected_ast`, which does not exercise this pass |
| Control-flow (`control_flow.rs`) | Yes | Yes, same cap | Yes (fail-closed #1732 intact) | Partly — same |
| Lower (`lower.rs`) | Yes | Yes | Yes | Yes for the straight-line + property + event shape (#3017). `lower_binary_op`'s `_ => Eq` default arm re-confirmed structurally unreachable for the seventh consecutive pass |

`event_names.rs` re-validated mechanically this pass: **267 entries, strictly
sorted, no duplicates, all lowercase**, and all seven high-frequency events the
recognizer-scaling doc names (`onactivate`, `onload`, `ontriggerenter`, `onhit`,
`ontimer`, `oninit`, `onupdate`) are present.

The two documented Champollion departures remain adjudicated **benign as
currently guarded** — unchanged reasoning from 2026-08-16, and neither file
changed in this window.

## Decline-Invariant Audit

| Decline point | Verdict |
|---|---|
| `classify_guard_atom` `?` inside `classify_if_condition`'s per-atom loop | Conservative — an unclaimed atom propagates `None` |
| `split_and` refusing to split `\|\|` | Conservative, deliberate |
| `lower_fragment`'s `_ => return None` statement arm | Conservative; the one `Stmt::While` exception is still exactly `lower_3d_loaded_wait` (OR-tree of `!Is3DLoaded` + one positive `Utility.Wait`) |
| `receiver_object` | Conservative — explicit `key == "self"` plus `quest_locals` / `player_locals` / `decl_locals` / `known_quest_properties` rejections (#2538/#2657 both live) |
| `prim_start_quest` / `prim_stop_quest` vs `prim_start_scene` / `prim_stop_scene` (the zero-arg `Start()`/`Stop()` collision) | Conservative in **both** directions — the quest primitives require `explicit_quest_receiver`, and `receiver_object` declines a `known_quest_properties` identifier so a Quest Property cannot fall through and be claimed as a scene (#2538). Re-verified this pass because the primitive-table order alone does not carry it |
| `prim_reset_quest` / `prim_set_quest_active` (`Reset`/`SetActive` shared with `ObjectReference`/`Cell`/`Weather`) | Conservative — `explicit_quest_receiver` (#2653) |
| `prim_activate` | Conservative — `abDefaultProcessingOnly == true` declines; a non-`GetPlayer()` runtime activator resolves through `receiver_object` or declines |
| `prim_equip_item` | Conservative — `abPreventRemoval == true` declines rather than losing the lock contract |
| `bool_arg` three-case contract | Conservative — present-but-non-literal declines the whole primitive |
| `AddItem` 4th-arg / `MoveTo` offset-arg declines | Intact |
| `translate_pex` on bad bytes **or** a decompiler panic | Clean `None`, `catch_unwind` still present |
| `QuestRef::Property` on an alias-bound entry | Still declines (correct) |
| `SceneActorBindings::resolve` on an unfilled alias | Returns `None`, never fabricates an entity |
| **No `Lock`/`Unlock`/`SetLockLevel` primitive exists** | Declines correctly — but the decline is now load-bearing in a way it was not before `1e9723ab`. See SCR-D5-2026-08-20-01 |

No leak found. The decline invariant itself is clean for the fourth consecutive
pass; the one Dim 5 finding is about a *missing* primitive, not a leaking one.

## Runtime Lifecycle Invariant Matrix

| Invariant | Verdict |
|---|---|
| Marker drain coverage (`event_cleanup_system`) | **16** markers drained — `SplashEvent`/`RippleEvent` were added in lockstep by `36fb9e78`. Their producers (`submersion_system` @ `Stage::PostUpdate`, `make_water_interaction_system` @ `Stage::Late`) and their consumer (`water_audio_system` @ `Stage::Late`) are all registered *before* `event_cleanup_system` (`boot.rs:1422`, the last registration in the builder), so the one-frame contract holds. The 10 batch/request components that self-drain in their consumer are #2672, still open, not re-filed |
| Two-phase lock-drop — `timer_tick_system` (`timer.rs:48`), `recurring_update_tick_system` (`recurring_update.rs:168`) | Explicit `drop()` before the second acquisition |
| Two-phase lock-drop — `trigger_detection_system` | Block-scoped phase 1, phase 2 after (`trigger.rs:139-145`) |
| `quest_fragment_dispatch_system` clone-before-lock | Intact (`fragment.rs:1534`); `DeferredFragmentEffects` still snapshots before the guards |
| Residual nested component locks inside `apply_effect` | Documented in-source with its exclusive-scheduling justification; every quest-resource system is `add_exclusive` in `boot.rs` — re-verified |
| Journal poll ordering | **Fixed** — `frags.is_empty()` now returns at `fragment.rs:1538`, *before* the destructive `poll_quest_events` at `:1554` (#3012 / `eca04dce`) |
| Cascade bound | `MAX_CASCADE = 64` (`fragment.rs:1322`) with WARN; #2124 guard compares `previous_stage != new_stage` |
| Fragment-activation flush ordering | Intact — `fragment_activation_flush_system` at `boot.rs:829` precedes `rumble_on_activate_dispatch` (`:831`), `quest_advance_dispatch` (`:840`), `two_state_activator_system` (`:871`) and `mg07_on_activate_dispatch` (`:945`); the `fragment_activation_order_tests` source pin still guards the first three |
| Edge-trigger seed (`occupant_inside: None`) | Intact at both producer and consumer (`trigger.rs:131-136`) |
| CTDA OR-precedence | Intact — block scan while `conditions[i].or_next`, `.any()`, early-return on a false block, empty list → `true`, plus the trailing-`or_next` clamp at `condition.rs:800` |
| `set_stage` history retention | Intact |
| `HasPerk` reads a component any live actor actually carries | **Defect — SCR-D6-2026-08-20-01** |

## Findings

### MEDIUM

#### SCR-D6-2026-08-20-01: #2940's `HasPerk` fix reads a component the player never gets and only FO4+ NPCs ever get — the function is still structurally 0.0 on Skyrim, FO3 and FNV

- **Severity**: MEDIUM
- **Dimension**: Scripting Runtime Systems (Dimension 6)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/condition.rs:690-703` (the read);
  `byroredux/src/npc_spawn.rs:204-215` (the only production writer);
  `crates/plugin/src/esm/records/actor/mod.rs:1082-1086` (the `PRKR` parse arm);
  `crates/plugin/src/esm/reader.rs:236-238` (the gate);
  `byroredux/src/scene.rs:1170-1220` (the player-entity component set)
- **Status**: NEW
- **Description**: `a605ee93` (Fix #2940) correctly repointed
  `ConditionFunction::HasPerk` from the dead `PerkList` projection to the
  canonical `byroredux_core::character::Perks`, and the FormID spaces line up —
  `Perks::perk_form_id` is written through `remap_fid` and `param_1` is
  load-order remapped by `remap_condition_form_ids` for indices 448/449, so
  the comparison is apples-to-apples. What the fix did **not** change is who
  writes `Perks`. There is exactly one production writer: `spawn_npc_entity`,
  and it is fed by `NpcRecord::perks`, which is populated only inside the
  `captures_av_props = game.uses_actor_value_properties()` gate — i.e.
  `Fallout4 | Fallout76 | Starfield`. Separately, the **player** entity
  (`scene.rs`, the `PlayerEntity` body) is given `Transform`,
  `GlobalTransform`, a character controller, `CollisionShape`, `RigidBodyData`
  and a `FormIdComponent`, and nothing else from the CHARAL family — no
  `Perks`, no `ActorValues`. `HasPerk`'s own doc-comment claims indices
  **449 (FO3/FNV)** and **448 (Skyrim)**; for neither of those families, and
  for the player in *any* game including FO4, can the `world.get::<Perks>()`
  at `condition.rs:697` ever return `Some`.
- **Evidence**:
  ```rust
  // condition.rs:696 — the read
  let Some(perks) = world.get::<Perks>(entity) else { return 0.0; };
  ```
  ```rust
  // npc_spawn.rs:204 — the only writer
  if !npc.perks.is_empty() { world.insert(placement_root, Perks { .. }); }
  ```
  ```rust
  // reader.rs:236 — the gate on the only producer of `npc.perks`
  matches!(self, Self::Fallout4 | Self::Fallout76 | Self::Starfield)
  ```
  `grep -rn "Perks" byroredux/src crates` outside `crates/core/src/character`
  returns those two sites plus `condition.rs` and two save-registry notes —
  no player-side insert anywhere.
- **Impact**: Perk-gated dialogue, quest and package CTDAs silently evaluate
  false for the player in every game and for every NPC outside FO4/FO76/
  Starfield. This is the *same observable behaviour* CHAR-D3-01 (#2940)
  described and was closed for, so the closed issue reads as resolved while
  the user-visible symptom is unchanged for the reference title (Skyrim) and
  for the reference-of-record (FNV). A condition returning `0.0` is the
  Bethesda-correct safe default in isolation, which is exactly why it is
  silent: there is no log, no telemetry and no test that distinguishes
  "actor genuinely lacks the perk" from "no actor in this game can ever have
  one".
- **Related**: #2940 (CLOSED — the fix is correct as far as it goes),
  #2947, #2944; the ESM-side question "does Skyrim `NPC_` carry `PRKZ`/`PRKR`,
  and if so should `uses_actor_value_properties` gate it?" belongs to
  `/audit-esm` Dim 4, not here — this finding deliberately does not assert
  the Skyrim wire format
- **Suggested Fix**: Two independent halves. (a) Give the player entity a
  `Perks` component (empty is fine) at spawn so the distinction between
  "checked and absent" and "unrepresentable" exists at all, and so a future
  `AddPerk` effect has somewhere to write. (b) Either widen the `PRKR` parse
  gate past `uses_actor_value_properties` for the games whose `NPC_` actually
  carries it, or add a one-line `log::debug!` at the `else` arm of
  `condition.rs:697` naming the game, so the structural zero is at least
  diagnosable. A regression test asserting `HasPerk` is non-zero for a
  Skyrim-parsed NPC would pin whichever choice is made.

#### SCR-D5-2026-08-20-01: no `Lock`/`Unlock` effect primitive exists, and `1e9723ab` just made that gap a one-way door — a fragment that unlocks a door declines *wholesale*, taking its sibling `SetStage` with it

- **Severity**: MEDIUM
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/effects.rs:57-140`
  (the `Effect` enum), `:398-431` (`EFFECT_PRIMITIVES`);
  `byroredux/src/interaction.rs:936-943` (the new gate);
  `byroredux/src/components.rs:94-107` (`Locked`);
  `byroredux/src/cell_loader/spawn.rs:828-836` (the only insert)
- **Status**: NEW
- **Description**: `1e9723ab` (Fix #3098) introduced a `Locked` marker,
  stamped from an authored `XLOC`, and made `activation_is_blocked` return
  `true` on its presence — the commit calls this "the deliberately blunt
  first policy". The blunt half is documented. What is not documented, and
  is the part that lives in this domain, is that **nothing anywhere in the
  engine removes `Locked`**: `grep -rn "Locked" byroredux/src crates` shows
  one insert (`spawn.rs:832`) and one read (`interaction.rs:941`), no
  `world.remove::<Locked>` on any path, and the `Effect` enum has 33
  variants covering quests, objectives, items, scenes, player control,
  vehicles, idles and cinematics but no `Lock`, `Unlock` or `SetLockLevel`.
  The scripting consequence is sharper than "the feature is missing":
  `lower_fragment` is a flat-sequence lowerer whose `_ => return None` arm
  declines the **entire fragment** on one unmodeled statement. A vanilla
  `QF_` fragment shaped like
  `MyDoor.Lock(false)` + `SetObjectiveCompleted(10)` + `SetStage(20)`
  therefore contributes *nothing* — the objective and the stage advance are
  discarded along with the unlock. That is the correct decline (a partial
  lowering would be worse), but it means the missing primitive costs more
  than the unlock itself.
- **Evidence**:
  ```
  $ grep -rn "Locked" byroredux/src crates --include="*.rs" | grep -v test
  byroredux/src/interaction.rs:941:    if world.get::<Locked>(entity).is_some() { return true; }
  byroredux/src/cell_loader/spawn.rs:832:            Locked {
  ```
  `EFFECT_PRIMITIVES` (`effects.rs:398`) — 33 entries, none matching
  `Lock` / `Unlock` / `SetLockLevel`.
  The scripted-activation path is unaffected and correct:
  `Effect::Activate` reaches `ActivateEvent` through
  `PendingFragmentActivations` → `fragment_activation_flush_system`, which
  never consults `activation_is_blocked` — matching Papyrus, where
  `Activate()` bypasses lock state. Only the **player's** interaction path
  is gated, and only the player's path can traverse a door.
- **Impact**: Every authored-locked door and container is impassable for the
  whole session in every target game (the #3098 commit message counts 378
  locked REFRs, 103 keyed, on vanilla `FalloutNV.esm` alone), with no key
  check, no lockpick, and now no scripted escape either. Any quest whose
  progression depends on a script unlocking a door is unfinishable, and the
  fragment that would have done it silently contributes zero effects rather
  than partial ones — so the failure presents as "the quest stalled", not as
  "the door is locked".
- **Related**: #3098 (CLOSED — the interaction half is a documented
  deliberate deferral; the *scripting* half is not mentioned there),
  #2289 (new effect primitives lacking decline-path tests)
- **Suggested Fix**: Add `Effect::SetLocked { target: ObjectRef, locked: bool }`
  behind a `prim_lock` matching `ObjectReference.Lock(abLock)` /
  `.SetLockLevel(..)` with the same conservative-shape discipline
  `prim_set_open` uses (literal-only bool via `bool_arg`, decline on any
  extra argument), and have `apply_effect` insert/remove the `Locked`
  component. That is a small, self-contained increment and it converts the
  wholesale fragment decline into a working one. If the effect is not wanted
  yet, at minimum record the coupling in `Locked`'s docstring so the next
  reader of `interaction.rs:941` knows nothing can clear it.

#### SCR-D7-2026-08-20-01: `m47-triggers.sh` — the domain's only engine-side gate — has no assertion that can fail on a script-attach regression, and reaches only the interior path

- **Severity**: MEDIUM
- **Dimension**: Engine Attach & Trigger Wiring (Dimension 7)
- **Untrusted-Input**: No
- **Location**: `docs/smoke-tests/m47-triggers.sh:29-32` (the SOFT
  declaration), `:76` (`--cell`), `:137-146` (the only HARD assertion),
  `:148-168` (the SOFT block); sibling `docs/smoke-tests/m43-quest-runtime.sh:56`
  (also `--cell`)
- **Status**: NEW (the `--cell` half was noted inside
  SCR-D7-2026-08-16-01's *Impact* paragraph; that finding is now closed as
  #3010 and the observation went with it. The harness itself has never been
  filed)
- **Description**: The harness exists to prove "the engine decompiles vanilla
  `.pex` at cell load and spawns XPRM trigger volumes on real game data". Its
  only exit-code-affecting assertion is `entities >= ENTITY_FLOOR` with
  `ENTITY_FLOOR=300` against an observed ~1900 — i.e. "a Skyrim interior
  loaded". Both M47.2 counts are explicitly SOFT: `recognized == 0` and
  `triggers == 0` each print a WARN and leave `hard_fail` untouched. Deleting
  `attach_vmad_scripts` entirely, or breaking `pex_archive_path`'s
  `scripts\…\.pex` normalisation so every lookup misses, would leave this
  harness **green**. The stated justification — "their values depend on the
  cell's content and the mod load order, not on engine correctness" — does not
  hold for the default invocation, because the cell is pinned
  (`WhiterunBanneredMare`) and the script header itself asserts that for that
  cell "`REFRs recognized` should be > 0". A deterministic signal is being
  discarded as if it were nondeterministic. Separately, both this harness and
  `m43-quest-runtime.sh` launch with `--cell`, so neither reaches the
  exterior REFR-walk / fragment-population path at all.
- **Evidence**:
  ```sh
  # :137-146 — the whole HARD gate
  if (( entities < ENTITY_FLOOR )); then hard_fail=1; else echo "PASS"; fi
  # :159-163 — the recognition "assertion"
  if (( recognized == 0 )); then echo "WARN — zero REFRs recognized. …"; fi
  ```
  ```
  # :23-25, the header's own claim about the default cell
  # Cell choice: the default (WhiterunBanneredMare) loads reliably and has
  # scripted activators, so `REFRs recognized` should be > 0.
  ```
- **Impact**: The one instrument that exercises decompile → recognize →
  attach on real game data cannot report a regression in any of the three.
  Every "the attach path is live" statement in this report series rests on
  source reading, not on a gate. Note the exterior REFR walk itself was
  checked this pass and *does* share the interior accumulator and summary
  line (`exterior.rs:233`/`:1275` → `load_references_budgeted` →
  `complete_reference_load`), so the `--cell` limitation costs coverage of
  the exterior *fragment-population* path (SCR-D7-2026-08-20-02), not of
  REFR attach.
- **Related**: #3010 (CLOSED), #2541, SCR-D7-2026-08-20-02
- **Suggested Fix**: Promote `recognized == 0` to a HARD fail **when the cell
  is the pinned default and `--scripts-bsa` resolved** (leave it SOFT under
  `BYROREDUX_TRIGGER_CELL` override, where content genuinely varies) — the
  script already computes both values and already distinguishes the override
  case. Keep the trigger-volume count SOFT; towns really are sparse. Add a
  third invocation with `--grid`/`--radius` so the exterior path is covered
  at all.

### LOW

#### SCR-D7-2026-08-20-02: #3010 was fixed by adding a *second* `populate_quest_fragments` call site rather than consolidating, with no test, no source pin and an `is_empty()` guard that re-runs the whole walk on every streamed cell when the table legitimately stays empty

- **Severity**: LOW
- **Dimension**: Engine Attach & Trigger Wiring (Dimension 7)
- **Untrusted-Input**: No
- **Location**: `byroredux/src/cell_loader/exterior.rs:1050-1058` (the new
  call + guard), `byroredux/src/cell_loader/load.rs:441` (the original),
  `byroredux/src/asset_provider/script.rs:85-149` (the populator),
  `crates/scripting/src/fragment.rs:105-107` (`is_empty`), `:60-70`
  (the two independent maps)
- **Status**: NEW (#3010 is CLOSED and its behavioural defect **is** fixed —
  this is about the shape of the fix, which is a distinct, unfiled gap)
- **Description**: SCR-D7-2026-08-16-01's suggested fix was to move
  `populate_quest_fragments` *inside* `populate_scene_runtime` "so it cannot
  drift from its three siblings again", plus a `SRC.contains(...)` source pin
  of the kind `exterior.rs:455` already uses. Neither was done. Instead a
  second call site was added at the head of `ExteriorCellApplyJob::begin`.
  Functionally that is sufficient — both exterior entries
  (`streaming_helpers.rs:500` and `exterior.rs:998`) funnel through `begin`,
  so exterior sessions now populate. But the drift surface is unchanged in
  kind and larger in degree: there are now **two** populate sites against
  **four** `populate_scene_runtime` sites, and nothing — no unit test, no
  source pin, no smoke gate (see SCR-D7-2026-08-20-01, which is `--cell`-only)
  — would notice if the new one were dropped in a future refactor of the
  streaming job. Riding along: the new call is guarded on
  `QuestStageFragments::is_empty()`, which reads **only** the `map` field.
  `populate_quest_fragments` writes two independent maps — `insert_vmad`
  populates `vmad` for every scripted quest *before* any `.pex` is resolved,
  and `insert` populates `map` only on a successful lowering. So a session
  where the VMAD side populates but no `QF_` `.pex` resolves (wrong or
  missing `--scripts-bsa` — the exact case the smoke harness's own WARN text
  anticipates) leaves `map` empty forever, and the full 845-quest walk, with
  a per-quest `HashMap` build and an archive `extract_pex` per script name,
  re-runs on **every** exterior cell `begin` for the rest of the session.
- **Evidence**:
  ```rust
  // exterior.rs:1052 — the guard
  if world.resource::<byroredux_scripting::QuestStageFragments>().is_empty() {
      crate::asset_provider::populate_quest_fragments(world, &wctx.record_index);
  }
  ```
  ```rust
  // fragment.rs:105 — what is_empty() actually reads
  pub fn is_empty(&self) -> bool { self.map.is_empty() }
  // :63 and :69 — the two independent Arc<HashMap>s
  map:  Arc<HashMap<(QuestFormId, u16), Vec<Effect>>>,
  vmad: Arc<HashMap<QuestFormId, ScriptInstanceData>>,
  ```
  ```
  $ grep -rn "populate_quest_fragments" byroredux/src | grep -v tests
  byroredux/src/asset_provider/script.rs:85   (definition)
  byroredux/src/cell_loader/load.rs:441       (interior, unconditional)
  byroredux/src/cell_loader/exterior.rs:1057  (exterior, is_empty-guarded)
  ```
  — no test file references either call site.
- **Impact**: Bounded. The re-walk is not catastrophic (a `u16`-bounded BSA
  hash lookup per quest, a few dozen cells per streaming session), and the
  behaviour is correct in every case — only wasteful. The durable part is the
  unguarded second call site: #3010 was a HIGH that survived every prior audit
  precisely because nothing pinned the call, and the fix reproduced that
  condition one site wider.
- **Related**: #3010 (CLOSED), #2541, SCR-D7-2026-08-20-01
- **Suggested Fix**: Either consolidate as originally suggested, or add the
  `SRC.contains("populate_quest_fragments(")` source pin to
  `exterior.rs`'s existing pin test module — one line, and it is the exact
  mechanism that would have caught #3010. Separately, change the guard from
  `is_empty()` to a "have we already attempted this index" latch (a
  `populated_from: Option<*const EsmIndex>` or a plain `bool` resource), so
  "tried and found nothing" is distinguishable from "not yet tried".

## Existing / correctly-tracked — NOT re-filed

Verified still open and still accurate against current code:
**#3014** (`crates/hkx`'s asset test passes vacuously via a bare `return`
instead of `#[ignore]`; the crate still has no byte-level negative-input test —
note `2fa2e351` fixed #3011 without adding one, so #3014 now also covers the
missing regression guard for that fix), **#3019**
(`decompile/mod.rs`'s first-commit-era pipeline docstring, wrong pass order),
**#2671** (alias match-CTDAs read the previous refresh's binding table),
**#2672** (`cleanup.rs` drain contract vs the 10 self-draining markers),
**#2289** (new effect primitives with no decline-path tests — now materially
larger, `EFFECT_PRIMITIVES` is 33 entries), **#2290**
(`translate/source.rs` module doc claims no `.pex` parser exists), **#2540**
(widened `SetObjective*` `i32` has no range test), **#2541** (no test pins the
`is_primary_synth` gate — note #3015/#3016 fixed the *behavioural* divergences
at that gate but did not add the missing pin), **#2542**
(`feature-matrix.md` pass order), **#2668** (`OffsetMap::to_original` linear
scan), **#2669** (`two_state_activator::vmad_bool` fallback), **#2670**
(inventory-grant rekey drops a grant with no `SceneAliasCandidate`), **#2267**
(`crates/hkx` `global_target` dead accessor), **#2153** / **#2270**
(lock-discipline documentation).

## Considered and disproved / dropped

- **"The exterior REFR walk has its own attach path and never prints the
  `M47.2 scripts:` summary."** Disproved: `exterior.rs:233` and `:1275` both
  call `references::load_references_budgeted`, which shares
  `ReferenceLoadJob`/`RefLoadAccum` with the interior loader and terminates in
  `complete_reference_load` (`complete.rs:172-182`). Exterior REFRs attach
  scripts and spawn trigger volumes through exactly the same code, and the
  summary line does print outdoors. Only *fragment population* was
  interior-only, and that is #3010, now fixed.
- **"`prim_start_scene` will claim `SomeQuest.Start()` when
  `explicit_quest_receiver` declines."** Disproved: `receiver_object`
  (`effects.rs:1108-1116`) rejects any identifier in `known_quest_properties`,
  `quest_locals`, `player_locals` or `decl_locals`, and the #2538 comment
  documents that exact scenario. The whole statement declines, as intended.
- **`read_debug_info`'s nested `with_capacity` amplification** —
  `function_count` (≤ 65535) × `instr_count` (≤ 65535) looks like an 8 GB
  reservation. It is not: only the *current* iteration's ≤ 131 KB
  `line_numbers` vec is speculative, and every element it holds requires two
  real bytes from the file, so the loop EOFs long before the outer count
  matters. Peak stays ≈ file size. Sound.
- **`decode_quaternion` supporting only quantizations 1 and 5 while
  `quaternion_layout` accepts 0/2/3/4** — a clip using an unsupported
  quaternion encoding produces `UnsupportedLayout` rather than a wrong pose,
  and the consumer degrades to "no idle installs". A coverage boundary,
  correctly fail-closed, not a defect.
- **`bspline_weights`'s span-search `while` loop** — re-checked at HEAD after
  #3011's changes; `read_spline_header`'s `degree ∈ 1..=8`,
  `control_count ∈ (degree, 4096]`, `knots.len() == control_count + degree + 1`
  and monotonic-knot checks, plus the `low_knot < time < high_knot`
  precondition on the branch that reaches it, still guarantee termination and
  in-range indexing. Sound.
- **`cinematic.rs:480`'s `unreachable!()`** — inside a `keys.windows(2)` loop,
  where every yielded slice is length 2 by construction. Sound.
- **`Perks::perk_form_id` vs `condition.param_1` FormID-space mismatch after
  `a605ee93`** — the old `PerkList` path resolved through `FormIdPool` to a
  *local* id; the new path compares raw. Checked: `PRKR` is read through
  `remap_fid` (`actor/mod.rs:1084`) and `param_1` is remapped for indices
  448/449 by `remap_condition_form_ids` (`condition.rs:453`,
  `param1_is_form_id` includes both). Both sides are global. The comparison is
  correct; the defect is upstream of it (SCR-D6-2026-08-20-01).
- **`frags.is_empty()` also discarding the legacy `QuestStageAdvancedBatch`
  ingress** — real (the batch markers are drained by `event_cleanup_system`
  the same frame), but `fragment.rs:1521-1524` documents batches as a
  compatibility surface "for direct tests/tools" with the sequenced journal
  authoritative, and no production path emits a batch that the journal does
  not also carry. Dropped.

## Future-Phase Readiness

- **The two "fix landed, effect did not" findings (SCR-D6-2026-08-20-01,
  SCR-D7-2026-08-20-02) are this pass's real signal.** Six findings were
  closed in four days; two of the six closed a symptom without closing the
  cause, and one of those (#2940) is now recorded as fixed for a game family
  where it demonstrably still returns the safe default. Re-verifying *closed*
  issues against live code is worth as much per unit of effort in this domain
  as reading new code — the decompiler and parser dimensions have now been
  clean for four consecutive passes.
- **SCR-D7-2026-08-20-01 is the cheapest durable win.** One `if` promoted from
  WARN to `hard_fail=1` converts the domain's only real-data instrument from a
  liveness check into a regression gate, and it is the precondition for
  trusting any future exterior-path or Obscript/SCTX (Phase 5) claim.
- **`crates/hkx` is now bounds-clean but still test-poor.** #3011 and #3013 are
  both fixed; #3014 (the vacuous asset test, no negative-input coverage
  anywhere in the crate) is the one that remains, and it is the reason both of
  those got in. The crate still has no owner of its own — this dimension found
  nothing new in it this pass only because the previous pass found everything
  that a first read finds.

## Findings Count

**4 new: 0 CRITICAL / 0 HIGH / 3 MEDIUM / 1 LOW.**

By dimension — **Dim 1** (`.pex` reader & opcode decode): 0. **Dim 2**
(decompiler CFG & lift): 0. **Dim 3** (control-flow / boolean / lower): 0.
**Dim 4** (`.psc` lexer & Pratt parser): 0. **Dim 5** (recognizer-chain
soundness): 1 MEDIUM. **Dim 6** (scripting runtime systems): 1 MEDIUM.
**Dim 7** (engine attach & trigger wiring): 1 MEDIUM + 1 LOW. **Dim 8** (Havok
idle / cinematic slice): 0.

TALLY: CRITICAL=0 HIGH=0 MEDIUM=3 LOW=1
