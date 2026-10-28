**HEAD**: fa6be4a6b · **Baseline**: `docs/audits/AUDIT_SCRIPTING_2026-10-21.md` (HEAD 4a3e89d56) · **Audited**: Dims 1, 2, 3, 4, 5, 6 (delta-first; 90 in-scope commits since 4a3e89d56) · **Unchanged since baseline (skimmed)**: Dim 7 (one behavioural commit, 7697b7cd7; spot-checked only)

# Scripting Runtime Audit: 2026-10-27

Suite run (`--preset comprehensive`). I ran it alone, with no sub-agents. I left the tree untouched and did not launch the engine. Scratch notes are in `/tmp/audit/scripting/dim_{1..7}.md`, and every item in them is either in this report or explicitly dropped below.

## Build and test state: CLEAN

```
cargo test -q -p byroredux-scripting                                     1280 passed; 0 failed; 11 ignored  (10-21: 1205)
cargo test -q -p byroredux --bin byroredux -- boot::schedule cell_loader::references reference_enable_gate attach::tests
                                                                         83 passed; 0 failed; 2 ignored
```

These runs used the shared repo `target/` only. Main CI is red for reasons outside this audit's scope: the `render_one_frame` size pin (REG-2026-10-27-01) and the lavapipe lane (SAFE-2026-10-27-01). The suite's lock-order cycles in this crate are already filed (ECS-01, below).

## Executive Summary

Shipped since 10-21:
- The #6139 ObScript command table, and the #6142 split of `apply_effect` / `evaluate_guard`.
- FO3/FNV/Oblivion result scripts: stage items and INFO Begin/End (6db47b0e4).
- A fail-closed `If` in the ObScript VM. An arm no longer runs on a declined getter's `0` stand-in.
- #6215, truncated global reads in the lowering, and #6216, Int-typed VM natives and locals.
- Oblivion's crime, jail and fine commands. Faction crime gold for Skyrim/FO4.
- #5647 (story-event collision), #5646 (`GetRandomPercent`), #6221 (untracked state declines), #6226 (`PlaySound` at the listener) and #6338 (generation-keyed FormID index).

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 0 |
| LOW | 2 |

The headline finding is SCR-2026-10-27-01. The new Oblivion crime runtime made an old caller-blind command row dangerous. `GlarthirRef.ModCrimeGold 5000` in vanilla MS38 now gives the **player** a 5000 bounty, and every guard who sees the player then comes to arrest them.

### Untrusted-input verdict (runtime-side decoders)

**No panic, OOB, OOM or overflow found.** The newly reachable `SCDA` path is the result-script entry (`ObScriptProgram::run_result_script`). It goes through the shared `obscript::statement_frame`, which checks each payload against the compiled bytes. A stream that ends mid-statement comes back `Malformed` (`a_truncated_result_script_is_malformed`). The `If` nesting cap and argument bounds are unchanged from 10-21. The verdict for the compiler frontends is in `AUDIT_PAPYRUS_2026-10-27` (0 findings).

## Decline-Invariant Audit (delta)

| Decline point | Verdict |
|---|---|
| `truncated_global_guard` (#6215): `GetValueInt` / `as Int` guards become exact bounds on the stored float | Conservative. I re-derived each bound by hand, and the brute-force test covers negatives and fractions. A non-integral literal declines. |
| `classify_value` with `as Int` over a non-integral operand, and `GuardReading::GlobalInt` in a value | Declines, and the VM runs it instead. |
| Non-Int casts in `guard_reading` / `classify_value` / `as_num` | **Leaks**: an `as Bool` cast is treated as transparent (SCR-2026-10-27-03). |
| VM Int natives (#6216): `classify_native_number` returns `Value::Int` | Correct. Script variables are typed on read (`member_type_in`). |
| ObScript `If` whose condition read a declined getter | Neither arm runs (`declined_calls` delta; `a_condition_that_read_a_declined_value_runs_no_arm`). |
| Oblivion `PayFine` / `PayFineThief` / `GoToJail` | Each declines whole before mutating anything: no crime model, cannot pay, no jail in reach, or already serving. |
| Oblivion `Get/Set/ModCrimeGold`, `GetGold` | **Wrong subject**: the calling reference is ignored (SCR-2026-10-27-01). |
| `SendTrespassAlarm` | **Reports done with no effect** (SCR-2026-10-27-02). |

## Runtime Lifecycle Matrix (delta)

| Item | State |
|---|---|
| Marker drains | Pattern-B rows for the seven ObScript request markers are pinned (#6267). The new types since 10-21 are `PendingGroupSwap`, `UnloadedReferenceStub`, `AssaultedByPlayer` and `PendingEvidenceDeposits`. None is an undrained transient. |
| Story events | `story_manager_dispatch_system` drains the marker and `PendingScriptStoryEvents` unconditionally, before the SmTree early-out. So FO3/FNV neither strand markers nor grow the queue. |
| Result-script cascade | `MAX_STAGE_SCRIPT_ROUNDS` = 16 per run. It polls its own journal subscriber and logs `missed_events`. |
| Journal subscribers | Four, unchanged. |
| CTDA crime rows | Split per dialect: Oblivion 116 `GetCrimeGold`, Skyrim 459 `GetCrimeGold(faction)`, 329 `IsTurnArrest` (Oblivion only), `IsGuard` unmodeled on Skyrim and later. |

## Findings

### SCR-2026-10-27-01: Oblivion's caller-scoped `GetCrimeGold` / `SetCrimeGold` / `ModCrimeGold` / `GetGold` ignore the calling reference and read or write the player's state
- **Severity**: HIGH (vanilla quest path; corrupts saved state; drives guard aggression)
- **Dimension**: Legacy ObScript
- **Location**:
  - Handlers: `crates/scripting/src/obscript_more/state.rs:35-47` (`get_crime_gold`, `cmd_1075`, `cmd_1076`, all taking `_ctx`).
  - Rows: `:638-640` (`needs_caller: false`).
  - `crates/scripting/src/obscript_more/tail.rs:573-581` (`get_gold`), row `:831`.
- **Status**: NEW. The handlers predate this window. They became harmful when e161c45f3/73924db73/1a67c9f30 made the bounty drive guards, jail and dialogue.
- **Untrusted-Input**: No
- **Description**: The CS wiki defines all four as actor functions:
  - *SetCrimeGold*: "Sets the current crime gold on an actor".
  - *ModCrimeGold*: "Modifies the current crime gold on an actor".
  - *GetCrimeGold*: "Returns the current crime gold on an actor".
  - *GetGold*: `[ActorID.]GetGold`, "Returns the actor's gold".

  The handlers never read `ctx.caller`. Each call reads or writes `ObScriptLedger.crime_gold`, which is the player's bounty, or counts the player's `Gold001`. The command table is asked before the CTDA-getter fallback (`ObScriptCommandHost::condition_getter`). So for id 0x1074, the row shadows the condition evaluator's arm, which does respect the Run-On subject (`ConditionFunction::GetCrimeGold => guards::bounty_of(world, entity)`, 0 for anyone but the player).
- **Evidence**: an Oblivion.esm `SCTX` census found these non-player callers:
  - `GlarthirRef.modcrimegold 5000` ×4, all MS38 "Paranoia". Three are `begin ondeath glarthirref` blocks on his victims' scripts. The fourth is the stage item "I showed Glarthir's note to a town guard…", whose result script reads `set ms38.arrestInProgress to 1 / modpcfame 1 / glarthirref.modav aggression 20 / glarthirref.modcrimegold 5000`.
  - `martinref.setcrimegold 0` ×2 and `OcatoRef.setcrimegold 0` (main quest).
  - `OrumGangCourier1Ref.getgold` and `DulfishgroOrumRef.getgold` (an Imperial City ambient package script).
- **Impact**:
  - Reporting Glarthir to the guard adds 5000 to the player's bounty. `crime.rs::rush_wanted_player` (bounty ≥ `rush_bounty`, 500) then force-greets every guard who detects the player, and `IsTurnArrest` and the arrest dialogue follow. `ObScriptLedger` is saved, so the bounty survives a save.
  - The main-quest result scripts wipe the player's real bounty.
  - The Orum courier's `goldCount` is the player's gold, and its `removeitem gold001 goldCount` acts on that number.
- **Related**: `GetCrimeGold` condition arm (`condition/eval.rs:647`); #6187 (the same handlers' saturation).
- **Suggested Fix**: Resolve the caller in each of the four handlers:
  - Player (or no caller): keep today's ledger / player-inventory behaviour.
  - Any other actor, `GetGold`: read that actor's own `Inventory`.
  - Any other actor, crime-gold trio: decline, since no NPC bounty is modelled. `bounty_of` already answers 0 on the condition side, and the two should agree.

  Add a regression test that runs `GlarthirRef.ModCrimeGold 5000` and checks the player's ledger is unchanged.

### SCR-2026-10-27-02: `SendTrespassAlarm` reports done with no effect
- **Severity**: LOW
- **Dimension**: Legacy ObScript
- **Location**: `crates/scripting/src/obscript_more/state.rs:159-164`, row `:659` (`Games::Both`)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The GECK says it "Sends out a trespass alarm as if someone had reported the player trespassing." The handler returns `done("SendTrespassAlarm")` and changes nothing. Its comment ("a trespass crime nobody records") predates `CrimeKnowledge` (1a67c9f30), which carries `CrimeKind::Trespass`, and the guard rush. Under the Dim 6 ledger-honesty rule (#6190), a command with no effect should decline and be counted, not report done.
- **Evidence**: FalloutNV.esm has six calls:
  - Gomorrah: `vGOMAlarmRef.SendTrespassAlarm Player`.
  - Ultra-Luxe: `VMS18MarjorieREF`, `BennyREF` and the cashier trigger, each on a drawn weapon.
  - The Atomic Wrangler trigger.
  - The Camp Golf score terminal.
- **Impact**: Casino security never reacts to these alarms. The `unknown_commands` census hides the gap.
- **Suggested Fix**: Return `Action::Declined` until trespass is modelled, then route it into the crime runtime.

### SCR-2026-10-27-03: an `as Bool` cast is transparent in the fragment lowering, so a Bool-cast numeric reading compared with a literal lowers as numeric equality
- **Severity**: LOW (decline-invariant hole; no vanilla occurrence found)
- **Dimension**: Recognizer Chain
- **Location**:
  - `crates/scripting/src/translate/effects/guards.rs:404-418` (`guard_reading`: a non-Int cast returns the inner reading).
  - `classify_value` (strips non-Int casts).
  - `crates/scripting/src/translate/compose.rs:138-150` (`as_num` strips casts off literals).
- **Status**: NEW (the residual of #6215, which made only `as Int` exact)
- **Untrusted-Input**: Yes (`.pex`-derived AST)
- **Description**: `(G.GetValue() as Bool) == True` classifies as `(Global G, Eq, 1.0)`, which means `G == 1`. Papyrus means `G != 0`. `(A.GetValue() as Bool) == (B.GetValue() as Bool)` becomes `Compare A == B`. Both diverge for any value other than 0 or 1. The bare atom `If G.GetValue() as Bool` is correct, because it becomes `Ne 0`. A literal cast like `GetStage() >= (15.5 as Int)` is likewise compared untruncated.
- **Evidence**: The FO4 `Scripts/Source` tree has 0 matches for `as bool) ==`. The Skyrim source is not on disk, so I could not check it.
- **Impact**: None on vanilla data I could check. A modded `.pex` with that shape would lower to a wrong guard instead of declining to the VM.
- **Suggested Fix**: In `classify_comparison` / `classify_value`, decline when a `Bool` (or `String`) cast wraps a numeric reading. Alternatively, fold it to `(reading, Ne, 0)` against the literal's truth. Have `as_num` refuse a cast literal whose cast changes the value.

## Cited, not re-filed (this run)

| Item | Owner |
|---|---|
| `apply_pending_group_swaps` / `sequence_player` AnimationPlayer↔PendingGroupSwap cycle (`obscript_anim.rs`) | AUDIT_ECS_2026-10-27 ECS-01 (= REG-04) |
| `player_controller_system` undeclared `PendingScriptStoryEvents` write via drowning, and `raise_story_event` missing from the hop table | ECS-05 / CONC-02 |
| `item_count` (GetItemCount CTDA 47) hand-written held-count sum | AUDIT_GAMEPLAY GAME-D1-2026-10-27-01 |
| Find-package `ActivateEvent` inserted after its consumers | GAME-D5-2026-10-27-01 |
| Script-started UseWeapon volley ignores incapacitation | GAME-D4-2026-10-27-01 |
| Scripted `PlayMusic` resolving to a folder | AUD-2026-10-27-D3-01 |
| `PendingEvidenceDeposits` not cleared on load | SAVE-2026-10-27-01 |
| `crime_gold.rs` / `scripting.md:809` say "the engine has no crime system" | AUDIT_TECH_DEBT_2026-10-25 |
| Known-open: #5764, #5586, #5802 | per skill |

## Dropped after verification

- **Jail commands half-applying on failure.** `pay_fine` / `go_to_jail` mutate state before `request_move`, but `request_move` can only fail when `PlayerMoveRequest` is absent, and `fragment/systems.rs:193` always installs it.
- **ObScript `StopQuest` uses `stop`, not `stop_with_shutdown`.** That is correct here: Oblivion/FO3/FNV author no shut-down stage and no quest aliases.
- **#6351 player OnInit key moved from local id 1 to 0x14.** A pre-7697b7cd7 save would re-run the player's provider `OnInit` once. This is one-time, so not filed.
- **Int script variables stored as f32 (#6216 residual).** They lose precision above 2^24. This is the pre-existing storage shape.
- **`IsRidingHorse`, `HasBeenEaten`, `IsLimbGone`, `IsPlayerGrabbedRef` answering false.** The engine cannot produce those states, so the answer is true, which is consistent with the 10-21 rule.

## Future-Phase Readiness

- The result-script runtime has one documented deviation: stage scripts run after the calling script, not inside the `SetStage` statement. The CS wiki *SetStage* page notes the original's nested-run semantics. If a mod reads back state that its own `SetStage`'s items write, this deviation becomes observable.
- The command table now makes a caller audit mechanical. Of 377 rows, 272 have `needs_caller: false`. Each row whose CS/GECK page shows `[ActorID.]` syntax should either use `caller(...)` or prove that the function is player-only. SCR-2026-10-27-01 is the first miss found.

Next: `/audit-publish docs/audits/AUDIT_SCRIPTING_2026-10-27.md` (labels `scripting`, `legacy-compat`, `game:oblivion` for 01; `game:fnv` for 02).
