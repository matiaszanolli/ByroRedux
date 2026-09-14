# Scripting Subsystem Audit — 2026-09-14

Nineteenth full pass over the M30/M47 Papyrus / `.pex` / ECS scripting domain.
It was run as the skill prescribes: an orchestrator plus eight dimension agents, at most three at a
time. Each agent wrote `/tmp/audit/scripting/dim_N.md` before this consolidation. After the agents
finished, the orchestrator re-checked the top finding of every dimension against the source.

**Scope**: `crates/pex`, `crates/papyrus` and `crates/scripting` (the owner crates), `crates/hkx`
(Dim 8), and the engine-side attach / spawn / combat-AI / cinematic wiring (Dims 6–8).
**Still a reported coverage gap, not audited** (unchanged since 09-06): the SKSE / JContainers /
StorageUtil / ObScript compatibility layer (`crates/scripting/src/{papyrus_provider/, obscript.rs,
obscript_runtime.rs, compatibility.rs}`) and `crates/sdk`. Only the seams into that layer were
re-checked, including b931492b (#4139); see Dim 5.

**HEAD**: `5278e163` at both start and finish; no domain commits landed during the pass. Every
dimension diffed its file set with `git log b3db49fa..HEAD`, where `b3db49fa` is the 09-11 baseline.

**Dedup baseline**: `gh issue list --limit 300`, the 2026-09-11 report, and per-finding
`gh issue list --state all --search` queries. No new finding matched an existing issue.

## What changed since 2026-09-11

`crates/pex` and `crates/papyrus` had **zero** commits, and neither did `trigger.rs`, `quest_stages.rs`,
`cleanup.rs` or `systems/cinematic.rs`. For those areas Dims 1–4 re-verified the code and hunted for
anything prior passes missed. All new code is in Dims 5–8:

| Commit | Effect on this domain |
|---|---|
| `fb8173fe` "Skyrim Havok packfiles + Skyrim LE" | `crates/hkx` +455 LOC: 32-bit (LE) packfile layout. **New untrusted-input decode** (Dim 8) |
| `d0dac91b` MQ101 chargen gate effects | New `SetInChargen` / `ShowRaceMenu` / `RequestSave` / `RequestAutoSave` primitives + `CinematicPresentationState` fields (save v23) |
| `f61ea044` NPC combat AI | New `SetEnemy` / `StartCombat` primitives, `crates/scripting/src/combat.rs` (`FactionRelations`, `AiCombatState`), `byroredux/src/systems/combat_ai.rs` (`npc_combat_ai_system`) |
| `11bed511` Fix #4136 | `ReferenceLockState` FormID-keyed lock ledger + spawn-side gate + save registration |
| `b931492b` Fix #4139 | Provider continuations bound to session-local entity handles dropped after load (SDK seam) |
| `f680df2e` Fix #4167 | Completeness guards on the HKX clip conversion |

Layout note: `fragment.rs` is now a thin head over
`crates/scripting/src/fragment/{effects,populate,state,systems,tests}.rs`. The skill still names
`fragment.rs::X`; resolve those names into the submodules.

## Build & test state — CLEAN

```
$ cargo test -q -p byroredux-pex -p byroredux-papyrus -p byroredux-scripting -p byroredux-hkx --all-targets
   657 passed; 0 failed; 7 ignored   (scripting lib 464, up from 440 at 09-11)

$ cargo test -q -p byroredux-scripting recognizes_da10_and_reproduces_hand_builder
   1 passed
$ cargo test -q -p byroredux-scripting --test pex_recognize_e2e -- --ignored da10_pex_reproduces_hand_builder_byte_for_byte
   1 passed   (real Skyrim SE `Skyrim - Misc.bsa`)
$ cargo test -p byroredux --bin byroredux -- combat_ai scripted_lock reference_lock
   12 passed   (Dim 6)
$ skyrim_le_packfiles_decode_to_the_same_rig_and_clips_as_se -- --ignored
   1 passed    (Dim 8)
```

## Headline verdicts

### Untrusted-input robustness — **NOT MET: a ~330 KB wire-valid `.pex` aborts the process**

This regresses the 09-11 "MET" verdict. The code did not change; a class of input that prior passes
never measured was probed for the first time. **SCR-D1-2026-09-14-01**: every `u16` string-table
reference is cloned into an owned `String`. Each 2-byte reference to a 65,535-byte table string costs
a full copy, so memory is amplified up to ~12,400× (measured):

- 105 KB → 1.25 GB after `parse`;
- a ~327 KB file aborts with `memory allocation of 65535 bytes failed` under an 8 GB cap;
- `call_sites` adds its own copies: 366 KB → 3.76 GB.

An allocation failure aborts the process. `catch_unwind` cannot catch it, so the #3948 panic net does
not help. Reachability: `--scripts-bsa` explicitly takes mod archives. Champollion stores indices, so
this is a port divergence.

Everything else holds under fuzzing:

- Dim 1: 1.8 M byte-flip mutations plus 22,279 truncations → 0 panics, 0 aborts.
- Dim 8: 3,844,806 mutated `.hkx` decodes with overflow-checks on → 0 panics.

The `.psc` parser has a separate stack-overflow-on-Drop abort (**SCR-D4-2026-09-14-01**). No runtime
game-data path parses `.psc`, so that finding is scoped to tooling and the localhost debug console.

### The 99.996% decompile-rate claim — **re-measured, still exact; measures robustness, not fidelity**

Dim 3 re-ran `pex_corpus_smoke` at HEAD:

| Archive | Decompiled |
|---|---|
| Skyrim SE | 14,026 / 14,026 |
| FO4 | 7,874 / 7,875 |
| Starfield | 4,740 / 4,740 |
| **Total** | **26,640 / 26,641** |

0 panics and 0 shape mismatches. The harness counts `Err` and panics as failures. The one failure
(FO4 `stimboxscript.pex`) is a legitimate `||` whose rejoin block is also a jump target; it fails
closed and is not caused by the missing debug-line guard.

**Caveat, now demonstrated**: the #3017 shape check computes its expectation with the same
`is_auto_state` rule `decompile_script` uses. It therefore cannot see **SCR-D3-2026-09-14-01**, which
mis-assembles 983 vanilla scripts: both sides agree on the wrong shape.

### The `.psc`-vs-`.pex` fidelity gate — **both halves pass, re-run this pass**

`recognizes_da10_and_reproduces_hand_builder` (`.psc`) and
`da10_pex_reproduces_hand_builder_byte_for_byte` (`.pex`, real Skyrim SE archive) both still assert
byte-equality against `da10_main_door(...)`. DA10 has no named auto state, so the gate does not
exercise SCR-D3-2026-09-14-01.

### The decline-on-unmodeled invariant — **violated by one new primitive**

Every standing decline point holds (Dim 5 table below). The breach is in new code from `f61ea044`:
`prim_set_enemy` accepts the two bool arguments of `Faction.SetEnemy` and dispatch then discards them
(**SCR-D5-2026-09-14-01**, HIGH). The real signature, read from Skyrim SE's own `faction.pex` and
confirmed independently by the orchestrator, is
`SetEnemy(Faction akOther, Bool abSelfIsNeutralToOther, Bool abOtherIsNeutralToSelf)`. So
`SetEnemy(x, true, true)` means "mutually neutral", yet it is recorded as mutual hostility.

## Coverage gap — SDK / extender-compatibility layer (reported, NOT audited, unchanged)

The layer is the same ~14k+ LOC (`crates/sdk`, `papyrus_provider/`, `obscript.rs`,
`obscript_runtime.rs`, `compatibility.rs`). This pass re-checked one seam: `b931492b` (#4139) only
drops post-barrier continuations after a load. It never touches lowering, and the canonical-first
ordering is intact. Given SCR-D1-2026-09-14-01, the untrusted `SCDA` decode inside `obscript.rs`
deserves the same *memory amplification* lens; it has never had one.

## Decompiler Soundness Matrix (Dims 1–4)

| Pass | Bounds-safe | Terminates | Total (no panic/abort) | Memory ≈ input | Fidelity-tested |
|------|:---:|:---:|:---:|:---:|:---:|
| Reader (`reader.rs` + `opcode.rs`) | Yes | Yes | Yes | **No — up to ~12,400× (SCR-D1-2026-09-14-01)** | Yes (51-row opcode table vs Champollion `Instruction.cpp`, exhaustive truncation) |
| `call_sites.rs` preflight | Yes | Yes (O(D) build, O(1) lookup; #3938 holds) | Yes | **No — adds its own clone amplification (same root cause)** | Yes |
| CFG (`cfg.rs`) | Yes | Yes | Yes (`split(at ≥ 1)`; the #2122 re-resolve holds) | Yes | Yes; corpus: 0 CFG errors / 99,918 fns |
| Lift + copy-prop (`lift.rs`) | Yes | Yes (#2024 linear chain) | Yes (#2666 count/replace fails closed in release; child traversal matches arm by arm) | Yes | Yes; corpus: 0 lift errors despite 109,578 temp re-writes |
| Boolean (`boolean.rs`) | Yes | Yes (each reprocess removes 2 blocks; cap 1024) | Yes (#3933 depth check in `combine`) | Yes | Yes |
| Control-flow (`control_flow.rs`) | Yes | Yes (cap 1024) | Yes (`\|\|` arm fails closed, #1732; doc still stale, #4115) | Yes | Yes |
| Lower (`lower.rs`) | Yes | Yes | Yes (`lower_expr` exhaustive; `lower_binary_op` default arm has no producer — all 13 emitted op strings enumerated) | Yes | **Expression level yes; script assembly NO — named auto states inverted (SCR-D3-2026-09-14-01)** |
| `.psc` lexer + parser | Yes | Yes (linear time/RSS on 19 adversarial shapes) | **No — iterative postfix/infix chains bypass `MAX_EXPR_DEPTH`; the AST aborts on recursive Drop at ~170k–200k terms (SCR-D4-2026-09-14-01)** | Yes (worst ~160× RSS/byte, linear) | **Partial — a line starting with `(` is glued onto the previous statement (SCR-D4-2026-09-14-02)** |

### The two documented Champollion departures

1. **No debug-line guard in `boolean.rs`: benign, now measured rather than argued.** Dim 3 scanned
   all 99,918 vanilla functions. Of 10,733 collapses that actually fire, **all are `Bool`-typed and
   none is non-Bool**. For `Bool` the merged form is semantically identical, including short-circuit
   order. Every non-Bool candidate fails the single-recompute gate.
2. **`||`-skip in `control_flow.rs`: correct and fail-closed** (#1732). Only the module doc is stale
   (#4115, still open).

Dim 2 also measured the copy-prop cross-boundary hazard. Of 3,516 cross-block temp reads, 3,405 are
boolean rejoin labels the boolean pass merges first, 109 read never-written temps, and 2 are Starfield
outliers (`uc02_terrormorphscript.pex:OnDistanceLessThan@26`, `robotquestrunner.pex:MakeQuestNameSave@5`)
that lift leaves unfolded. An end-to-end detector found 0 folds moved into an If/While body. Nothing
reportable.

## Decline-Invariant Audit (Dim 5)

| Decline point | Verdict |
|---|---|
| Chain order `two_state_activator → rumble → quest_stage_gate`, `find_map` | Conservative, unchanged |
| `classify_effect_with_providers` canonical-first (#3934) | Holds |
| `split_and` keeps `\|\|` whole; per-atom `classify_guard_atom(..)?` | Conservative, unchanged |
| `lower_statements` three-map `bind_local`, `Return(None)` inert, `_ => None` | Conservative, unchanged |
| Narrow `While` (`lower_3d_loaded_wait`) / `If` (`Effect::Conditional`) exceptions; `MAX_CONDITIONAL_DEPTH` derived | Holds |
| Hole binding (`QuestRef` / `ObjectRef`, alias-bound `SceneActorBindings::resolve`) | Conservative, unchanged |
| `translate_pex*` panic net (#1816 / #3948) | Holds for panics. **Cannot catch the OOM abort in SCR-D1-2026-09-14-01** |
| #3496 mechanized arity scan | Holds and covers all 6 new primitives; none exempted |
| **NEW `prim_set_in_chargen`** | Arity exact, non-literal declines. **Field names invented** → SCR-D5-2026-09-14-02 |
| **NEW `prim_show_race_menu` / `prim_request_save` / `prim_request_auto_save`** | Arity exact. **Lowered to counters nothing reads, counted as claimed coverage** → SCR-D5-2026-09-14-04 |
| **NEW `prim_set_enemy`** | **Bool args lowered then discarded; `true` flags invert the meaning** → **SCR-D5-2026-09-14-01 (HIGH)** |
| **NEW `prim_start_combat`** | **Accepts `ActorRef::Player` as the combatant; the AI then drives the player's Transform** → SCR-D5-2026-09-14-03 |
| translate↔dispatch pairing (5 new variants) | All have real `apply_effect` arms; none `unreachable!` |
| `fragment_coverage` / `mq101_conformance` harnesses | `effect_kind` exhaustive, no wildcard; declines tallied honestly (stubs aside, -04) |

Cross-dimension note from Dim 3 (not filed): `quest_stage_gate::find_advance_event` takes the first
`OnActivate`/`OnTriggerEnter` in *any* state. In the 85 scripts where an auto state overrides a
default handler, that is the default-state handler, not the one the runtime boots into. This behaves
the same for both frontends. It becomes relevant once SCR-D3-2026-09-14-01 is fixed and a
state-aware consumer lands.

## Runtime Lifecycle Invariant Matrix (Dim 6)

| Invariant | Verdict |
|---|---|
| Two-phase lock drop (timer / trigger / recurring_update) | Holds |
| Clone-before-lock (`QuestStageFragments` / `SceneFragments`) | Holds |
| Cascade FIFO; `MAX_CASCADE` counts only `is_cascade` | Holds |
| `push_quest_stage_advances` sole sink writer (#3277) | Holds |
| Marker drain coverage (Pattern A / B contract test) | Holds for scripting markers. **`HitEvent` now has two producers that overwrite each other** → SCR-D6-2026-09-14-01 |
| CTDA OR-precedence; empty ⇒ true; `RunOn` declines | Holds |
| Trigger edge / seed / multi-triggerer / occupancy prune | Holds |
| `QuestAliasReadinessGate` ordering | Holds |
| `apply_effect` nested-lock doc inventory pin (#3951) | Holds; `AiCombatState` write listed; chargen effects deferred-only |
| `Access` source-scan (#3951 / #4064) | Holds; `npc_combat_ai_system` is exclusive, and its declaration was checked by hand and found complete |
| #4136 `ReferenceLockState` ledger: FormID key space matches reader; applied before disabled gate and attach; save-registered + round-trip test; pre-reload restore | Holds. Two LOW gaps: SCR-D6-2026-09-14-03 / -04 |
| Ledger vs load-order change | Holds (reload re-runs with the snapshot's masters) |
| Combat AI schedule (`combat_input` → `npc_combat_ai` → `combat_damage`); Dead / stale targets | Holds (`EntityId` never reused) |
| Combat AI `PhysicsWorld` hold discipline | **Violated** → SCR-D6-2026-09-14-02 |
| #4116 `ActivateEvent` consumer-order test omits `mg07_on_activate_dispatch` | Still open (order itself still correct) |

## Engine Attach & Spawn Gates (Dim 7)

| Spawn path | Identity stamp | Script attach | `ReferenceEnableState` consulted |
|---|---|---|---|
| Static-mesh synth child (`spawn_placed_instances`) | primary only | yes | **yes — the only production call site (`spawn.rs` `placement_is_disabled`)** |
| Invisible trigger volume (`synth_child.rs` trigger branch) | primary only | yes | **no** → SCR-D7-2026-09-14-01 |
| Actor job (`references/mod.rs`) | `synth_idx == 0` | yes | **no** → SCR-D7-2026-09-14-02 |
| LIGH-only / fxlight branches | primary only | yes | **no** → SCR-D7-2026-09-14-02 |
| Logical fallbacks (stat / NIF miss, marker) | primary only | yes | no (identity-only, harmless) |
| FO4 precombined | none (no REFR identity) | none | n/a |
| Persistent-worldspace logical stubs (`exterior.rs`) | yes | **no** | no — #4112 (still open) |
| MQ101 scene-actor alias stubs (`materialize_scene_actor_alias_stubs`) | yes | **no** | no → SCR-D7-2026-09-14-03 |

Still holds: silent-miss degradation; `base_record_script_instance`'s seven arms plus per-REFR VMAD
merge; `.pex` path normalization with first-listed-archive-wins; XPRM → `TriggerVolume`
(no `/2`, `[b0,b2,b1].abs()*scale`); `quest_advance_system` per-(entity, triggerer) gating;
`M47.2 scripts:` counters (blind to the two stub paths); `scene.show` read-only; #4136 lock gate
composition.

## Havok / Cinematic Slice (Dim 8)

| Invariant | Verdict |
|---|---|
| Zero `unsafe`; no behavior-graph execution | Holds after `fb8173fe` |
| Pointer width driven by the file header (`layoutRules[0]`) | Holds |
| Other layout gates (fileVersion, contentsVersion, reusePaddingOptimization) | **Not read** → SCR-D8-2026-09-14-01 |
| Every offset / count bounds-checked; no lying-count pre-alloc; no recursion | Holds; 3,844,806 mutated decodes, 0 panics, worst 1.04 s (#3011 cap) |
| `Layout` field order | Sourced against the havok-2013 headers; 32-bit offsets derived (2010 SDK not in-repo) and confirmed by the passing LE==SE real-data test |
| Real data | All sampled LE / SE: fileVersion 8, `hk_2010.2.0-r1` |
| Spline / quantization decode; track-count mismatch rejected | Unchanged by `fb8173fe` |
| Z-up→Y-up exactly once; #4167 guard non-vacuous | Holds |
| Once-per-serial playback; root-motion apply-then-drain; completion events; retention scoping | Holds |
| Router ↔ gate share one eligibility predicate (#3954) | **Only on paper** — the router re-implements `base_form_advance_is_eligible` inline → SCR-D8-2026-09-14-02 |
| #3817 tether/cinematic state lifetime; #4190 ScenePlayer deep clone | Still open, cited |

## Findings

**2 HIGH, 9 MEDIUM, 6 LOW** (17 new). Six earlier findings are still open and re-verified: #4112,
#4113, #4115, #4116, #4190, #3817.

---

### HIGH

#### SCR-D1-2026-09-14-01: every `.pex` string-table reference is cloned into an owned `String`, so a ~330 KB wire-valid file exhausts 8 GB and aborts the process uncatchably

- **Severity**: HIGH (domain table: unbounded alloc reachable from untrusted `.pex`; same class as #3783)
- **Dimension**: PEX Reader & Opcode Decode
- **Untrusted-Input**: Yes
- **Location**: `crates/pex/src/reader.rs` `Reader::string_index` (`self.strings.get(idx).cloned()`), called by every table reader (`read_user_flags`, `read_typed_names`, `read_debug_info`, `read_properties`, `read_function`, `value()` tags 1/2); `crates/pex/src/model.rs` (owned `String` fields); `crates/pex/src/call_sites.rs` `scan_function` (per-call `CallSite` clones of `source_file_name` / `object.name` / scope)
- **Status**: NEW. Searched "string_index", "pex string clone", "pex OOM", "pex allocation amplification". Nearest are #3783 (stack overflow), #1710 (var-arg pre-alloc) and #55 (NIF string table); none cover this. The 08-20 and 09-06 reports checked only `with_capacity` counts.
- **Description**: The string table allows one string of up to 65,535 bytes. A `u16` index costs 2 bytes on disk but allocates a fresh full copy per reference. Containers repeat (objects × states × functions × params × instructions), so total allocation is bounded only by file size, at up to ~32,000× per reference. Champollion's `getStringIndex` (`FileReader.cpp:504-518`) returns an index handle and never copies. Rust aborts on allocation failure instead of panicking, so `translate::catching_panics` (the #3948 net around parse → preflight → `call_sites` → decompile) cannot recover, and the engine dies during cell load. Reachable via `ScriptProvider::resolve_pex`, whose doc tells users to list mod archives in `--scripts-bsa`.
- **Evidence** (orchestrator confirmed the clone at `reader.rs:139-147` and the absence of any size budget in `reader.rs`/`lib.rs`; measurements from Dim 1's probe `d1fuzz/src/bin/amp.rs`, wire-valid FO4 LE `.pex` with one 65,535-byte string):

  | Shape | File | Peak RSS | Ratio |
  |---|---|---|---|
  | 10,000 user flags | 95.6 KB | 628 MB after `parse` | 6,861× |
  | 10,000 params (2 refs each) | 105.6 KB | 1,253 MB after `parse` | 12,418× |
  | 65,535 params, one function | ≈327 KB | — | **aborts: `memory allocation of 65535 bytes failed`** (`ulimit -v 8000000`) |
  | 20,000 `callstatic` + `call_sites()` | 365.6 KB | 1,257 MB after `parse`, **3,762 MB after `call_sites`** | 3,597× (parse) |

- **Impact**: One malicious or corrupt mod `.pex` of a few hundred KB crashes the engine at cell load, with no fallback. It is the memory-side twin of the CPU DoS fixed in #3938, which only stalled for 11.6 s; here the process dies. Vanilla content is unaffected (`actor.pex` is 13 KB).
- **Related**: #3938, #3783, #1710, #3948
- **Suggested Fix**: Mirror Champollion. Intern the table as `Arc<str>` in `read_string_table` and have `string_index` return `Arc::clone`, or store `u16` indices; apply the same to `CallSite`. Stopgap: a pre-parse budget (e.g. Σ referenced-string bytes ≤ 64 × `bytes.len()`) behind a new `PexError` variant. Regression test: the 65,535-params shape parses in bounded memory or returns `Err`, and never aborts.

#### SCR-D5-2026-09-14-01: `Faction.SetEnemy` lowers `abSelfIsNeutralToOther` / `abOtherIsNeutralToSelf` and then discards them — `true` flags are recorded as mutual hostility

- **Severity**: HIGH (domain table: recognizer emits a component on an unmodeled term instead of declining; today's sink has no reader, see Impact)
- **Dimension**: Recognizer-Chain Soundness
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/effects.rs` `Effect::SetEnemy` variant (doc `abModifyPlayer, abModifyEnemy`) and `prim_set_enemy`; `crates/scripting/src/fragment/effects.rs` `Effect::SetEnemy { faction, other_faction, .. }` dispatch arm; `crates/scripting/src/combat.rs` `FactionRelations::set_enemy` doc
- **Status**: NEW (no issue mentions `SetEnemy`)
- **Description**: The real signature is `SetEnemy(Faction akOther, Bool abSelfIsNeutralToOther, Bool abOtherIsNeutralToSelf)`. A `true` flag makes that direction *neutral* instead of enemy. `prim_set_enemy` captures the flags under invented names (`modify_player` / `modify_enemy`, dead-code fallback `unwrap_or(true)` behind an exact arity check). The dispatch arm matches with `..` and pushes an undirected hostile pair regardless. This is the #3487 `MoveTo` lesson in reverse: an argument whose non-default value changes the call's meaning is accepted and ignored, where the rule is to decline.
- **Evidence**: Dim 5 extracted `scripts\faction.pex` from Skyrim SE `Skyrim - Misc.bsa` and parsed it with `byroredux_pex::parse`. The orchestrator re-ran the probe and got `Faction.SetEnemy(Faction akOther, Bool abSelfIsNeutralToOther, Bool abOtherIsNeutralToSelf) flags=0x2 [native]`. A raw string-table scan finds the neutral names and no `abModifyPlayer` / `abModifyEnemy`. The only pinning test, `lowers_mq101_set_enemy`, uses `false, false`; no test covers `true` or non-literal flags.
- **Impact**: `F.SetEnemy(G, true, true)` means "make F and G mutually neutral" but yields `FactionRelations::is_enemy(F, G) == true`, which is the wrong-predicate corruption the invariant forbids. Blast radius today: `FactionRelations` has no production reader. The orchestrator confirmed this: the `crates/sdk` / `mod-runtime` hits are the different `FactionRelationship` type. The corruption goes live as soon as ambient hostility reads it. MQ101's own calls pass `false, false` and are correct. Frequency of non-false flags in vanilla or mods was not measured.
- **Related**: #3487; SCR-D5-2026-09-14-02 (same invented-signature class)
- **Suggested Fix**: Accept only the literal default shape (both flags `false`) and decline otherwise, or model the neutral direction explicitly. Rename the fields to the Papyrus names, fix the `combat.rs` doc, and add decline tests for `true` and non-literal flags.

---

### MEDIUM

#### SCR-D3-2026-09-14-01: `decompile_script` inverts named auto states — it hoists the auto state to script scope and demotes the real default state to a non-auto `State ''`, the reverse of Champollion (983 vanilla scripts)

- **Severity**: MEDIUM (a wrong AST on vanilla content across all three Papyrus games, silently, with no decline; not HIGH today because no production consumer changes outcome, see Impact; escalates to HIGH as soon as any state-aware consumer reads a `.pex` AST)
- **Dimension**: Decompiler Control-Flow / Boolean / Lower (script assembly)
- **Untrusted-Input**: No
- **Location**: `crates/pex/src/decompile/lower.rs` `decompile_script` (the `for state in &object.states` loop: `if is_auto_state(object, state)` hoists; the else arm pushes `ScriptItem::State { is_auto: false }` hardcoded); `crates/pex/examples/pex_corpus_smoke.rs::expected_top_level_item_count` (encodes the same rule); `.claude/commands/audit-scripting/SKILL.md` Dim 3 assembly bullet (encodes the wrong premise)
- **Status**: NEW. #3786 and #3943 fixed this comparison's case-sensitivity, not what it compares.
- **Description**: In a `.pex` object the script-scope state is always the one named `""`. `auto_state_name` names the state the object *boots into*: `""` without an `Auto State`, `"X"` with `Auto State X`. `decompile_script` treats "the state whose name equals `auto_state_name`" as script scope, which is only right when that name is `""`. For `Auto State waiting`, three things go wrong:
  - `waiting`'s callables become top-level items;
  - the real `""` state becomes `State '' { is_auto: false }`;
  - no item ever carries `is_auto: true`.
- **Evidence**: The orchestrator read the Champollion reference, `Decompiler/PscCoder.cpp::writeStates` (lines 468–497): `if (name.empty()) writeFunctions(0, ...)`, else emit `State name`, prefixed `Auto ` when `caselessCompare(name, autoStateName) == 0`. The `.psc` frontend agrees (`defaultRumbleOnActivate.psc` → `State { is_auto: true }`). Dim 3 corpus census over 26,641 objects:

  | Game | named `Auto State` | …`State ''` holds user callables | …`on`-events | …auto overrides a same-named default |
  |---|---|---|---|---|
  | Skyrim SE | 570 | 299 | 206 | 36 |
  | FO4 | 276 | 133 | 92 | 25 |
  | Starfield | 137 | 74 | 38 | 24 |
  | **Total** | **983** | **506** | **336** | **85** |

  Concrete case: `defaultsetstagealiasscript.pex` (`auto_state_name='waitingForPlayer'`) decompiles to `State ''` {onActivate, testForTrigger, GetState, GotoState}, `State 'hasBeenTriggered'`, and 11 `waitingForPlayer` handlers at top level.
- **Impact**: No present-day divergence. `quest_stage_gate::find_advance_event`, `fragment/populate.rs::function_body`, `papyrus_provider/lower_program.rs` and `compatibility.rs` all flatten top-level and `State` items in body order, and none reads `is_auto` or a state name. The hazard is structural: the two frontends disagree on state topology for 983 vanilla scripts, the smoke harness cannot see it, and in the 85 override cases a future state-aware consumer would pick the wrong handler.
- **Related**: #3786, #3943; the stale `boolean.rs` departure-1 note ("the smoke harness discards the resulting `Script`", stale since #3017)
- **Suggested Fix**:
  - Route the `name.is_empty()` state to script scope, and emit each non-empty state as `State { is_auto: name.eq_ignore_ascii_case(auto_state_name) }`.
  - Update `expected_top_level_item_count` to the same rule.
  - Rewrite `mismatched_casing_auto_state_still_hoists_its_callables_to_script_scope`. It models a layout the corpus never contains (all 983 also have a `""` state) and pins the wrong behavior.
  - Add a `lower.rs` unit test (`""` {OnInit} + `Waiting` {OnActivate}) and a `.pex` ↔ `.psc` topology test on `defaultRumbleOnActivate`.
  - Correct the SKILL.md bullet.

#### SCR-D4-2026-09-14-01: left-deep postfix/infix chains bypass `MAX_EXPR_DEPTH`; ~0.4 MB of `.psc` or console text aborts with a stack overflow on AST Drop

- **Severity**: MEDIUM (the domain table says HIGH for parser stack overflow; downgraded one step because no runtime path feeds game or mod `.psc` into the parser. Re-escalate if a `.psc` ingest path lands.)
- **Dimension**: Papyrus Lexer & Pratt Parser
- **Untrusted-Input**: Partial (`extender_preflight` on mod-author `.psc`; localhost debug console `eval_expr` → `parse_expr`, 16 MB frame cap)
- **Location**: `crates/papyrus/src/parser/expr.rs::parse_expr_bp_inner` (postfix `continue` arms for `.` / `[` / `(` / `as`, and the infix loop); `crates/papyrus/src/ast.rs` (`Box`ed `Expr` children, derived recursive Drop/Clone/Debug)
- **Status**: NEW (no match for postfix-chain / Drop overflow; #1270, #3783 and #3933 cover recursion, not iterative chain depth)
- **Description**: `MAX_EXPR_DEPTH` counts only recursive `parse_expr_bp` calls. `a.a.a…`, `a()()…`, `a[0][0]…` and `a+a+…` are built by the *iterative* loop, which wraps `lhs` in a new `Box` each iteration while `expr_depth` stays at 1. The finished tree is as deep as the input is long, and Drop plus every recursive walker recurse once per level. The cap's own doc, and the 09-11 matrix's "Total: Yes", assume AST depth ≤ 256.
- **Evidence**: The orchestrator confirmed the postfix arms never touch `expr_depth`. Dim 4 probe (8 MB main thread): `chain_member` at 100k–170k terms parses and drops fine; at 200k / 400k / 700k / 1M terms it hits `thread 'main' has overflowed its stack … aborting` (SIGABRT). `chain_add`, `chain_call` and `chain_index` behave the same, as does `parse_expr` on 1M terms (158 ms parse, then abort on drop).
- **Impact**: Process abort of the engine via the debug console, or of tooling that scans third-party `.psc`. Any recursive AST consumer inherits the unbounded depth; the #4113 reasoning is `.pex`-only.
- **Related**: #1270, #3783, #4113
- **Suggested Fix**: Count chain length as depth: increment (or locally count against `MAX_EXPR_DEPTH`) on each postfix/infix iteration that wraps `lhs`, returning `ExpressionTooDeep`. Regression test: `a` + `.a` ×10,000 via `parse_expr` returns `ExpressionTooDeep`.

#### SCR-D4-2026-09-14-02: the Pratt loop's newline-skipping `peek()` glues a line starting with `(` onto the previous statement's expression, giving a silent wrong AST with zero errors

- **Severity**: MEDIUM (silent wrong AST from valid source; no runtime lowering consumes `.psc` ASTs today)
- **Dimension**: Papyrus Lexer & Pratt Parser
- **Untrusted-Input**: Partial (same reach as D4-01; also affects every in-repo test and tooling result built from hand-written `.psc`)
- **Location**: `crates/papyrus/src/parser/expr.rs::parse_expr_bp_inner` (`while let Some(tok) = self.peek()`); `crates/papyrus/src/parser/mod.rs::peek_with_span` (skips `Token::Newline`)
- **Status**: NEW. Same root cause as #2656 (closed), which was fixed only at `parse_property_flags`.
- **Description**: `docs/engine/papyrus-parser.md` makes `Token::Newline` the statement terminator unless `\` joins lines, and `preprocess` has already removed every `\` continuation. After an operand, though, the loop's `peek()` skips the newline. A next line starting with `(` becomes a call postfix on the previous expression, and the next statement's `.X()` suffix chains on. `(expr as Type).Method()` is valid statement syntax, so valid source reaches this.
- **Evidence**: The orchestrator confirmed `peek()` skips newlines while `peek_raw()` does not. Dim 4 probe, all with `errs=0`:
  1. `SetStage(10)` ⏎ `(akRef as ObjectReference).Disable()` → one `ExprStmt`: `Call{MemberAccess{Call{Call{SetStage,[10]},[akRef as ObjectReference]},Disable},[]}`.
  2. `If akActionRef == Game.GetPlayer()` ⏎ `(GetOwningQuest() as QF_Foo).Bar()` ⏎ `EndIf` → the condition absorbs the body line, and the body is empty.
  3. `Actor a = b as Actor` ⏎ `(a as ObjectReference).Disable()` → one `VarDecl`.

  Prevalence: 0 hits in the 33 on-disk `.psc` files. The Skyrim and Starfield `.psc` corpora are not on disk, so vanilla prevalence is **UNVERIFIED**.
- **Impact**: Statements silently disappear into a neighbouring expression, and the `If` condition/body split gets corrupted. `extender_preflight` under-reports calls. The psc-vs-pex fidelity gate cannot catch it, because the r5 fixtures lack the shape.
- **Related**: #2656, #1734
- **Suggested Fix**: Decide postfix/infix continuation with `peek_raw()` in `parse_expr_bp_inner`. Audit the other same-line `peek()`/`check()` decisions (`parse_type`'s `[`, `parse_qualified_ident`'s `:`, `parse_call`'s comma), and pin with a test built from example 1.

#### SCR-D5-2026-09-14-02: `Effect::SetInChargen` fields and the persisted `CinematicPresentationState` flags carry invented semantics

- **Severity**: MEDIUM
- **Dimension**: Recognizer-Chain Soundness
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/effects.rs` `Effect::SetInChargen` + `prim_set_in_chargen`; `crates/scripting/src/cinematic.rs` `in_chargen` field doc and `set_in_chargen`; `crates/save/src/snapshot.rs` (FORMAT v23)
- **Status**: NEW
- **Description**: The real signature is `Game.SetInChargen(Bool abDisableSaving, Bool abDisableWaiting, Bool abShowControlsDisabledMessage)`, the same in Skyrim `game.pex` and FO4 `Game.psc:312`. The code documents it as `(abEnabled, abWaitForRaceSex, abStayInFirstPerson)`. The values are kept in the right positions, but every label is wrong, and the real semantics (block save and wait) are enforced nowhere, so the lowering claims a statement it doesn't model.
- **Evidence**: The orchestrator re-ran the probe on `game.pex`: `Game.SetInChargen(Bool abDisableSaving, Bool abDisableWaiting, Bool abShowControlsDisabledMessage) flags=0x3 [global native]`. The string table has none of the three names the code uses.
- **Impact**: No reader today. The wrong names are now baked into save format v23 and a public resource, so the first consumer (the documented future "chargen camera/input mode") will read disable-saving as "in chargen". Saving and waiting are not blocked during MQ101 chargen as vanilla requires.
- **Related**: SCR-D5-2026-09-14-01
- **Suggested Fix**: Rename the fields and docs to `disable_saving` / `disable_waiting` / `show_controls_disabled_message` before a consumer lands, ideally in the same format bump. Add a non-literal-bool decline test.

#### SCR-D5-2026-09-14-03: `StartCombat` accepts the player as the combatant; `npc_combat_ai_system` would then drive the player's Transform and auto-strike

- **Severity**: MEDIUM
- **Dimension**: Recognizer-Chain Soundness
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/effects.rs` `prim_start_combat` (`actor: receiver_actor(object, scope)?`); `crates/scripting/src/fragment/effects.rs` `Effect::StartCombat` arm; `byroredux/src/systems/combat_ai.rs` `npc_combat_ai_system`
- **Status**: NEW
- **Description**: `receiver_actor` maps `Game.GetPlayer()` or a player local to `ActorRef::Player`. `prim_start_combat` accepts that as the receiver, and dispatch inserts `AiCombatState` on the player entity. `npc_combat_ai_system` has no player exclusion: it overwrites that entity's `Transform` with a straight-line chase and emits `HitEvent`s on cooldown. The runtime models only an NPC chase-and-strike, so a player receiver is unmodeled and should be declined. The real engine behavior of `Player.StartCombat` is **UNVERIFIED** (CK wiki unreachable), but it is not "AI moves the player".
- **Evidence**: `Actor.StartCombat(Actor akTarget)` (probe). `start_combat_declines_on_wrong_arg_count` uses a player receiver but tests only arity.
- **Impact**: A fragment of the form `Game.GetPlayer().StartCombat(X)` fights the character controller for the player's Transform. Reachability in vanilla or mods was not measured; MQ101's pinned shapes use alias NPC receivers.
- **Related**: SCR-D5-2026-09-14-01, SCR-D6-2026-09-14-01
- **Suggested Fix**: Decline when the receiver is `ActorRef::Player` (optionally also when receiver == target), with a correct-arity decline test.

#### SCR-D6-2026-09-14-01: `HitEvent` now has two producers, both `insert`ing into the target's single SparseSet slot, so same-frame hits overwrite each other

- **Severity**: MEDIUM
- **Dimension**: Scripting Runtime Systems
- **Untrusted-Input**: No
- **Location**: `byroredux/src/systems/combat_ai.rs` `npc_combat_ai_system` phase 2 (`events.insert(target, HitEvent{..})`); `byroredux/src/combat.rs` `combat_input_system` (`events.insert(target, ..)`); stale "exactly one HitEvent producer" comments in `byroredux/src/combat.rs` and `byroredux/src/npc_spawn.rs`
- **Status**: NEW. This is the same defect class as #1864 / #3277 on `QuestStageAdvancedBatch`, fixed there with a merging helper.
- **Description**: `HitEvent` is `SparseSetStorage` with one row per target, and `insert` replaces the row. Schedule is `combat_input_system` (`update.rs` ~127) → `npc_combat_ai_system` (~157) → `combat_damage_system` (~175). If the player and an AI strike one target in the same frame, the AI's insert replaces the player's hit. If several NPCs strike one target in the same frame, all but the last are dropped. NPCs armed by one fragment start with identical cooldowns and stay synchronized, so the loss persists for the whole fight.
- **Evidence**: The orchestrator confirmed both producers use `events.insert(target, byroredux_scripting::HitEvent{..})` with no existing-row check, that `HitEvent` is `SparseSetStorage`, and the schedule order. All combat_ai tests use a single attacker.
- **Impact**: Damage and on-hit script events (including the extension OnHit path) are silently lost in MQ101's Helgen combat gate (stages 270/272/365).
- **Related**: #3277, #1864, #4105
- **Suggested Fix**: Make the per-target hit list-valued (a `HitEventBatch`, or accumulate into an existing row) behind one push helper both producers call, and fix the two comments.

#### SCR-D6-2026-09-14-02: `npc_combat_ai_system` holds `PhysicsWorld` across every storage it acquires, reversing `ragdoll_writeback_system`'s `Transform → PhysicsWorld` edge

- **Severity**: MEDIUM (not a live deadlock: both systems are exclusive and run in different stages; trips the lock-order checker and breaks the rule #2134 / #3262 / #3655 enforced)
- **Dimension**: Scripting Runtime Systems
- **Untrusted-Input**: No
- **Location**: `byroredux/src/systems/combat_ai.rs` `npc_combat_ai_system` (`world.try_resource::<PhysicsWorld>()` at the top, alive through `query::<AiCombatState>`, `query::<Transform>`, `attack_damage`'s storages, and phase 2's `query_mut::<Transform/AiCombatState/HitEvent>`); `byroredux/src/ragdoll.rs` `ragdoll_writeback_system` (Transform/Parent/Children/GlobalTransform/LocalBound/WorldBound held, `PhysicsWorld` taken last)
- **Status**: NEW (#3580 was a different system's `PhysicsWorld` hold)
- **Description**: The crate-wide rule, per the #3655 comment: no storage is acquired under a `PhysicsWorld` guard. The combat AI's module doc claims to mirror `wander_system_inner`, but wander takes `PhysicsWorld` only after its storage guards drop. Every frame (the `AiCombatState` storage is always registered) the combat AI records `PhysicsWorld → Transform`, while `ragdoll_writeback_system` records `Transform → PhysicsWorld`.
- **Evidence**: The orchestrator confirmed the acquisition order: `combat_ai.rs:50` `PhysicsWorld`, then `:56` `Transform`, versus `ragdoll.rs:507` `Transform`, then `:529` `PhysicsWorld`. The lock tracker records edges on both read and write. The resulting panic under `BYRO_LOCK_ORDER_CHECK=1` is derived from the code, not observed (no engine launch).
- **Impact**: A debug run with the lock-order checker enabled panics at boot, which blinds the tool that gates promoting a system to a parallel lane. Moving either system to a parallel lane would turn this into a real ABBA risk.
- **Related**: #2134, #3262, #3655, #2404
- **Suggested Fix**: Restructure like #2134 / #3262. Phase 1a snapshots actor and target transforms, reach, damage and cooldown with no `PhysicsWorld` held. Phase 1b takes `PhysicsWorld` alone for `step_toward`. Drop it before the phase-2 `query_mut`s.

#### SCR-D7-2026-09-14-01: invisible trigger volumes bypass the `ReferenceEnableState` spawn gate, and `disable_after_advance` is only a live component removal — a once-only quest trigger re-arms and can re-fire `SetStage` after every cell reload

- **Severity**: MEDIUM
- **Dimension**: Engine Attach & Trigger Wiring
- **Untrusted-Input**: No
- **Location**: `byroredux/src/cell_loader/references/synth_child.rs` `spawn_synth_child` trigger branch (returns before the only `spawn_placed_instances` call); `crates/scripting/src/papyrus_demo/quest_advance.rs` `quest_advance_system` (`disable_sources` → `components.remove`); `crates/scripting/src/translate/recognizers/quest_stage_gate.rs` `recognize_specific_actor_trigger`; `crates/scripting/src/quest_stages.rs` `QuestStageState::set_stage`
- **Status**: NEW (#3278 and #3789, both closed, added and ordered the gate only on the mesh path)
- **Description**: #3278 put the only runtime consumer of `ReferenceEnableState` (`placement_is_disabled`) inside `spawn_placed_instances`. The trigger branch spawns its `TriggerVolume`, stamps identity, attaches the script and returns without consulting the ledger. This has two consequences:
  1. A fragment `Disable()` on a trigger writes the ledger, but no load path reads it for triggers.
  2. The vanilla actor-base stage trigger's `disableWhenDone` / `onlyOnce` lowers to `disable_after_advance: true`, which `quest_advance_system` implements only as `components.remove(entity)`. Nothing is persisted, and on reload the attach chain inserts a fresh armed component. Its only condition, `GetStageDone(prereq) == 1`, stays true. `set_stage` overwrites `current_stage` (it can move backward) and re-queues the stage fragment.
- **Evidence**: The orchestrator confirmed that `placement_is_disabled` has one production call site (`spawn.rs` inside `spawn_placed_instances`, called at `synth_child.rs:671`), and that the trigger branch (`synth_child.rs` ~216–243) `return`s before reaching it.
- **Impact**: After any cell revisit or save load, a once-only quest trigger fires again when its gated actor re-enters, for example MQ101's cart-horse `BaseForm` triggers. Non-idempotent stage fragments (`AddItem`, `MoveTo`) re-run, and `current_stage` can regress, with nothing logged. **UNVERIFIED**: the corpus count of such triggers, and whether shipped Papyrus re-runs a fragment on `SetStage` of a done stage. The engine-side re-fire does not depend on either.
- **Related**: #3278, #3789, SCR-D7-2026-09-14-02
- **Suggested Fix**: In the trigger branch, skip volume + attach when `placement_is_disabled` holds for the placement FormID. In `quest_advance_system`, record `disable_after_advance` into `ReferenceEnableState`, keyed by the source's `SceneAliasCandidate::reference_form_id`. Test: fire → unload → reload → no re-fire.

#### SCR-D7-2026-09-14-02: actor jobs and LIGH-only / fxlight placements ignore `ReferenceEnableState`, so a `Disable()`d NPC or light respawns with full content on the next load

- **Severity**: MEDIUM
- **Dimension**: Engine Attach & Trigger Wiring
- **Untrusted-Input**: No
- **Location**: `byroredux/src/cell_loader/references/mod.rs` `load_references_budgeted` actor arm (builds its own placement root, bypassing `spawn_placed_instances`); `byroredux/src/cell_loader/references/synth_child.rs` LIGH-only and fxlight branches
- **Status**: NEW (#3278's completeness checklist asked for a consumer test; the shipped tests exercise only the mesh path)
- **Description**: `NpcSpawnJob` never reaches the #3278 gate, so a disabled ACHR/ACRE spawns its body, armor, skeleton and collision, and gets identity plus scripts. The LIGH-only and fxlight branches build `LightSource` entities directly. #3278's comment says one gate "covers all three at once" (render/collide/light); that is true only for the mesh path.
- **Evidence**: `grep placement_is_disabled` finds one production call site. The orchestrator confirmed the actor arm `continue`s before `spawn_synth_child`.
- **Impact**: A Papyrus `Disable()` on an actor, common in quest fragments, is silently undone on the next cell load, leaving a visible, collidable, alias-fillable NPC the script removed. Disabled lights come back the same way.
- **Related**: #3278, #3789, SCR-D7-2026-09-14-01
- **Suggested Fix**: Hoist the enable check to the per-REFR level in `load_references_budgeted`, before the actor/synth dispatch, with a per-branch policy (identity-only spawn vs skip), and test the actor and LIGH-only arms.

---

### LOW

#### SCR-D5-2026-09-14-04: `ShowRaceMenu` / `RequestSave` / `RequestAutoSave` lower to counters nothing reads, and `fragment_coverage` counts them as claimed

- **Severity**: LOW
- **Dimension**: Recognizer-Chain Soundness
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/effects.rs` `prim_show_race_menu` / `prim_request_save` / `prim_request_auto_save`; `crates/scripting/src/cinematic.rs` `show_race_menu` / `request_save`; `crates/scripting/examples/fragment_coverage.rs` (`claimed` tally)
- **Status**: NEW
- **Description**: The arities match (0-arg native globals). Dispatch only bumps `race_menu_shown_count` / `save_requested_count`, which nothing reads. No race menu opens and no save is written; the variant docs say so. However, `fragment_coverage` counts any `Some(effects)` as fully lowered, so these stubs feed the commit's reported 45.9% → 50.0% with no placeholder marking. There are no over-arity decline tests for the two save primitives.
- **Impact**: No state corruption. The costs are misleading coverage numbers and a silent MQ101 divergence (no race choice, no chargen saves) that a reader of the tally cannot see.
- **Related**: SCR-D5-2026-09-14-02
- **Suggested Fix**: Tag placeholder effects (e.g. `Effect::is_placeholder()`), report them as a separate "claimed-by-stub" count, and add over-arity decline tests.

#### SCR-D6-2026-09-14-03: `SetLockLevel` on an untouched keyed reference records `key_form_id: None` while the live component keeps the key — ledger and component drift, and the drift is saved

- **Severity**: LOW (latent: `Locked::key_form_id` has no reader; `activation_is_blocked` does no key check)
- **Dimension**: Scripting Runtime Systems
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment/state.rs` `ReferenceLockState::set_lock_level` (`None => self.set_locked(form_id, lock_level, None)`); `crates/scripting/src/fragment/effects.rs` `Effect::SetLockLevel` arm (pushes only the requested level)
- **Status**: NEW (residual of #4136)
- **Description**: #4136's commit says the effects record the *outcome*, so ledger and component can't drift. The `SetLocked` arm does; `SetLockLevel` pushes only the level. On an authored-keyed door with no prior override, the live `Locked` stays `{level, Some(K)}`, the ledger gets `{level, None}`, and on the next load `scripted_lock_override` wins and the key is gone. The test `set_lock_level_on_an_untouched_reference_records_the_authored_lock` asserts `None`, which is only correct for keyless locks.
- **Impact**: None today. Once key checks land, a script-re-levelled keyed door can't be opened with its key after a revisit or load, and saves written before the fix keep the keyless override.
- **Related**: #4136, #3159
- **Suggested Fix**: In the `SetLockLevel` arm, read the component back and push `DeferredLockChange::Locked { lock_level, key_form_id }` (the outcome), mirroring `SetLocked`.

#### SCR-D6-2026-09-14-04: `SetLocked` / `SetLockLevel` on a non-resident reference record nothing, unlike `Enable` / `Disable`

- **Severity**: LOW
- **Dimension**: Scripting Runtime Systems
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment/effects.rs` `Effect::SetLocked` / `Effect::SetLockLevel` arms (`resolve_object(..)?` runs before `lock_ledger_key`)
- **Status**: NEW
- **Description**: `lock_ledger_key` could key a direct-property target with no loaded entity, but both arms return early when `resolve_object` fails. #3278 made `Disable` / `Enable` resolve `resolve_property_form_id(..).or_else(entity)` without requiring residency. As a result, a fragment that unlocks a door in a non-resident cell is a no-op (debug log only), and the authored XLOC stands when the player arrives.
- **Impact**: A quest-unlocked door in an unloaded cell stays locked. Vanilla frequency is unmeasured, and whether shipped `ObjectReference.Lock` applies to unloaded persistent refs is **UNVERIFIED**.
- **Related**: #3278, #4136
- **Suggested Fix**: For `locked == false` with a direct-property FormID, push `DeferredLockChange::Unlocked` even without an entity. Keep declining a re-lock (it needs the authored XLOC) and document why.

#### SCR-D7-2026-09-14-03: `materialize_scene_actor_alias_stubs` is a second identity-only actor-stub path that never attaches scripts (sibling of #4112)

- **Severity**: LOW
- **Dimension**: Engine Attach & Trigger Wiring
- **Untrusted-Input**: No
- **Location**: `byroredux/src/asset_provider/script.rs` `materialize_scene_actor_alias_stubs` (Skyrim-only, hardcoded MQ101 `0x0003372B` / SCEN `0x000BECD4`), called from `byroredux/src/scene/world_setup.rs`
- **Status**: NEW (present at `b3db49fa`; #4112's location and fix cover only `exterior.rs`)
- **Description**: This function materializes MQ101's scene-actor ACHRs (Tullius / Ulfric / Elenwen) via `spawn_logical_quest_reference` plus a `RemoteSceneActorStub` marker, with no `attach_*` call. Base SCRI/VMAD and the ACHR's own VMAD are therefore dropped while the actor is stub-only. Every `spawn_logical_quest_reference` caller in `synth_child.rs` does attach.
- **Impact**: Scoped to one demo scene. Whether these records carry VMAD is **UNVERIFIED**. Neither stub path is counted in `M47.2 scripts:`.
- **Related**: #4112
- **Suggested Fix**: Fix together with #4112 through one shared "logical actor identity + attach" helper. Attach once per `reference_form_id` so a later real load of the home cell doesn't double-attach.

#### SCR-D8-2026-09-14-01: the HKX packfile gate ignores fileVersion / contentsVersion / reusePaddingOptimization, so a non-hk_2010 layout is misread rather than refused

- **Severity**: LOW
- **Dimension**: Havok Idle / Cinematic Slice
- **Untrusted-Input**: Yes (no panic; wrong decode or a misleading error)
- **Location**: `crates/hkx/src/packfile.rs::Packfile::parse` (layout-rules check); `crates/hkx/src/animation.rs::Layout::new`
- **Status**: NEW (made broader by `fb8173fe`, which made the header the only source of field offsets)
- **Description**: `parse` validates `layoutRules[0]` (pointer size) and `[1]` (endianness) only. `Layout::new` assumes the Havok 2010 class field order (`m_fileVersion` @0xC and `m_contentsVersion` @0x28 are never read) and MSVC layout with `reusePaddingOptimization == 0` (@0x12 never read). Per `hkStructureLayout.h`, GCC-style tail-padding reuse shifts derived members by 4 on 64-bit. FO4 `hk_2014.1.0-r1` files (fileVersion 11) are refused only by accident, with the misleading error `global fixups precede local fixups` (0 of 404 decode).
- **Evidence**: Dim 8 probe: every sampled vanilla LE/SE file reads fileVersion 8, `hk_2010.2.0-r1`, `[4|8,1,0,1]`.
- **Impact**: No impact on vanilla content. A modded Skyrim `.hkx` in another Havok layout would animate wrong instead of logging "unsupported layout". The consumer is Skyrim-gated.
- **Related**: #3011
- **Suggested Fix**: Return `UnsupportedLayout` unless `fileVersion == 8`, `layoutRules[2] == 0` and `contentsVersion` starts with `hk_2010`. Add rejection cases next to `rejects_layouts_other_than_32_or_64_bit_little_endian`, plus a CI-running `Layout::new(4)` offset pin.

#### SCR-D8-2026-09-14-02: the cinematic router re-implements `base_form_advance_is_eligible` inline; #3954's "one shared predicate" is shared only on paper

- **Severity**: LOW
- **Dimension**: Havok Idle / Cinematic Slice
- **Untrusted-Input**: No
- **Location**: `byroredux/src/systems/cinematic.rs::scene_trigger_actor_approach_system_inner` (inline `target_stage >= current_stage` / `!get_stage_done` / `evaluate_condition_list` conjunction, ~545–585); `crates/scripting/src/trigger.rs::base_form_advance_is_eligible`
- **Status**: NEW — residual of #3954 (CLOSED). The 09-11 report marked the sharing fixed.
- **Description**: The predicate's doc says it was "extracted so the router's target selection and the gate's `next_ready` share one predicate". Only the gate calls it; the router hand-copies the same conjunction. `scene_phase_awaited_stage`, #3954's other shared predicate, *is* called by the router.
- **Evidence**: The orchestrator confirmed that every call site of `base_form_advance_is_eligible` is in `trigger.rs` (the gate plus tests), and that `cinematic.rs` contains the inline conjunction.
- **Impact**: Equivalent today (both sides traced). A future change to the gate's rule won't reach the router, which is exactly the drift #3954 was filed for and would stall the MQ101 cart with no diagnostic.
- **Related**: #3954, #3937
- **Suggested Fix**: Call `byroredux_scripting::base_form_advance_is_eligible(...)` in the router's between-scenes cap, or add a source-shape test beside the #3937 guard requiring the call.

---

## Existing findings re-verified (not re-filed)

| Issue | Finding | State at HEAD |
|---|---|---|
| #4112 | Persistent-worldspace logical-actor stubs never attach scripts | Still open — `exterior.rs` logical-stub loop unchanged |
| #4113 | `MAX_EXPR_DEPTH` / `MAX_REBUILD_DEPTH` uncorrelated | Still open. SCR-D4-2026-09-14-01 shows the `.psc` side has no tree-depth bound at all |
| #4115 | `control_flow.rs` module doc says "advanced past" | Still open |
| #4116 | `ActivateEvent` consumer-order test omits `mg07_on_activate_dispatch` | Still open; order itself still correct |
| #4190 | Approach system deep-clones `ScenePlayer` | Still open |
| #3817 | `HorseTetherState` / `ActorCinematicState` never terminate | Still open |

## Future-Phase Readiness

- **Obscript / `SCTX` (M47.2 Phase 5)**: still unbuilt. This cycle adds a fourth untrusted-input lesson for the third frontend. Beyond recursion bounds as a tree property (#3933), time bounds (#3938) and decline at every consumer (#3934), the frontend also needs **memory bounded relative to input size**: share interned strings, never copy per reference (SCR-D1-2026-09-14-01). OOM is the one failure the panic net can never catch.
- **The fragment lowerer (b2)**: live and still growing (chargen, combat). The new failure mode is primitives modeled from call sites instead of declarations (SCR-D5-2026-09-14-01/-02). Future primitives should cite the `.pex` native signature, not the fragment that calls them; the `d5probe` recipe (parse the game's own `faction.pex` / `game.pex` / `actor.pex`) is cheap and exact.
- **State-aware consumers**: SCR-D3-2026-09-14-01 must land before any `GotoState` runtime, boot-state reader or state-scoped handler lookup consumes a `.pex` AST.
- **`ReferenceEnableState` as the persistence contract**: #3278's gate now has three known bypasses (triggers, actors, lights) plus the `disable_after_advance` in-memory-only removal. A single per-REFR gate site (SCR-D7-2026-09-14-02's fix) would make this structural.
- **M47.1 condition resolvers**: unchanged. Re-verification against a live headless cell with real CTDA data is still outstanding (no engine launch this cycle).
- **M47.3 Phase 4+**: unchanged. Created Object alias spawn, Story Manager event fills, true `LCTN` traversal, reference-collection aliases, unloaded-world Find-Matching, and the injected overlay families remain parsed but not applied.
- **The SDK / extender layer**: still unaudited. Apply SCR-D1-2026-09-14-01's memory-amplification lens to its `SCDA` decode first.

## Findings Count

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 2 |
| MEDIUM | 9 |
| LOW | 6 |
| **Total** | **17** |

New findings by dimension: 1 (1 HIGH), 3 (1 MEDIUM), 4 (2 MEDIUM), 5 (1 HIGH, 2 MEDIUM, 1 LOW),
6 (2 MEDIUM, 2 LOW), 7 (2 MEDIUM, 1 LOW), 8 (2 LOW). Dimension 2 found nothing new. Two headline
verdicts changed since 09-11: untrusted-input robustness went from MET to **NOT MET**, and the
decline-on-unmodeled invariant went from held to **one new violation**. Skill doc-rot found this pass
(fix when next syncing `audit-scripting/SKILL.md`): the Dim 3 assembly bullet's auto-state premise
(SCR-D3-2026-09-14-01), and `fragment.rs::X` symbol references that now live in `fragment/*.rs`.

---
*Generated by the `/audit-scripting` skill. Suggested next step:*
`/audit-publish docs/audits/AUDIT_SCRIPTING_2026-09-14.md`
