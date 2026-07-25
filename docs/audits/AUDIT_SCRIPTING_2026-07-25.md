# Scripting Subsystem Audit — 2026-07-25

Eighth full pass over the M30/M47 Papyrus/.pex/ECS scripting domain (prior
reports: `AUDIT_SCRIPTING_2026-06-23.md`, `_06-27.md`, `_07-02.md`, `_07-03.md`,
`_07-06.md`, `_07-16.md`, `_07-21.md`). Run directly, single-session, no
sub-agent delegation, as one leg of a `comprehensive` audit-suite sweep. All
seven dimensions covered against `crates/pex/`, `crates/papyrus/`,
`crates/scripting/`, and the engine-side attach path
(`byroredux/src/cell_loader/references/`, `byroredux/src/asset_provider/`,
`crates/plugin/src/esm/records/`).

**Dedup baseline**: `gh issue list --repo matiaszanolli/ByroRedux` (29 open
issues at audit time) plus direct `gh issue view` / `gh issue list --state
closed --search` confirmation of every prior-pass finding's disposition.

**Test baseline**: `cargo test -p byroredux-pex -p byroredux-papyrus -p
byroredux-scripting` — 80 + 4 + 49 + 187 unit tests, all passing, 0 failed
(plus 3 `#[ignore]`-gated `.pex` E2E tests needing Skyrim SE game data, and 2
`#[ignore]`-gated doctests). No regressions in the suite.

## Executive Summary

**What shipped, re-confirmed live, no regressions**: M30.2 `.psc` lexer/Pratt
parser; M47.0 event-hook runtime; M47.1 condition evaluation (13 catalog
functions, correct safe-default sentinels, plus the newly-closed
`RunOn::Reference` resolver — see below); M47.2 `.pex` reader + 5-phase
decompiler + recognizer chain + dynamic VMAD attach path + XPRM trigger
volumes + the fragment-lowerer + the QUST VMAD property-table wiring + the
`AddItem`/`MoveTo` object-targeting effects; M47.3 Phase 0 QUST alias decode
(out of crate scope, noted for context).

**Deferred, correctly, not flagged as defects**: Obscript/SCTX frontend
(Phase 5); the M47.1 condition resolvers' live-headless-cell re-verification;
M47.3 quest-alias-fill runtime (`Property`-resolution decline on an
alias-bound VMAD entry remains correct-by-design).

**All 8 findings from the 2026-07-21 pass (#2122–#2129) independently
re-verified FIXED in current code**, not just closed on GitHub — each fix was
re-read at its exact location and, where a regression test exists, confirmed
present and passing (see "Confirmed-fixed prior-audit findings" below). The
one 2026-07-21 finding correctly left open as informational (#2130,
`quest_advance_system`'s unenforced one-signal-per-entity assumption) is
still open and still accurately describes current code — not re-filed.

**Findings this pass**: 6 new (0 CRITICAL / **2 HIGH** / 2 MEDIUM / 2 LOW).
An earlier attempt at this same pass fanned out per-dimension sub-agents and
lost its own synthesizing context before merging their results — two of those
orphaned sub-agents had already independently verified their dimensions
adversarially (not just re-reading the fix, but constructing hostile inputs
against it) and surfaced two real HIGH-severity bugs that this direct
single-session run's Dimension 4 and Dimension 5 passes did not catch, because
it re-verified #2125's and the alias-decline invariant's fixes by reading the
code rather than by constructing an adversarial repro. Both orphaned findings
were independently re-confirmed before inclusion here (see below) rather than
taken on faith. This pass's own new findings, on top of those two, came from
dimensions outside the recently-touched fix surface: the item-record
VMAD-decode asymmetry (Dimension 7 / `crates/plugin`) and two doc-accuracy
gaps (this skill's own entry-point list; `m47-2-design.md`'s stale Phase-0
checklist item).

**Untrusted-input robustness verdict — CLEAN.** Independently re-verified:
every primitive read in `crates/pex/src/reader.rs` funnels through `take()`
(no bare slice index); the `OpCode::from_u8` transmute guard is `>=` not `>`
and `from_u8_round_trips_and_rejects_oob` iterates all 51 discriminants, not
spot values; hostile var-arg counts grow geometrically (never
`with_capacity(hostile_n)`); the CFG's jump-target bound and the CFG stale-key
fix (#2122) both hold under direct re-reading; both recursion caps
(`MAX_REBUILD_DEPTH` in `control_flow.rs` and `boolean.rs`, `MAX_EXPR_DEPTH`/
`MAX_STMT_DEPTH` in the Papyrus parser) are present and tested; the Papyrus
lexer's line-continuation `OffsetMap` byte counts (2/3/2 for `\n`/`\r\n`/`\r`)
and the trailing-backslash-at-EOF edge case were traced directly and are
correct. No panic/OOB/unbounded-alloc path found.

**The 99.996% (26640/26641) decompile-rate claim — re-verified honest** by
directly reading `crates/pex/examples/pex_corpus_smoke.rs`: `decompile_script`
runs inside `std::panic::catch_unwind`, and both the panic arm and the `Err`
arm increment failure counters (`decompiled_panic`, `decompile_failures`) that
feed the printed percentage — no swallowing.

**The `.psc`-vs-`.pex` fidelity gate — verified present and passing.**
`recognizes_da10_and_reproduces_hand_builder` re-ran clean in this pass's
`cargo test` run; `da10_pex_reproduces_hand_builder_byte_for_byte`
(`crates/scripting/tests/pex_recognize_e2e.rs`) is present, `#[ignore]`-gated
on Skyrim SE game data as before.

## Decompiler Soundness Matrix

| Pass | Bounds-safe | Terminates | Total (no panic) | Fidelity-tested |
|------|:---:|:---:|:---:|:---:|
| Reader (`crates/pex/src/reader.rs`) | Yes — `take()` sole gate, re-verified no bypass | Yes | Yes | Yes — 3 dialects round-trip; corpus-smoke harness independently re-verified honest |
| CFG (`cfg.rs`) | Yes — inclusive jump bound correct | Yes | Yes | **#2122 stale-block-key fix re-verified in place**: `find_block_for_instruction` re-resolved via `current_key` after both splits (`cfg.rs:244-245`); both `backward_jmpf_target_inside_own_block_conditions_the_right_block` and the `jmpt` sibling test present |
| Lift + copy-prop (`lift.rs`) | Yes | Yes (O(n²) fix from #2024 unchanged) | Yes — `Cast` heuristic's `as_identifier().unwrap()` re-traced and confirmed guarded by the preceding `matches!` short-circuit; `CallMethod`/`CallStatic`/`CallParent` operand indices re-cross-checked against the skill's expected order and match exactly | Yes |
| Boolean (`boolean.rs`) | Yes | Yes — `MAX_REBUILD_DEPTH=1024` present, `rebuild_rejects_excessive_recursion_depth` passes | Yes | Yes |
| Control-flow (`control_flow.rs`) | Yes | Yes, same cap present | Yes | Yes |
| Lower (`lower.rs`) | Yes | Yes | Yes | Yes |

Pass order re-confirmed directly in `decompile/lower.rs::decompile_body`:
`build_cfg` → `lift_function` → `rebuild_boolean_operators` → `reconstruct` →
`lower_body` — boolean collapse still runs before control-flow reconstruction,
as required.

`event_names.rs`'s `EVENT_NAMES` table independently re-verified: 267 entries,
programmatically confirmed sorted and duplicate-free (not just spot-checked),
and every high-frequency event named in the recognizer-scaling doc
(`onactivate`, `onload`, `ontriggerenter`, `onhit`, `ontimer`, `oninit`,
`onupdate`) is present.

## Decline-Invariant Audit

The recognizer-chain decline invariant (`crates/scripting/src/translate/`)
remains **sound, no new leaks found**. Directly re-read and confirmed:
`classify_guard_atom(atom, player_param)?`'s per-atom `?`-propagation in
`quest_stage_gate.rs`; `split_and`'s disjunction-stays-whole guarantee;
`effects.rs::lower_fragment`'s flat-sequence `classify_effect(...)?` /
`_ => return None` shape; `resolve_property_form_id`'s
`PropertyValue::Object { form_id, alias: -1 } => Some(form_id), _ => None`
match (the alias-bound decline is still exactly one match arm, not a
heuristic). One test-coverage gap first noted in the 07-21 pass **remains
unaddressed**: no fixture in `crates/scripting/src/fragment/tests.rs`
constructs a `PropertyValue::Object` with `alias != -1`, so the
correct-by-design alias-bound decline branch is still exercised only by
inspection, not a regression test. Still not a defect — carried forward in
Future-Phase Readiness rather than filed as a new finding, consistent with
the prior pass's treatment.

## Runtime Lifecycle Invariant Matrix

| Invariant | Status |
|---|---|
| Marker drain coverage | CLEAN — re-enumerated every `impl Component for` in `crates/scripting/src/**`, cross-checked against `cleanup.rs`'s 12-type drain list; every non-drained component (`RecurringUpdate`, `TriggerVolume`, `ScriptTimer`, `ActorStats`, `RumbleOnActivate`, `Dlc2Ttr4aPlayerScript`, `QuestAdvanceOnActivate`, `MG07LabyrinthianDoor`, `KeystoneInventory`) is persistent state/config, not a one-frame event, by direct inspection of each |
| Two-phase lock-drop (`timer_tick_system`, `trigger_detection_system`, `recurring_update_tick_system`) | CLEAN — `trigger_detection_system` uses a `{ }` block-scope drop rather than an explicit `drop()` call (functionally equivalent, re-verified no overlap); the other two use explicit `drop()` |
| `quest_fragment_dispatch_system` nested-lock safety | CLEAN, independently re-derived (not just trusted from the prior report): `crates/core/src/ecs/scheduler.rs` confirms stages run sequentially in discriminant order and, within a stage, all `parallel` systems complete before any `exclusive` system starts, and `exclusive` systems run strictly sequentially among themselves. `quest_fragment_dispatch` and every sibling quest-resource system are registered via `add_exclusive` in `boot.rs`. No ABBA cycle is reachable. The MEDIUM documentation-debt finding from 07-21 (#2126) is fixed — a doc comment on `apply_effect` now states this dependency explicitly |
| Cascade bound + genuine-transition guard | CLEAN — `MAX_CASCADE=64` bound present with `log::warn!` on overflow; #2124's fix (`adv.previous_stage != adv.new_stage`, not `adv.new_stage != stage`) re-verified in place |
| Edge-trigger seed | CLEAN — `occupant_inside: Option<bool>` lazy-seed intact, confirmed by direct read |
| CTDA OR-precedence | CLEAN — block-scan/`.any()`/AND-combine logic in `condition.rs::evaluate` re-traced line-by-line, matches spec exactly including the trailing-OR clamp and empty-list `true` contract |
| `RunOn::Reference` | CLEAN — #2123's fix in place: `resolve_entity_by_global_form_id(world, condition.reference_form_id)`, no longer an unconditional `None`; dedicated test `run_on_reference_resolves_the_entity_carrying_the_form_id` present and passing |
| `ScriptRegistry` hardcoded demo registration | **Still NOT retired** — see SCR-D6-NEW4-01 below (new finding, LOW) |

## Findings

### HIGH

#### SCR-D4-NEW4-01: Unterminated `State`/`Struct`/`Group` hangs the Papyrus parser forever — a regression introduced by the #2125 fix itself

- **Severity**: HIGH
- **Dimension**: Papyrus Lexer & Parser (Dimension 4)
- **Untrusted-Input**: Yes — any truncated or hand-malformed `.psc` file (network transfer cutoff, disk corruption, a mod author's file missing one closing keyword) triggers this
- **Location**: `crates/papyrus/src/parser/script.rs` — `parse_state` (loop at 519-578), `parse_struct` (588-617), `parse_group` (640-678)
- **Status**: NEW — regression introduced by the #2125 fix (commit `cacc9935`), independently found by an orphaned sub-agent from an earlier attempt at this audit and re-confirmed empirically in this pass
- **Description**: The #2125 fix correctly gave `parse_state`/`parse_struct`/`parse_group` a per-child recovery loop instead of a bare `?` — but none of the three loops check `self.at_eof()` before dispatching into their catch-all `_ =>` arm, unlike `parse_script`'s own top-level loop (`script.rs:74`, `if self.at_eof() { break; }`). At genuine EOF, `peek()` returns `None`, which falls into `_`. That arm calls `parse_type()` (or `parse_variable_body()`), which at EOF fails **without consuming any token** (`parser/mod.rs:365-386`). The error handler then calls `skip_to_next_line()`, which also consumes nothing at EOF (`advance_raw()` returns `None` immediately). Control falls through `continue` back to the top of the loop with `self.pos` unchanged — an infinite loop, 100% CPU, no progress, ever.
- **Evidence**: independently re-confirmed via a throwaway test (`crates/papyrus/src/parser/script.rs`, built, run, reverted — `git status`/`diff --stat` clean afterward) spawning `parse_script` on a thread with a 3-second `mpsc::recv_timeout`. Input `"ScriptName Foo\n\nState MyState\n"` (missing `EndState`) hung past the timeout. The same shape was independently confirmed for `Struct`/`EndStruct` and `Group`/`EndGroup` by the original finder. Notably, **pre-#2125-fix this did not hang**: the old bare-`?` shape propagated the EOF error immediately and returned `Err`, unwinding cleanly. The recovery loop introduced by the fix removed that early exit without replacing the termination condition it depended on.
- **Impact**: a straightforward denial-of-service on any code path that parses untrusted/imported `.psc` source (mod installation, community content import) — the parser hangs the calling thread indefinitely on a very ordinary corruption/truncation shape, not an exotic adversarial input.
- **Suggested Fix**: add `if self.at_eof() { push_error(ParseError::unexpected_eof(...)); break; }` as the first check inside each of the three loops' `_ =>` arm (or before dispatching into it), mirroring `parse_script`'s own `at_eof()` guard. A single shared helper (e.g. a `container_body_loop` combinator) would prevent this class of drift the next time a fourth container is added — see the sibling MEDIUM finding below, which is the same bug shape in a fourth container the #2125 fix didn't reach at all.

#### SCR-D5-NEW4-01: `QuestRef::Property` resolution ignores VMAD alias binding, unlike its `ObjectRef::Property` sibling

- **Severity**: HIGH
- **Dimension**: Recognizer-Chain Soundness / Decline-Invariant Audit (Dimension 5)
- **Untrusted-Input**: No — a real/malformed-VMAD-data correctness gap, not an adversarial-input path
- **Location**: `crates/plugin/src/esm/records/script_instance.rs:105-110` (`ScriptInstance::object_form_id`); consumed by `crates/scripting/src/fragment.rs:108-121` (`resolve_quest`) and `crates/scripting/src/translate/recognizers/quest_stage_gate.rs:76-81` (`recognize`)
- **Status**: NEW — independently found by an orphaned sub-agent from an earlier attempt at this audit and re-confirmed empirically in this pass
- **Description**: `ObjectRef::Property` resolution (`fragment.rs:143-151`, `resolve_property_form_id`) explicitly requires `PropertyValue::Object { form_id, alias: -1 } => Some(form_id), _ => None` — declining for any other `alias` value, per its own doc comment ("declines here rather than trusting the raw `form_id` sitting next to a live alias index"). But the sibling `QuestRef::Property` resolution path uses a different helper, `ScriptInstance::object_form_id`, which matches `PropertyValue::Object { form_id, .. } => Some(form_id)` — the `alias` field is discarded (`..`) and never checked. This helper is the **only** resolver for `QuestRef::Property` in both live call sites: `resolve_quest` (used by every `SetStage`/`SetObjectiveDisplayed`/`SetObjectiveCompleted`/`SetObjectiveFailed`/`CompleteAllObjectives` effect dispatch) and `quest_stage_gate::recognize` (used to bind the whole `QuestAdvanceOnActivate` component's `owning_quest`).
- **Evidence**: `docs/engine/m47-3-quest-alias-design.md:66` lists "`QuestRef::Property` / `ObjectRef::Property` alias decline" as one row, claimed done for both — but only the `ObjectRef` side is actually implemented. No test anywhere in the crate exercises `object_form_id`/`resolve_quest`/`quest_stage_gate::recognize` with a non-`-1` alias. `AUDIT_SCRIPTING_2026-07-21.md`'s "Decline-Invariant Audit" section explicitly re-verified `resolve_property_form_id`'s alias branch and found it correct-by-design, but did not examine `object_form_id`/`resolve_quest`, so this gap wasn't previously caught. Independently re-confirmed via a throwaway test (`script_instance.rs`, built, run, reverted — tree confirmed clean) constructing `PropertyValue::Object { form_id, alias: 3 }` and asserting `object_form_id` still returns `Some` — it does.
- **Impact**: if a `Quest`-typed VMAD property is ever alias-bound (the wire format doesn't distinguish by the property's declared Papyrus type, only by the generic type-1 "Object" tag every form-reference property shares), both the quest-stage-gate recognizer and the fragment effect dispatcher will silently resolve the raw `form_id` field sitting next to the alias index — not the intended target once a property is alias-bound — instead of declining. For the recognizer this means emitting a `QuestAdvanceOnActivate` component stamped with the wrong `owning_quest`; for the fragment dispatcher it means a `SetStage`/objective effect silently mutating the wrong quest's state. Real-world reachability is uncertain — Quest-typed properties aren't the typical target of CK's alias-fill UI (aliases usually fill Actor/ObjectReference-typed properties) — so actual corpus yield may be low, but the code has no structural guard against it, unlike its `ObjectRef` sibling.
- **Suggested Fix**: give `ScriptInstance` an `object_form_id`-equivalent that mirrors `resolve_property_form_id`'s explicit `alias: -1` match (or have both callers use that stricter matching directly), so `QuestRef::Property` on an alias-bound VMAD entry declines the same way `ObjectRef::Property` already does. Add a regression test with `alias: 3` (or any non-`-1` value) asserting both `fragment::apply_effects` and `quest_stage_gate::recognize` decline rather than resolve.

### MEDIUM

#### SCR-D4-NEW4-02: Bad setter still drops a valid getter from a full-form `Property` — the same bug shape #2125 fixed elsewhere, uncaught in this fourth container

- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Parser (Dimension 4)
- **Untrusted-Input**: Yes
- **Location**: `crates/papyrus/src/parser/script.rs:463-505` (`parse_property_accessors`)
- **Status**: NEW — sibling gap in the #2125 fix (same bug shape, different container, not covered by commit `cacc9935`); independently found by an orphaned sub-agent from an earlier attempt at this audit
- **Description**: Full-form `Property`'s getter/setter loop still uses bare `?` on `parse_function(...)` calls (lines 477, 495, 496) — exactly the pre-fix shape `parse_state`/`parse_struct`/`parse_group` had before #2125. An error in the setter propagates up through `parse_property` → `parse_type_prefixed_item` → `parse_script_item`, discarding the entire `ScriptItem::Property` — including a getter that parsed with zero errors.
- **Evidence**: a throwaway test with a valid `Int Function Get() ... EndFunction` followed by a malformed `Function Set(Int value)` body produced 3 recovered errors, but `script.body` was empty — the property (and its valid `Get()`) never appears in the AST at all.
- **Impact**: same class as the original #2125 impact, scoped to properties: any full-form property (getter+setter idiom) where the setter has a syntax error silently loses the getter too, with no diagnostic naming the property. This does not hang (bare `?` still exits immediately at EOF, unlike the HIGH finding above), so it's MEDIUM, not HIGH — the same severity #2125 was originally filed at.
- **Suggested Fix**: apply the identical per-child recovery pattern used in `parse_state`/`parse_struct`/`parse_group` to `parse_property_accessors`'s two `Some(Token::KwFunction)` / `_` arms — being careful to add the `at_eof()` guard from the HIGH finding above at the same time, rather than reintroducing that regression a second time.

#### SCR-D7-NEW4-01: Item-record family (`WEAP`/`ARMO`/`AMMO`/`MISC`/`KEYM`/`ALCH`/`INGR`/`BOOK`/`NOTE`) never gets its `VMAD` script decoded — `base_record_script_instance` can never resolve a script for any of them

- **Severity**: MEDIUM
- **Dimension**: Engine Attach Path & Trigger-Volume Wiring (Dimension 7), with the root cause in `crates/plugin`
- **Untrusted-Input**: No (a real-content-coverage gap, not a hostile-input path)
- **Location**: `crates/plugin/src/esm/records/common.rs:298-318` (`CommonItemFields` — only a presence-only `has_script: bool`, no `script_instance` field); contrast `crates/plugin/src/esm/records/common.rs:248-266` (`CommonNamedFields`, which fully decodes `script_instance: Option<ScriptInstanceData>`); `crates/plugin/src/esm/records/items.rs:143,272,394,472,488,504,540,555,589` (every `parse_weap`/`parse_armo`/`parse_ammo`/`parse_misc`/`parse_keym`/`parse_alch`/`parse_ingr`/`parse_book`/`parse_note` builds its `ItemRecord.common` via `CommonItemFields::from_subs`); `crates/plugin/src/esm/records/index.rs:599-616` (`base_record_script_instance` — walks `activators`/`containers`/`npcs`/`creatures` only, can't reach `items` even in principle since `ItemRecord` carries no `script_instance` field to return)
- **Status**: NEW
- **Description**: Two near-identical "common fields" structs exist for ESM record parsing. `CommonNamedFields` (used by `ACTI`, `CONT`, `SCOL`, `PKIN`, `TREE`, and — via `NpcRecord`, which is shared with `CREA` per #1273 — `NPC_`/`CREA`) fully decodes a `VMAD` sub-record into `script_instance: Option<ScriptInstanceData>` via `ScriptInstanceData::parse(&sub.data)`. `CommonItemFields` — used by every item-family record parser (`WEAP`, `ARMO`, `AMMO`, `MISC`, `KEYM`, `ALCH`, `INGR`, `BOOK`, `NOTE`) — only sets a presence-only `has_script: bool` flag and has **no `script_instance` field at all**. `base_record_script_instance` (the M47.2 attach path's VMAD accessor, `index.rs:599`) walks `self.activators`/`self.containers`/`self.npcs`/`self.creatures` and returns each one's `script_instance` — it structurally cannot reach `self.items`, because `ItemRecord.common: CommonItemFields` has nowhere to store a decoded VMAD even if the lookup arm were added. The doc comment on `CommonItemFields::has_script` (`common.rs:312-316`) blames this on work "gated on the scripting-as-ECS work tracked at M30.2/M48" — but that work has demonstrably shipped (this whole audit domain is M47.2's live decompile+recognizer chain), just never extended to this one struct. The referenced tracking issue, #369 ("VMAD sub-records skipped on every Skyrim record"), is CLOSED — its fix evidently covered `CommonNamedFields`'s consumers but not `CommonItemFields`'s.
- **Evidence**: `common.rs:284-290` (`CommonNamedFields::from_subs`'s `VMAD` arm sets both `has_script = true` AND `script_instance = Some(ScriptInstanceData::parse(&sub.data))`) vs. `common.rs:338-339` (`CommonItemFields::from_subs`'s `VMAD` arm: `b"VMAD" => out.has_script = true,` — one line, no parse call, no field to hold the result even if it wanted to). `items.rs`'s 9 `parse_*` functions all build from `CommonItemFields`. `index.rs:603-615`'s `base_record_script_instance` match arms: `activators`, `containers`, `npcs`, `creatures` — no `items` arm exists, and none could return anything useful even if added under the current `ItemRecord` shape.
- **Impact**: Any Skyrim+/FO4+ weapon, armor piece, potion/ingestible, book, key, ammo, or ingredient record that carries a `VMAD`-attached Papyrus script (e.g. an `OnEquip`/`OnUnequip` hook granting a temporary effect, a quest book that fires a stage-advance on read, a scripted key) silently never attaches its script — `attach_vmad_scripts` calls `index.base_record_script_instance(base_form_id)`, which returns `None` for every item-family base record regardless of what its `VMAD` actually contains. This is a silent content-coverage gap (a decline, not a wrong lowering — no game state gets corrupted), but it is real and previously unflagged across all seven prior audit passes of this domain. No corpus scan was run this pass to quantify how many real Skyrim/FO4 item records actually carry a non-trivial VMAD (the existing `qust_alias_survey`/`pex_corpus_shapes` methodology could be adapted for this); severity is set at MEDIUM (a documented content-family gap, not proven HIGH-frequency) pending that measurement.
- **Related**: Superficially related to #369 (closed) — this is effectively an unclosed sibling of that fix, in the one record family (`CommonItemFields`) its closure didn't reach.
- **Suggested Fix**: Add a `script_instance: Option<ScriptInstanceData>` field to `CommonItemFields`, populate it the same way `CommonNamedFields` does (`ScriptInstanceData::parse(&sub.data)` in the `VMAD` arm), thread it into `ItemRecord`, and add an `items` arm to `base_record_script_instance` (returning `r.common.script_instance.as_ref()`, mirroring the existing arms). Update the stale doc comment on `has_script` once the decode lands. Before prioritizing, consider running a quick corpus census (real `Skyrim.esm`/`Fallout4.esm`) of how many `WEAP`/`ARMO`/`ALCH`/`BOOK`/etc. records actually carry a non-empty `VMAD`, to convert the MEDIUM estimate into a measured severity.

### LOW

#### SCR-D7-NEW4-02: This skill's Dimension 7 entry-points list three functions under a module path that no longer contains them

- **Severity**: LOW (doc-rot in the skill file itself, per `_audit-common.md`'s Path-Reference Convention)
- **Dimension**: Engine Attach Path & Trigger-Volume Wiring
- **Untrusted-Input**: No
- **Location**: `.claude/commands/audit-scripting/SKILL.md` (Dimension 7 "Entry points" line, citing `byroredux/src/cell_loader/references/mod.rs` for `attach_vmad_scripts`, `attach_script_for_refr`, `trigger_volume_from_primitive`)
- **Status**: NEW
- **Description**: These three functions were split out of `references/mod.rs` into a new sibling file `byroredux/src/cell_loader/references/attach.rs` (its own header states "Split out of the original `cell_loader/references.rs` (#1877)"). `mod.rs` now only re-exports them (`use attach::{attach_container_inventory, attach_script_for_refr, trigger_volume_from_primitive};`) and retains their test modules. The skill's entry-point line still names only `mod.rs`.
- **Impact**: Cosmetic/navigational only — a future audit pass or contributor following the skill's entry-point list would look in the wrong file first. No functional impact; the functions themselves are correct and unchanged in behavior (verified directly this pass).
- **Suggested Fix**: Update the Dimension 7 "Entry points" line in `SKILL.md` to cite `byroredux/src/cell_loader/references/attach.rs` for these three functions, keeping `references/mod.rs` for the call sites / dispatch context.

#### SCR-D6-NEW4-01: `m47-2-design.md`'s own Phase-0 "done" checklist still has "retire hardcoded `papyrus_demo::register_spawners`" unchecked — and it is, in fact, still live in `boot.rs`

- **Severity**: LOW (tech debt; verified inert against all real game data, not a live correctness bug)
- **Dimension**: Scripting Runtime Systems / Engine Attach Path
- **Untrusted-Input**: No
- **Location**: `byroredux/src/boot.rs:475-489` (the call site); `docs/engine/m47-2-design.md:212-237` ("Engine integration" section, stating "The hardcoded demo registration is retired in favor of this path") and `:333` (`- [ ] hardcoded `papyrus_demo::register_spawners` retired from the attach path`, still unchecked)
- **Status**: NEW
- **Description**: `boot.rs` still builds a `ScriptRegistry`, calls `byroredux_scripting::papyrus_demo::register_spawners(&mut script_registry)` (which registers exactly one entry, `"defaultRumbleOnActivate" → spawn_default_rumble`), and inserts it as a live world resource that `attach_scpt_script` (the pre-Skyrim `SCRI`→`SCPT`→`ScriptRegistry` Obscript path) consults on every cell load for every REFR. `m47-2-design.md`'s prose says this was supposed to be retired once the dynamic VMAD/`.pex` recognizer path (M47.2, Phase 0) landed — its own "Verification checklist for 'M47.2 done'" leaves that exact line unchecked, consistent with `boot.rs` never having been updated to drop the call.
- **Evidence**: `boot.rs:483-484` (`let mut script_registry = ...; byroredux_scripting::papyrus_demo::register_spawners(&mut script_registry);`); `crates/scripting/src/papyrus_demo/mod.rs:230-238` (`register_spawners` registers only `"defaultRumbleOnActivate"`); `crates/plugin/src/esm/records/index.rs:552-585` (`base_record_script`, the SCRI/SCPT path this registry feeds, only ever returns a form ID sourced from an `SCRI` sub-record — a field Skyrim+ records don't carry, since Skyrim+ scripting is exclusively `VMAD`-based).
- **Impact**: Traced this through rather than assumed: because `attach_scpt_script`'s `ScriptRegistry` lookup is only ever reached via an `SCRI`-sourced `script_form_id` (pre-Skyrim: Oblivion/FO3/FNV), and `"defaultRumbleOnActivate"` is a Skyrim-era script name that would never appear as an `SCPT` record's `editor_id` cross-referenced by an `SCRI` sub-record in real pre-Skyrim content, this registration is provably inert against every real game's data today — not a double-attach or correctness risk, purely dead wiring that the design doc's own exit criteria say should have been removed. No live bug; a stale TODO that outlived the milestone it was scoped to.
- **Suggested Fix**: Drop the `register_spawners` call (and, if nothing else populates it, the `ScriptRegistry` resource itself) from `boot.rs` once confirmed no other in-tree code path still depends on it, per the design doc's own Phase-0 checklist; or, if the demo registration is being deliberately kept as a smoke-test convenience, check the box in `m47-2-design.md` and add a comment at the `boot.rs` call site explaining why it's being kept past its original retirement plan.

## Confirmed-fixed prior-audit findings (re-verified in place, no regression)

**From the 2026-07-21 report, all eight independently re-verified fixed in current code (not just closed on GitHub)**:

- **#2122**/SCR-D2-NEW3-01 (CFG stale-block-key on backward interior `jmpf`/`jmpt`) — fix in place at `cfg.rs:244-245` (`current_key` re-resolved via `find_block_for_instruction` after both splits); both `backward_jmpf_target_inside_own_block_conditions_the_right_block` and `backward_jmpt_target_inside_own_block_conditions_the_right_block` tests present.
- **#2123**/SCR-D6-NEW3-01 (`RunOn::Reference` always `None`) — fixed: `condition.rs:263-265` now calls `resolve_entity_by_global_form_id(world, condition.reference_form_id)`; `run_on_reference_resolves_the_entity_carrying_the_form_id` test present and passing.
- **#2124**/SCR-D6-NEW3-02 (cascade genuine-transition guard compared wrong variable) — fixed: `fragment.rs:540` now reads `if adv.previous_stage != adv.new_stage`.
- **#2125**/SCR-D4-NEW3-01 (parser container-level error discards whole `State`/`Group`/`Struct`) — fixed in all three: `parse_state`, `parse_struct`, `parse_group` (`crates/papyrus/src/parser/script.rs`) each now catch per-child/per-member/per-property errors and `skip_to_next_line`, with `#2125` comments marking each site.
- **#2126**/SCR-D6-NEW3-03 (nested-lock safety undocumented) — fixed: a doc comment directly on `apply_effect` (`fragment.rs:180-192`) now states the exclusive-scheduling dependency explicitly, referencing `#2126`.
- **#2127**/SCR-D1-NEW2-01 (opcode metadata test covered only 7/51 rows) — fixed: `metadata_matches_champollion_full_table` (`opcode.rs:201`) iterates all 51 discriminants against an independently-transcribed literal table.
- **#2128**/SCR-D6-NEW3-04 (`quest_stages.rs` header stale re: fragment dispatch) — fixed: the module doc now states fragment dispatch "have both shipped — see `crate::fragment`."
- **#2129**/SCR-D6-NEW3-05 (`fragment.rs` doc claims `MoveTo` still declines) — fixed: no such claim remains in the current doc comment at that location.

**From all prior reports, re-spot-checked this pass, no drift**: the reader's three-dialect round-trip tests, the recursion caps in both `control_flow.rs` and `boolean.rs`, the `.psc`-vs-`.pex` DA10 fidelity gate on both sides, the 12-type marker-drain list, the CTDA OR-precedence block-scan logic, `QuestStageState`'s stage-history retention semantics, and the trigger volume's edge-triggered `Option<bool>` seeding.

## Existing / correctly-tracked (NOT re-filed — dedup)

- **#2130** (SCR-D7-NEW3-01) — `quest_advance_system`'s "one signal per entity per frame" assumption is unenforced. Still OPEN, description re-verified accurate against current `quest_advance.rs` — informational, no fix required until the future "player activates a REFR" system (Stage 4) lands. Not re-filed.

No other open scripting-domain issues exist in the current 29-issue open list (`gh issue list`, verified this pass) — the domain's open-issue count has actually dropped since 07-21 as a direct result of the same-session fix commit (`6b986478`) that closed #2122–#2129.

## Future-Phase Readiness

- **Item-record VMAD gap (SCR-D7-NEW4-01)**: worth a corpus census before scheduling — the fix itself (mirroring `CommonNamedFields`'s existing decode into `CommonItemFields`) is cheap and mechanical; the open question is how much real vanilla content it actually unlocks. Recommend the same corpus-survey methodology already used for `qust_alias_survey.rs` (VMAD presence count across `Skyrim.esm`/`Fallout4.esm`'s `WEAP`/`ARMO`/`ALCH`/`BOOK`/etc. records) before triaging severity further.
- **Hardcoded `ScriptRegistry` demo registration (SCR-D6-NEW4-01)**: cheap to close (delete a call site + check a box in the design doc) — flagged mainly so it doesn't sit unresolved through an eighth, ninth, tenth audit pass citing the same unchecked line.
- **SKILL entry-point drift (SCR-D7-NEW4-02)**: a one-line doc fix; worth doing opportunistically the next time this skill file is touched for any other reason, per `_audit-validate.sh`'s Path-Reference Convention.
- **Alias-bound `resolve_property_form_id` test-coverage gap**: unchanged guidance from the 07-21 pass — the decline is correct-by-design and unpinned by a direct `alias != -1` fixture; still low-priority since the behavior is architecturally forced by a one-line match arm, not a heuristic that could silently drift.
- **Condition resolvers, live-cell re-verification**: unchanged guidance from all prior passes — unit-test-clean, still not re-verified against a live headless cell with real CTDA data.
- **M47.3 quest-alias-fill runtime**: unchanged — the `Property`-resolution decline on an alias-bound VMAD entry remains correct-by-design, out of this skill's crate scope.
- **Obscript/SCTX frontend (Phase 5)**: unchanged, not built, correctly out of scope.

---
*Single-session direct audit (no sub-agent delegation), run 2026-07-25.
Dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux` (29 open issues)
+ `docs/audits/AUDIT_SCRIPTING_2026-07-21.md` + direct `gh issue view` /
`--state closed --search` confirmation that #2122–#2129 are closed and fixed
in place, and #2130 remains open and accurate.*
