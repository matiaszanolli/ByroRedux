# Scripting Subsystem Audit — 2026-09-29

**HEAD**: `9fcfdc3fc` · **Baseline**: `docs/audits/AUDIT_SCRIPTING_2026-09-22.md` (HEAD `ee6d3fb39`) ·
**Audited**: Dim 1 (Recognizer Chain), Dim 2 (Fragment Dispatch & Locks), Dim 3 (Event Runtime), Dim 4 (Attach &
Reference Identity), Dim 5 (Scene/Package/Dialogue/Cinematic), Dim 6 (Legacy ObScript) ·
**Unchanged since baseline (skimmed)**: Dim 7 (Provider/Extender Layer — zero scripting-side commits; guards spot-checked)

Run as part of `/audit-suite --preset comprehensive`. Single agent, no sub-agents: every dimension was analysed
directly and written to `/tmp/audit/scripting/dim_N.md` before the next; this report was reconciled against all
seven scratch files. Dedup baseline: pre-fetched open issues (`/tmp/audit/issues.json`, 163 open), `gh issue
list --state closed --search …` per finding, and today's sibling reports (`AUDIT_{ECS,CONCURRENCY,GAMEPLAY,
ESM,PERFORMANCE}_2026-09-29.md` cover most of the new dialogue code's lock, access-row, perf and UI defects and are
cross-referenced, not re-filed).

## What changed since 2026-09-22

47 files, +3,920 / −506 in this skill's scope. The ones that matter here:

| Commit | Effect on this domain |
|---|---|
| `ab31cfefe`, `766e1746e` | **P4 dialogue**: new `systems/npc_dialogue.rs` (activation → topic selection, native response surface), `running_quests_binding_entity`, `select_first_info`, Skyrim DIAL `QNAM` ownership |
| `ee952b499`, `996bf7315` | Alias refresh perf: `CandidateIndex` per-fill buckets, `IdentityIndex` + `subject_requirement` for condition-only fills; `evaluate` rewritten over `or_blocks` |
| `9789d8153`, `424aad268` | #4414 directed `SetEnemy` (both neutral flags carried), #4816 `StopCombat` primitive |
| `cd4fc019a`, `017c5e8a5` | #4415/#4822 `AddSpell`/`RemoveSpell` primitives + `magic.rs` runtime (`SpellList`, `SpellCatalog`) |
| `d3e043d3e` | #4334 `ReferenceScriptState` (once-only trigger park + reload consult) |
| `f87490826` | #4813 authored Initially-Disabled joins the spawn gate (`ReferenceEnableState` becomes tri-state), #4820 `PlacementContentWithheld` |
| `a70b54f14` | #4694 PlayerRef `0x14` resolved inside `resolve_entity_by_global_form_id` |
| `078f650ec` | `trigger_detection_system` persistent-scratch rewrite |
| `198adebe2` | #4612 `QuestRevision` change key on quest/objective state |
| `2f8538334` | **Fixes #4750 and #4751** (ObScript VM depth cap; doc link) under an unrelated commit subject — both issues still OPEN |

## Build & test state — CLEAN

```
$ TMPDIR=/mnt/data/tmp cargo test -p byroredux-scripting -j 8           496 passed; 0 failed; 1 ignored  (1 dead_code warning: obscript_vm.rs:869 test helper `r`)
$ TMPDIR=/mnt/data/tmp cargo test -p byroredux --bin byroredux -j 8 -- \
    boot::schedule cell_loader::references reference_enable_gate npc_dialogue \
    cinematic strip_ active_tether attach::tests extensions                185 passed; 0 failed; 1 ignored
```
(The ABBA lock-order lane is red at HEAD — that is ECS-2026-09-29-D1-01/-D1-02, not re-run here.)

## Executive Summary

- **Shipped and verified this cycle**: directed `SetEnemy`, `StopCombat`, `AddSpell`/`RemoveSpell` all follow the
  "literal-default or decline" discipline against the real Skyrim signatures; #4334 once-only park/consult is keyed
  consistently and stamped before attach; #4694 closes the resolver half of last cycle's SCR-D3-2026-09-22-01; the
  alias-refresh pre-filters are exact supersets; the trigger rewrite preserves edge semantics; `QuestRevision` is
  bumped by every mutator; #4750/#4751 are fixed in code.
- **New findings**: 0 CRITICAL, 1 HIGH, 1 MEDIUM, 3 LOW (5 total).
  - HIGH: `GetIsID` compares the placed-reference FormID against a base-object parameter — it is false on the very
    actor it names (SCR-D3-2026-09-29-01). This is also the evaluator half of the new P4 dialogue's speaker filtering.
  - MEDIUM: live `Enable()`/`Disable()` of a resident reference only writes the ledger; #4813 made that reachable on
    vanilla quest flow (SCR-D4-2026-09-29-01).
- **Untrusted-input verdict (runtime-side decoders)**: `SCDA` bytecode — **NO panic / OOB / unbounded recursion / OOM
  found** (`MAX_OBSCRIPT_VM_NESTING` now on both recursive paths; operand stack bounded by the u16 payload). Provider
  manifests / `.pex`-derived provider lowering — unchanged since 09-22, caps intact. The `.pex`/`.psc` frontends'
  verdict lives in the newest `AUDIT_PAPYRUS_*`.

## Decline-Invariant Audit (Dims 1, 6)

| Decline point | Verdict |
|---|---|
| `classify_guard_atom` / `split_and` (`||` not split) | Holds, untouched |
| `lower_statements` `While`/`If` exceptions, `MAX_CONDITIONAL_DEPTH` | Holds, untouched |
| `prim_set_enemy` (1..=3 args, omitted flag = declared default, non-literal declines) | Holds — verified vs `faction.psc` |
| `prim_stop_combat` (0 args, player receiver declines) | Holds |
| `prim_add_spell` (non-literal `abVerbose` declines; literal dropped — no HUD message modeled) / `prim_remove_spell` (exactly 1 arg) | Holds |
| `every_effect_primitive_bounds_its_argument_count`, `only_the_counter_only_primitives_are_placeholders` | Pass |
| Named call arguments (`CallArg::name`) in the canonical table | **Ignored — binds positionally** (SCR-D1-2026-09-29-01, LOW; unreachable from `.pex`) |
| `classify_effect_with_providers` canonical-first; provider seam handles names | Holds, untouched |
| `translate_pex` panic net | Holds, untouched |
| ObScript VM recursion (`exec_if_chain`↔`run_arm_body`, `decode_args_at_depth`) | **Fixed** (#4750, issue still OPEN) |
| ObScript unknown commands → traced no-op, counted | Holds |

## Runtime Lifecycle Matrix (Dims 2–5)

| Invariant | Verdict |
|---|---|
| Nested-lock safety = exclusive scheduling | Holds; inventory drifted — `SpellList`/`SpellCatalog`/`ActorValues` via `magic.rs` unlisted and invisible to the #3949 scan (SCR-D2-2026-09-29-01, LOW) |
| `SetEnemy` deferred past quest guards; `LoadOrderIdentity` then `FactionRelations`, never both | Holds |
| Cascade bound / cursor-stack `apply_effects` / populate-replace | Untouched |
| Quest journal (3 subscribers) + new `QuestRevision` bumped by every mutator, unsaved | Holds |
| FormID-keyed ledgers: `ReferenceEnableState` (now tri-state), `ReferenceLockState`, `ReferenceScriptState` (#4334) | Hold — keyed by FormID, not entity |
| Live consumer of an enable/disable edge on a resident reference | **Missing** (SCR-D4-2026-09-29-01, MEDIUM) |
| Marker drain coverage (Pattern A/B) | Holds; no new marker type; `ItemEventBatch` still Pattern A (no consumer, #4713) |
| `ActivateEvent` consumers | Update order pinned (#4712/#4116 closed); Late `npc_dialogue_selection` before `event_cleanup_system` |
| Two-phase lock drop — `trigger_detection_system` | `TriggerVolume` guard scoped before marker insert; private `TriggerOccupancyState` write guard now spans the system (edges out only, no reverse acquirer) |
| Edge-trigger seed (`occupant_inside: None`), `triggerers` merge | Holds after rewrite |
| CTDA OR-precedence (`or_blocks`) / empty list = true | Holds |
| CTDA identity functions | **`GetIsID` wrong parameter space** (SCR-D3-2026-09-29-01, HIGH) |
| PlayerRef `0x14` through the shared resolver | **Fixed** (#4694) |
| Alias pre-filters (`CandidateIndex`, `subject_requirement`/`IdentityIndex`) are exact supersets | Holds |
| Once-only park/consult (#4334) | **Fixed** and verified |
| Router/gate shared predicate (#4333), cinematic lock order (#4546) | Untouched |
| Cinematic retention lifetime | #3817 still OPEN, cited |

## Findings

### HIGH

#### SCR-D3-2026-09-29-01: `GetIsID` compares the Run-On's placed-reference FormID, but its parameter is a base-object FormID — every positive `GetIsID(<NPC_>)` evaluates false on the actor it names

- **Severity**: HIGH
- **Dimension**: Event Runtime (CTDA evaluator)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/condition.rs:321-336` (`ConditionFunction::run_on_identity`, `GetIsID` arm), used by
  `evaluate_condition` `:807-814` and by the alias pre-filter `IdentityIndex::narrowest`
  (`crates/scripting/src/scene/quest_alias.rs:404-440`); identity source
  `byroredux/src/cell_loader/references/synth_child.rs:53-64` (`FormIdComponent` local = `placed_ref.form_id`, the
  REFR/ACHR id).
- **Status**: NEW. #1666 (closed) asked for "the Run-On's base/placement FormID == param_1"; only the placement half
  shipped. Distinct from the unpublished SCR-D3-2026-09-22-01 remark about `GetIsID(0x14)` on the player (that was the
  sentinel; this is base vs reference for every actor).
- **Description**: `GetIsID ObjectID` tests the calling reference's **base object**. The engine's own census
  (`crates/plugin/src/esm/records/misc/dialogue.rs:362-395`, `speaker_from_conditions`) shows `param_1` resolves to an
  `NPC_` on 19,344 of 19,345 `Oblivion.esm` `GetIsID` conditions, and the plugin's CTDA arg table labels function 72
  "base FormID" (`records/condition.rs:452`). The evaluator instead compares the entity's `FormIdComponent`, which the cell
  loader stamps with the placed reference's own FormID. The base id is already on the same entity
  (`SceneAliasCandidate::base_form_id`; the player is stamped `{reference 0x14, base 0x7}`, `byroredux/src/scene.rs:1205-1213`),
  and `dialogue::actor_matches` (`crates/scripting/src/dialogue.rs:178-191`) already accepts reference **or** base — so an
  INFO's speaker check passes and the CTDA re-check of the same speaker then fails.
- **Evidence**:
  ```rust
  Self::GetIsID => {
      let fid_comp = world.get::<FormIdComponent>(entity)?;   // placed REFR/ACHR identity
      let pool = world.try_resource::<FormIdPool>()?;
      pool.resolve(fid_comp.0).map(|pair| pair.local.0)       // compared with a base NPC_ param_1
  }
  ```
- **Impact**: `GetIsID <base> == 1` is false on the actor it names, and `GetIsID <base> == 0` exclusions always pass. Live
  consumers: INFO selection through both the SCEN `select_info` path and the new P4 `select_first_info` activation path
  (`byroredux/src/systems/npc_dialogue.rs:120-140`); AI-package condition lists (all games); perk entry-point conditions;
  condition-only quest-alias fills (`IdentityIndex` buckets by the same value, so such an alias never binds). The failure is
  silent, with no log line.
- **Related**: GAME-D2-2026-09-29-01 (topic ownership ignores the speaker; this is the evaluator half of speaker
  filtering), #1666, #4694.
- **Suggested Fix**: have the `GetIsID` identity read `SceneAliasCandidate` and pass on `base_form_id == param_1`. Keep the
  reference id as a second accepted value only if a census shows REFR-valued params. `run_on_identity` must then return the
  base id (or a small set), so `IdentityIndex` stays an exact superset. Replace `get_is_id_matches_entity_global_form_id`
  with a test where an ACHR's base `NPC_` is `param_1`.

### MEDIUM

#### SCR-D4-2026-09-29-01: a scripted `Enable()`/`Disable()` on a resident reference only writes the ledger — since #4813 withholds every Initially-Disabled placement, a quest stage's `Enable()` no longer materialises the actor/object until its cell reloads

- **Severity**: MEDIUM
- **Dimension**: Attach & Reference Identity
- **Untrusted-Input**: No
- **Location**:
  - `crates/scripting/src/fragment/effects.rs:932-963`: the arm pushes to `deferred.reference_enable_changes`.
  - `crates/scripting/src/fragment/effects.rs:355-360`: the deferred apply is `ReferenceEnableState::set_enabled` only.
  - `byroredux/src/cell_loader/spawn.rs:712`: a disabled root gets `PlacementContentWithheld` and no mesh or collider.
  - `byroredux/src/cell_loader/references/mod.rs:656-676`: a disabled actor becomes an identity stub, with no NPC job.
  - `byroredux/src/components.rs:91-102`: the doc says "there is no live re-spawn".
  - `byroredux/src/interaction.rs:1345-1357`: "its meshes only go on the next load".
- **Status**: NEW. There is no open issue. #3278, #3489, #4698, #4813 and #4820 are all closed, and #4820's suggested fix
  defers the work "until live re-spawn exists". Nothing tracks that re-spawn.
- **Description**: before #4813, the authored "Initially Disabled" flag was ignored. References that a quest later
  `Enable()`s were therefore already (wrongly) present. #4813 now withholds them correctly, and #4820 blocked interaction
  with them rather than re-spawning. No system consumes the enable edge: `ReferenceEnableState` has no reader besides the
  spawn gate, interaction, and save. The lowering and the ledger are correct; the missing piece is the live consumer.
- **Impact**: the quest-gated references in the #4813 census stay absent after the stage fragment that enables them has
  run, until the cell reloads. Examples: Skyrim `HelgenKeep01` actors, the MS10 pirates, `MQ106DragonParchment`, and the
  FNV `Vault11c` turrets. An alias bound to such a stub drives scenes and packages with a bodiless actor that has only a
  transform. The reverse also holds: a live `Disable()` leaves the reference rendered and solid until reload.
- **Related**: #4813, #4820, #3278, #3489.
- **Suggested Fix**: on a resident reference's enable edge, run that REFR's placement spawn (or restore the withheld
  content). On a disable edge, strip the mesh, collider and volume from the root, keeping the identity (the #3278 posture).
  At minimum, record the deferral on ROADMAP M47 so it is visible.

### LOW

#### SCR-D1-2026-09-29-01: the canonical effect table ignores `CallArg::name` — a named argument binds positionally

- **Severity**: LOW
- **Dimension**: Recognizer Chain
- **Untrusted-Input**: No (unreachable from `.pex`)
- **Location**: `crates/scripting/src/translate/compose.rs:81-100` (`method_call`, `int_arg`),
  `crates/scripting/src/translate/effects.rs:1738-1743` (`bool_arg`), and every `prim_*`.
- **Status**: NEW
- **Description**: no helper in `translate/` inspects `CallArg::name`. The provider seam does
  (`papyrus_provider/lower_call.rs:357`, `lower_program.rs:531/550/678`). As a result,
  `SetEnemy(PlayerFaction, abOtherIsNeutralToSelf = true)` would lower as `self_neutral: true`, the inverted direction.
  This accepts a wrong AST.
- **Impact**: none on the production path today. `crates/pex/src/decompile/lower.rs:166-169` always emits `name: None`.
  Only hand-authored `.psc` reaches this code, and every current `parse_script` caller is a test, including the `.psc` half
  of the `recognizes_da10…` fidelity gate. This is a latent hole in the decline invariant.
- **Suggested Fix**: decline in `method_call` (or a shared argument accessor) when any argument carries a name, as the
  provider seam does.

#### SCR-D2-2026-09-29-01: `apply_effect`'s nested-lock inventory omits `SpellList` / `SpellCatalog` / `ActorValues` taken through `magic::{add_spell,remove_spell}`, and the #3949 scan cannot see call-outs

- **Severity**: LOW
- **Dimension**: Fragment Dispatch & Locks
- **Untrusted-Input**: No
- **Location**:
  - `crates/scripting/src/fragment/effects.rs:600-650`: the doc block.
  - `crates/scripting/src/fragment/effects.rs:712-730`: the arm, applied inline under the quest guards, unlike the deferred `SetEnemy`.
  - `crates/scripting/src/magic.rs:153-219`: `query_mut::<SpellList>`, then `try_resource::<SpellCatalog>`, then `query_mut::<ActorValues>`.
  - `crates/scripting/src/fragment/tests.rs:3856-3911`: the scan covers `fragment::SOURCES` bodies only.
- **Status**: NEW. This is the same class as #3949 (closed, LOW).
- **Description**: all three acquisitions run nested under `resource_2_mut::<QuestStageState, QuestObjectiveState>()`, and
  none of the three types is named in the inventory. The SKILL states that both magic types belong there. The guard scan
  matches acquisitions inside the fragment bodies only, so anything one module away (`crate::magic`) is invisible to it.
- **Impact**: no cycle today. Every quest-resource system is exclusive, and the ABBA lane's two cycles do not involve these
  edges. The Dim-2 checklist's delegated inventory is incomplete, and its guard is structurally blind to call-outs.
- **Suggested Fix**: add a "via `crate::magic::{add_spell,remove_spell}`" bullet that names the three types. Then either
  extend the scan to `magic.rs`, or defer the arm the way `SetEnemy` is deferred.

#### SCR-D5-2026-09-29-01: `running_quests_binding_entity`'s doc says a missing `QuestStageState` means the quest "never appears"; the code treats every installed quest as running

- **Severity**: LOW
- **Dimension**: Scene/Package/Dialogue
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/scene/quest_alias.rs:908-915` (the doc) vs `:923-928`
  (`running.as_ref().is_none_or(|stages| stages.is_running(*quest))`).
- **Status**: NEW. Routed from `/audit-concurrency` (the "Routed" section of `AUDIT_CONCURRENCY_2026-09-29.md`) and verified.
- **Description**: the code follows the crate's deliberate data-only convention. `refresh_scene_actor_bindings`
  (`:709-718`: "the live engine always installs QuestStageState") and `quest_alias_diagnostics` (`:180-182`) do the same.
  The doc is wrong, not the code.
- **Impact**: none in production. The doc is the only contract P4 callers have for this function.
- **Suggested Fix**: reword `:914-915` to say that a quest with no installed alias definition never appears, and that with
  no `QuestStageState` resource (tool and test worlds) every installed quest counts as running.

## Cross-referenced (filed today by sibling audits — not re-filed)

| ID | Covers |
|---|---|
| ECS-2026-09-29-D1-01 (HIGH) | Lock-order cycles from `populate_candidates` and `running_quests_binding_entity`. One cycle is visible only under `-p byroredux-scripting`. |
| ECS-2026-09-29-D1-02 (LOW) | Test-body back edges, including `cinematic_trio_survives_save_load_round_trip` and `quest_alias_collection_fill_type_still_declines_not_binds` |
| ECS-2026-09-29-D5-01 (LOW) | The `interaction_system` and `npc_dialogue_selection` Access rows. The second is missing `DialogueSurfaceState`. |
| ECS-2026-09-29-D6-01 / PERF-D1-2026-09-29-01 | The per-frame Talk-candidate scan over every reference and quest |
| ECS-2026-09-29-D7-01 (MEDIUM) | `dialogue_snapshot` reads the first `NpcDialogueTopic`, so a second conversation shows the wrong NPC |
| CONC-D3-2026-09-29-01 (LOW) | The `LoadedCellIndex` guard in `npc_dialogue` is shadowed rather than dropped |
| GAME-D2-2026-09-29-01 (MEDIUM) | Topic ownership ignores the speaker, the DIAL category and the branch structure |
| GAME-D2-2026-09-29-02 / GAME-D4-2026-09-29-01 (LOW) | The dialogue entry has no `player_can_act` or combatant gate. `StartCombat` keeps the ambient marker. |
| ESM-2026-09-29-D2-03 (LOW) | The DIAL ownership docs name only `QSTI`, not `QNAM` |

## Existing findings re-verified

| Issue | State at HEAD |
|---|---|
| #4750 (SCR-D6-2026-09-22-01, obscript_vm recursion) | **Fixed in `2f8538334`; issue still OPEN.** Close it. |
| #4751 (SCR-D6-2026-09-22-02, `ObScriptDiagnostics` link) | **Fixed in `2f8538334`; issue still OPEN.** Close it. |
| SCR-D3-2026-09-22-01 (unpublished; the player half of the resolver gap) | The resolver half was fixed by #4694 (`a70b54f14`). The `GetIsID` remark is superseded by SCR-D3-2026-09-29-01. |
| #4334 (onlyOnce re-arm) | Closed; fix verified, including the park/consult key symmetry and stamp-before-attach |
| #3817 (cinematic retention never terminates) | Still OPEN; cited |
| #4113 / #4115 | `/audit-papyrus` scope; not re-verified |

## Future-Phase Readiness

- **P4 dialogue**: the selection route works end to end, but its correctness is gated on three open items. SCR-D3-2026-09-29-01
  (the evaluator can't see speakers), GAME-D2-2026-09-29-01 (ownership), and ECS-D7-01 (the snapshot keyed on the wrong NPC).
  For Oblivion, FO3 and FNV the route is unreachable by design, because `running_quests_binding_entity` needs quest aliases.
- **Quest-driven world changes**: SCR-D4-2026-09-29-01 is the next structural gap in M47. Enable and Disable are lowered,
  persisted and gated at spawn, but have no live consumer.
- **ObScript phase 2**: the VM is now depth-capped on every recursive path. The pins
  (`excessive_nested_*`, `truncated_bytecode_reports_malformed`) are in place for object-script blocks and the Message UI.
- **Alias follow-ups** (the `m47-3-quest-alias-design.md` "Remaining subsystem boundary" list) are unchanged. After the fix,
  the perf pre-filters must keep treating `GetIsID` as an exact superset.

## Findings Count

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 1 |
| LOW | 3 |
| **Total (new)** | **5** |

By dimension: Dim 1 (1 LOW), Dim 2 (1 LOW), Dim 3 (1 HIGH), Dim 4 (1 MEDIUM), Dim 5 (1 LOW). Dims 6 and 7 found nothing new.

---
*Generated by the `/audit-scripting` skill (single-agent run). Suggested next step:*
`/audit-publish docs/audits/AUDIT_SCRIPTING_2026-09-29.md` (domain label `scripting`; add `dialogue` for SCR-D3-2026-09-29-01
and SCR-D5-2026-09-29-01, and `quests` for SCR-D4-2026-09-29-01).
