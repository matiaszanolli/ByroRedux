# Scripting Subsystem Audit — 2026-09-06

Seventeenth full pass over the M30/M47 Papyrus / `.pex` / ECS scripting domain
(prior reports: `AUDIT_SCRIPTING_2026-06-23.md` … `_08-30.md`). Run as the skill
prescribes — **orchestrator + eight dimension agents, max three concurrent** —
each agent writing `/tmp/audit/scripting/dim_N.md` before consolidation. Every
CRITICAL/HIGH-class claim below was **independently re-verified by the
orchestrator** against the code (and, for the decompiler abort, re-run from the
agent's repro crate) before it was admitted; the per-finding "Disproof
attempted" lines are the agents', the confirmation notes are the orchestrator's.

**Scope**: `crates/pex`, `crates/papyrus`, `crates/scripting` (owner crates),
`crates/hkx` (Dim 8), and the engine-side attach / cinematic / cell-loader
wiring (Dims 7–8). **Explicitly out of scope and reported as a gap** (see
"Coverage gap" below): the SKSE/JContainers/StorageUtil/ObScript
compatibility layer that landed 2026-08-31→09-02 — `crates/scripting/src/
{papyrus_provider/, obscript.rs, obscript_runtime.rs, compatibility.rs}` and
`crates/sdk` (~23k LOC). Only its *seams* into this domain's existing surfaces
were audited, and three of this pass's HIGHs sit on exactly those seams.

**Method**: source reads and greps, `git diff 18a6bc94..HEAD` per path (the
2026-08-30 report's commit), `cargo check`/`cargo test` scoped per package, and
four empirical probes: a hand-built in-memory `Pex` driven through
`decompile_script` (Dim 3, re-run by the orchestrator), a wire-valid hostile
`.pex` timed through `parse()` vs `Pex::call_sites()` (Dim 1), an exhaustive
349 525-input differential harness for the #2668 bisect (Dim 4), and a
`.psc` depth-stress under a 1–8 MiB thread (Dim 4, disproof). No engine launch,
no game-data corpus run.

**Dedup baseline**: `gh issue list --limit 300` (132 OPEN issues, saved to
`/tmp/audit/scripting/issues.json`, cleaned up per Phase 4; CLOSED issues cited
below were individually verified with `gh issue view`),
`docs/audits/AUDIT_SCRIPTING_2026-08-30.md`, and `git log 18a6bc94..HEAD`.

## What changed since 2026-08-30

481 commits on `main`; in-domain, `56 files changed, 14 140 insertions, 545
deletions` under `crates/scripting` + engine wiring, and `7 files / +805` under
`crates/pex` + `crates/papyrus`. `crates/hkx`: **no change**.

| Commit(s) | Effect on this domain |
|---|---|
| `88e7dbfc` Fix #3783 | The previous pass's only HIGH (SCR-D2-2026-08-30-01, unbounded `Node` Clone/Drop → stack-overflow abort). Adds `lift::MAX_EXPR_DEPTH = 256` + `DecompileError::ExpressionTooDeep`, stops the boolean pass deep-cloning block scopes, case-insensitive auto-state match (#3786). **Fix verified present and correct for the shape it targets — and bypassed by a different well-formed shape (SCR-D3-2026-09-06-01).** |
| `bac0a76f` #3501 | `decompile/mod.rs` pass-order docstring now matches `decompile_body` |
| `f29c53cd` Fix #2668 | `OffsetMap::to_original` linear scan → `partition_point` bisect; the dead `removed` accumulator noted last pass is gone |
| `287f270f`, `217d9b62` | NEW `crates/pex/src/call_sites.rs` (333 LOC) — extender call-site preflight over a parsed `Pex`, wired into `translate_pex_detailed_with_providers` **ahead of** `decompile_script`, outside the panic net |
| `316fb202`..`fed3e550` (162 commits, 2026-08-31→09-02) | The SDK/extender layer: `papyrus_provider/` (6.2k), `obscript.rs` (686), `obscript_runtime.rs` (1.6k), `compatibility.rs` (984), `crates/sdk` (14.2k). Seams into this domain: `Effect::ProviderCall` (`effects.rs:258`), `classify_effect_with_providers` (`effects.rs:555`), the `*_with_providers`/`populate_owned_*` populate family and `DeferredFragmentEffects::apply_at_depth` provider barriers (`fragment.rs:549-700`), `translate_pex_detailed*` (`translate/mod.rs`), a second SCPT attach lane in `attach.rs`, and two `Stage::Late` exclusives in `boot.rs` |
| `962c9375` | `apply_effects`'s `Effect::Conditional` arm now recurses on `branch ++ tail` and `break`s (provider sequencing in branches) — **changes the recursion-depth premise the 08-30 pass relied on (SCR-D5-2026-09-06-02)** |
| `8ad3f7eb` Fix #3489 | `Effect::Enable` counterpart to `Disable` — verified a faithful mirror |
| `46cb7515` Fix #3785, `ae9d4194` Fix #3279 | `Effect::Conditional` unresolvable-guard decline; `MAX_CONDITIONAL_DEPTH = 256` — both verified in place |
| `d03f7a35` Fix #3739 | `build_scheduler` split into `register_{early,update,post_update,physics,late}_systems` — the scripting order now lives in `register_update_systems` (boot.rs ~852-1235) and `register_late_systems` (~1495+); verified unchanged relative order, but only the flush/quest-advance half is test-pinned (SCR-D6-2026-09-06-06) |
| `3f213038` Fix #3690, `90f81e8e` Fix #3254, `a3980338` Fix #3838 | Cinematic retention scan hoisted + `CellRoot` strip scoped to `retained ∩ victims`; persistent scratch in the trigger-approach router (Dim 8) |
| `26f8738d`/`265f0c9b` (#3277/#3278), `b28acb0c` (#3441) | Already verified last pass; re-verified still in place |

## Build & test state — CLEAN

```
$ export CARGO_BUILD_JOBS=4
$ cargo check -p byroredux-scripting -p byroredux-pex -p byroredux-papyrus -p byroredux-hkx -p byroredux-sdk --all-targets
    Finished `dev` profile in 8.92s

$ cargo test -p byroredux-pex -p byroredux-papyrus -p byroredux-hkx -p byroredux-scripting
   pex 63 passed (1 ignored: r5_fidelity, game-data) · papyrus 91 + 4 passed · hkx 18 passed (1 ignored)
   scripting 411 passed (4 ignored: pex_recognize_e2e ×1, extender_compat_e2e ×3, game-data gated)
   0 failed.
```

(Up from 334 scripting tests at the last pass; the +77 are almost entirely the
provider-seam and fragment-barrier guards.)

## Headline verdicts

### Untrusted-input robustness — **NOT MET: a well-formed `.pex` can still abort the process, and can stall cell load for minutes**

The previous pass's verdict ("NO PANIC, NO OOB, NO OOM … but YES, A PROCESS
ABORT") was answered by `88e7dbfc` (#3783) for the exact shape it reported —
and this pass shows the class is not closed:

1. **SCR-D3-2026-09-06-01 (HIGH)** — the new `MAX_EXPR_DEPTH = 256` ledger is
   re-initialised to `vec![1; len]` on every `rebuild_expression` call, and the
   control-flow and boolean passes each call it a *second* time over trees
   that are already folded. A `jmp +1` every 250 instructions, or a
   left-associative `&&` chain (the shape the compiler emits for a long
   conjunction), produces a **40 001-deep `Expr`** that `decompile_script`
   returns as `Ok` in release, and aborts release at the wire ceiling
   (63 251 instructions, `SIGABRT` exit 134). Orchestrator re-ran the repro:
   `single 1000` → `Err … nests deeper than 256` (the cap works within one
   block); `split 40000 250` → `Ok`, depth 40 001; `split 63000 252` → stack
   overflow abort. `catch_unwind` cannot intercept a stack overflow, so
   `translate_pex`'s panic net (#1816/#3287) is bypassed exactly as before.
2. **SCR-D1-2026-09-06-01 (MEDIUM)** — the new `Pex::call_sites()` preflight
   is O(F·D) (linear `find` over `debug_info.function_infos` once per
   function) and runs synchronously on the attach thread *before* decompile.
   Measured: `parse()` 14 ms vs `call_sites()` **11.6 s** on a wire-valid
   1.7 MB `.pex`. Bounded CPU, not a crash — hence MEDIUM, not the table's HIGH.
3. **SCR-D5-2026-09-06-02 (HIGH)** — at *dispatch*, `apply_effects` now
   recurses once per **sequential** `Effect::Conditional` (not per nested one),
   cloning the whole remaining fragment each time: O(N²) live `Effect` clones
   and N stack frames for a fragment with N sequential `If GetStageDone(..)`
   blocks. `MAX_CONDITIONAL_DEPTH` bounds nesting only; the `.pex` instruction
   ceiling bounds N at ~32k. This is the candidate the 08-30 pass dropped as
   "transitively bounded by lowering" — `962c9375` invalidated that premise.

A fifth HIGH is not on the untrusted surface but is a lock-ordering regression from a
2026-09-05 fix (SCR-D8-2026-09-06-01, `SceneRegistry` guard hoisted to function scope), and
a fourth is a pre-existing marker-ordering defect (SCR-D6-2026-09-06-01, SCEN package
`Activate` inserted after two of its consumers ran).

Everything else on the untrusted surface is sound and was re-verified from
code, not trust: the `.pex` reader (single `take` gate, 51 contiguous
`#[repr(u8)]` discriminants under a `>= MAX_OPCODE` guard, `Vec::new()`+push
var-args), the CFG builder, the lift/copy-prop pass (fail-closed >1-match, not
a debug-only assert — the checklist was stale there), the `.psc` lexer + Pratt
parser (balanced depth caps, single gated entry, correct bisect), and
`crates/hkx`. No memory-unsafety, no OOB, no unbounded allocation from a lying
count.

### The 99.996% decompile-rate claim — **measures what it claims**

`crates/pex/examples/pex_corpus_smoke.rs:177-218` calls `decompile_script`
inside `catch_unwind` and tallies `decompiled_ok` / `decompiled_err` /
`decompiled_panic` as three separate buckets; pct = `ok / (ok + err + panic)`
(`:266-268`). Unchanged since 08-30. Two caveats carried forward: (a) a
`SIGABRT` of the SCR-D3 class is not catchable here either — a vanilla-corpus
non-issue; (b) the harness exits non-zero only on *parse* failures, so it
cannot gate a decompile regression in CI (noted, `/audit-tech-debt`). One new
LOW: its `expected_top_level_item_count` still uses the pre-#3786
case-sensitive auto-state predicate (SCR-D3-2026-09-06-02).

### The `.psc`-vs-`.pex` fidelity gate — **both halves pin equality**

`recognizes_da10_and_reproduces_hand_builder`
(`translate/recognizers/quest_stage_gate.rs:428`, runs unconditionally, passes)
and `da10_pex_reproduces_hand_builder_byte_for_byte`
(`crates/scripting/tests/pex_recognize_e2e.rs:81`, `#[ignore]`-gated on Skyrim
SE data) both assert equality against `da10_main_door(...)`. The `.pex` half
still does not run in CI — known, not re-filed.

## Coverage gap — SDK / extender-compatibility layer (reported, NOT audited)

Per the skill's own scoping note this surface needs its own pass designed
with Dim-8-level rigor; it was **not** squeezed into Dims 5/6/7. What this
pass did do: inventory it, and audit the *seams* where the existing domain
now calls into it. Three of the four HIGHs and one MEDIUM live on those seams
(SCR-D5-01/02/03), which is itself the strongest argument for the dedicated
pass.

| Module | LOC | Purpose (module doc) | Untrusted input? | Registered / reached from | Tests |
|---|---|---|---|---|---|
| `crates/scripting/src/obscript.rs` | 686 | bounded structural decode of legacy compiled `SCDA` (xNVSE `ScriptAnalyzer` framing; xOBSE/xNVSE command tables) | **Yes** — `attach.rs:606` feeds `script.compiled` from the ESM | `attach_legacy_obscript_program` (`attach.rs:295`) | 6 |
| `crates/scripting/src/obscript_runtime.rs` | 1624 | conservative ECS runtime for load-order probes; "any other executable statement rejects its handler as a unit" | indirectly | `legacy_obscript_load_order_system`, `Stage::Late`, `add_exclusive_with_access` (`boot.rs:1791`) | 13 |
| `crates/scripting/src/compatibility.rs` | 984 | extender-call preflight over decoded PEX/SCDA/source calls → `CompatibilityReport` | decoded call sites only | `record_compatibility_report` (`asset_provider/script.rs:232/310/379`, `attach.rs:288/705`) | 7 |
| `crates/scripting/src/papyrus_provider/` | ~6.2k | front end (`catalog`/`lower_call`/`lower_program`, host-neutral) → `ir` → back end (`execute`, an interpreter against the live `World`) | decompiled AST | `papyrus_provider_system` (`Stage::Late`, `boot.rs:1804`); fragment seam `Effect::ProviderCall` → `DeferredFragmentEffects::provider_steps` | 38 |
| `crates/pex/src/call_sites.rs` | 333 | extender call-site scan over a parsed `Pex` | **Yes** | `analyze_pex_compatibility` ← `translate/mod.rs:152` | covered by Dim 1 this pass |
| `crates/sdk/` (26 files) | 14 154 | canonical engine-state exposure (actor values, equipment, plugins, factions, forms, UI, input, storage, legacy containers…) | via provider routes | `byroredux-sdk` dep of `byroredux-scripting`; `byroredux/src/extensions/` | 99 |

Spot observations for the future pass (not findings — no checklist applied):
- `obscript.rs::decode_extender_calls` applies the `checked_add` + `> len()`
  guard before every direct slice at the sites read (`:112-125`, `:201-218`,
  `:281-302`, `:155-160`); every `unwrap()`/`expect()` in `obscript.rs` and
  `obscript_runtime.rs` is below `#[cfg(test)]` (line 512 / 863); no `unsafe`,
  no `with_capacity(untrusted)`.
- `papyrus_provider/execute.rs:848-858` has three `unreachable!("validated …
  type")` arms in the back-end interpreter that trust the front end's typing —
  now that #3852 split them into separate files, a drift surfaces as a panic in
  `papyrus_provider_system`, and nothing catches it (SCR-D5-2026-09-06-07 is
  the same observation for the fragment seam).
- Lock shape: both Late exclusives have declared `Access` (`boot.rs:1791-1812`)
  and an ordering test (`boot.rs:2441-2442`); the declarations are
  documentation-only today and under-declared (SCR-D6-2026-09-06-04).
  `DeferredFragmentEffects::apply_at_depth` re-acquires `resource_2_mut::<
  QuestStageState, QuestObjectiveState>` per provider-step tail **after** the
  outer guards are released — the "guard-free" contract was traced at all four
  production call sites and holds (Dim 6).
- The SDK design docs exist (`docs/engine/sdk-v0.1-development-plan.md`, 1563
  lines; `sdk-v0.1-next-action-plan.md`) but neither `docs/feature-matrix.md`
  nor `ROADMAP.md`'s M47.2 row mentions the layer at all (SCR-ORCH-2026-09-06-01).

## Decompiler Soundness Matrix (Dims 1–4)

| Pass | Bounds-safe | Terminates | Total (no panic) | Fidelity-tested |
|------|:---:|:---:|:---:|:---:|
| Reader (`reader.rs`) | Yes | Yes | Yes | Yes |
| `call_sites.rs` (new preflight) | Yes | Yes — **but O(F·D), 11.6 s on 1.7 MB (SCR-D1-01)** | Yes | Fixture only |
| CFG (`cfg.rs`) | Yes | Yes | Yes | Yes |
| Lift + copy-prop (`lift.rs`) | Yes | Yes (#2024 linear chain) | Yes (#2666 fail-closed; `MAX_EXPR_DEPTH` within one block) | Yes |
| Boolean (`boolean.rs`) | Yes | Yes (`MAX_REBUILD_DEPTH = 1024`, graph strictly shrinks) | **No — re-fold at `:292` and `combine` at `:281` bypass the 256 cap (SCR-D3-01)** | Partly |
| Control-flow (`control_flow.rs`) | Yes | Yes, same cap | **No — whole-body re-fold at `:225` bypasses the cap (SCR-D3-01)**; `||`-skip fails closed (#1732) | Partly |
| Lower (`lower.rs`) | Yes | Yes | **No — `lower_expr` recursion is where the deep tree aborts** | Yes for straight-line/property/event shape |
| `.psc` lexer + parser | Yes | Yes | Yes (`MAX_EXPR_DEPTH`/`MAX_STMT_DEPTH = 256`, additive, fits 1 MiB at the workspace's `opt-level = 1`) | Yes |

Re-verified directly this pass: `MAX_OPCODE = 51`, `#[repr(u8)]`, guard `>=`,
`from_u8_round_trips_and_rejects_oob` iterates the whole range,
`metadata_matches_champollion_full_table` pins all 51 rows; every `create_node`
operand index `< arg_count()`; var-arg vec `Vec::new()`+push (#1710); `read_binary`
all-or-`Err`; `EVENT_NAMES` 267 entries strictly sorted with every
high-frequency event present; 47/47 keyword tokens `ignore(ascii_case)`;
`replace_constant_id`'s >1-match is a real `Err`, not a `debug_assert!`.

### The two documented Champollion departures — re-adjudicated

| Departure | Verdict |
|---|---|
| `boolean.rs`: no debug-line guard | **Benign as documented.** The one hand-writable false-merge shape (`Bool t = a ; If t ; t = b ; EndIf` → `t = a && b`) is semantically identical for a `Bool`, and an `Int`/property never reaches it (compiler inserts a `cast`/`propget` temp). A fidelity divergence, not a wrong-behaviour vector. |
| `control_flow.rs`: the `||`-skip | **Correct and fail-closed** (`return Err(self.fail())`, #1732; guard `conditional_predecessor_fails_closed` passes). The module-level sentence at `:27-29` still says "advanced past" — stale prose only. |

## Decline-Invariant Audit (Dim 5)

| Decline point | Verdict |
|---|---|
| Chain order `two_state_activator` → `rumble` → `quest_stage_gate`, `find_map` first-match | Conservative (`translate/mod.rs:49-55, 78`) |
| `classify_if_condition`'s per-atom `classify_guard_atom(..)?` | Conservative — a real `?` (`quest_stage_gate.rs:373`) |
| `split_and` leaving `||` whole | Conservative and intentional (`compose.rs:143-155`) |
| #1905 mixed-quest cross-check | Conservative (`quest_stage_gate.rs:387-391`) |
| `lower_statements`: `VarDecl`/`Assign` via `bind_local`, `Return(None)`, `_ => None` | Conservative |
| Narrow `While` exception (`lower_3d_loaded_wait`) | Conservative — OR-tree of `!Is3DLoaded()` + one `Utility.Wait` only |
| Narrow `If` exception (`Effect::Conditional`) | Conservative — no elseif, exact `StageDone{0.0\|1.0}`, cloned scopes, `has_latent`, `MAX_CONDITIONAL_DEPTH = 256` (#3279 verified) |
| `Effect::Conditional` dispatch — unresolvable guard declines whole Conditional + `warn!` (#3785) | Conservative — `resolved` flag, `continue` (`fragment.rs:1528-1548`) |
| `Effect::Conditional` dispatch — recursion bound | **NOT conservative — linear in sequential Conditionals since `962c9375` (SCR-D5-02)** |
| Hole binding `OwningQuest`/`SelfRef`/`Property`; `object_form_id` requires `alias == -1` (#2186) | Conservative |
| `receiver_object`: explicit `"self"` guard; three-map (`quest_locals`/`decl_locals`/`object_locals`) | Conservative (`effects.rs:1302-1343`) |
| `AddItem` literal-only `abSilent`, 4th arg declines; `MoveTo` 2-arg only | Conservative; **#3487 OPEN** (structural 100% real-corpus `MoveTo` decline) — cite, don't re-derive |
| `Effect::Enable` (#3489) mirrors `Disable` (alias-aware receiver, optional literal bool) | Conservative (`effects.rs:923-932` vs `:909-918`; shared dispatch arm `fragment.rs:951-983`) |
| `Effect::Disable` FormID-keyed sink + `spawn.rs` consumer (#3278) | Conservative; re-verified in place |
| `Effect::SetGlobalValue` strict `resolve_property_form_id`; `Globals` save-registered (`save_io.rs:476`) | Conservative |
| Multi-`Fragment_N`-per-stage merge: authored order, replace-once, idempotent | Conservative (`fragment.rs:2026-2067`) |
| rumble literal-only property extraction | Conservative |
| `two_state_activator` three-case `vmad_bool` | Conservative |
| **`recognize_specific_actor_trigger` (`quest_stage_gate.rs:148-169`, 2026-08-24)** | **Leaks a partial lowering — `unwrap_or(-1)` / `bool_property(.., false)` collapse "present but mistyped" onto the `.psc` default, the two-case collapse #2669 fixed in its sibling (SCR-D5-04, MEDIUM)** |
| **`classify_effect_with_providers` — provider catalog consulted BEFORE `EFFECT_PRIMITIVES`** | **Not sound at the seam: one strict manifest alias under `Self`/`Game`/`Utility` declines every fragment using `Self.SetStage`/`Game.*`/`Utility.Wait`; an exact-name alias silently REPLACES a canonical effect (SCR-D5-01, HIGH)** |
| **Provider barrier accepted at lowering vs. dispatcher with `callback: None`** | **Partial application — prefix runs, tail dropped (SCR-D5-03, MEDIUM)** |
| `translate_pex*` clean `None` on bad bytes / `Err` / panic, all three entry variants | Conservative for `decompile_script`; **`analyze_pex_compatibility` and `lower_provider_program` run outside the net (SCR-D5-07, LOW)** |
| `translate_pex` byte-identical without providers (empty catalog → `Ok(None)`) | Conservative |
| `CanonicalEvent::from_papyrus` → `Unknown` is "no consumer" | Vacuous — **no production caller** (SCR-D5-06, LOW) |

## Runtime Lifecycle Invariant Matrix (Dim 6)

| Invariant | Verdict | Evidence |
|---|---|---|
| Two-phase lock drop — `timer_tick_system` / `recurring_update_tick_system` / `trigger_detection_system` | Holds | `timer.rs:48`, `recurring_update.rs:168`, `trigger.rs:156-175 → :328` |
| `QuestStageFragments`/`SceneFragments` cloned before locks | Holds | `fragment.rs:2432`, `:2366` |
| **Guard-free contract**: `DeferredFragmentEffects::apply` never under a live `resource_2_mut::<QuestStageState, QuestObjectiveState>` | Holds at all 4 production sites | `fragment.rs:689-711`, `:666-680`, `:1693`, `commands/quest.rs:212-226`; the always-on same-thread reentrancy tracker (`lock_tracker.rs:9-12`) would panic otherwise |
| No ECS guard across the provider `callback` | Holds | owned `Arc` clone (`papyrus_provider/runtime.rs:37`); `execute.rs:247` / `obscript_runtime.rs:732` `drop(programs)` before host entry |
| `push_quest_stage_advances` sole `QuestStageAdvancedBatch` writer (#3277) | Holds | sole `insert` `quest_stages.rs:689`; six callers, no seventh |
| Marker drain coverage — Pattern A (17) / Pattern B (10) / persistent (23), none unassigned | Holds | `cleanup.rs:93-110`; new `OnInitEvent`, `EquipmentEventBatch` drained `:109-110`; contract test passes |
| **Producer scheduled before all consumers, drained once** | **BROKEN for the SCEN package `Activate` leaf** (SCR-D6-01, HIGH) | `package.rs:615-625` inserts `ActivateEvent` from `boot.rs:1060`, after `rumble` (`:1009`) and `quest_advance` (`:1025`) |
| Flush → same-frame consume → drain (`fragment_activation_flush_system`, #2654) | Holds for fragments | `boot.rs:1005-1025`, pinned `boot.rs:2388-2411` |
| Cascade FIFO, `is_cascade`-gated `MAX_CASCADE = 64`, WARN, genuine-transition-only | Holds | `fragment.rs:2444-2516` |
| `MAX_PROVIDER_FRAGMENT_BARRIERS = 64` | Bounds a recursion that cannot cycle; early return drops the level's non-provider deferreds (SCR-D6-05, LOW) | `fragment.rs:594-600` |
| CTDA OR-precedence + empty ⇒ true | Holds | `condition.rs:856-892` |
| Safe-default sentinels; `RunOn` declines | Holds | `condition.rs:473`, `:531` |
| #3441 / #3580 lock-cycle fixes | Holds | `condition.rs:470-493`, `trigger.rs:263-292/361-399`, `condition.rs:634-649` |
| Edge-trigger seed (`None` never pushes); multi-triggerer append; `intersects_sphere` only for tethered horses; occupancy `retain` | Holds | `trigger.rs:167-172`, `:330-343`, `:222-226`, `:310` |
| `actor_quest_trigger_is_in_sequence` | Holds; semantics unchanged (only #3580 scope reorder) | `trigger.rs:352-448` |
| `QuestAliasReadinessGate` three guards + `drop(bindings)`/`drop(registry)` | Holds | `quest_stages.rs:951-966` |
| Scheduling order `scene_playback → scene_fragment_dispatch → … → quest_fragment_dispatch → fragment_continuation`, cleanup LAST | Holds — but only the flush/quest-advance half is test-pinned (SCR-D6-06, LOW) | `boot.rs:1045, 1049, 1076, 1086, 1904` |
| Declared `Access` on the two Late exclusives vs actual acquisition | Documentation-only today; under-declared (SCR-D6-04, LOW) | `scheduler.rs:340-353, 511-512` |
| `ScriptRegistry` no live hardcoded attach (#2191) | Holds | `registry.rs:51-58` |

## Havok / Cinematic Slice (Dim 8)

| Invariant | Verdict | Evidence |
|---|---|---|
| `crates/hkx` untrusted-input discipline (no `unsafe`; `bone_count`/`transform_count`/`num_blocks`/`control_count` ≤ 4096, `degree` ≤ 8, `sample_count ≤ MAX_TRANSFORM_SAMPLES`; knot monotonicity; every slice pre-validated) | Holds; **no source change since 08-30** | `animation.rs:52, 74-82, 141-157, 196-204, 579-599, 725-727`; 18 tests |
| "No behavior-graph execution" | Holds | only `hkaSkeleton`/`hkaSplineCompressedAnimation`/`hkaAnimationBinding` resolved; no `hkb*` symbol in the crate |
| Static/dynamic split by file mask; track-count mismatch → `Err`, out-of-range bone → per-track warn+drop (#3013) | Holds | `animation.rs:206-216, 264-268, 503-560`; `asset_provider/animation.rs:172-202` |
| Z-up→Y-up exactly once; deterministic candidate order | Holds | `asset_provider/animation.rs:139-164, 208-214`; `crates/hkx` performs no conversion |
| Once-per-serial playback; apply-then-drain root motion; unknown annotation ignored; exit events synthesised | Holds | `systems/cinematic.rs:33-36, 74-79, 117-140, 206-216`; `asset_provider/animation.rs:300-317` |
| #3690 once-per-batch retention scan; #3254 strip scoped to `retained ∩ victims`; transitive `Children` walk | Holds | `unload.rs:34-47, 75-86, 137, 165, 215-236`; five guards green |
| #3838 scratch cleared per frame; closure `Send + Sync`; selection semantics unchanged | Holds | `cinematic.rs:429-430, 453-487, 575, 589` |
| **#3838 `SceneRegistry` guard lifetime** | **BROKEN — hoisted to function scope, inverts `ecs.md:658` order, cycles against `trigger.rs:344/369` (SCR-D8-01, HIGH)** | `cinematic.rs:460-681` |
| Router ↔ gate agreement | Same-quest: agree (confirms 08-30); cross-quest waits + centerless triggers diverge (SCR-D8-02, LOW) | `cinematic.rs:470-486, 561-573` vs `trigger.rs:369-440` |
| `HorseTetherState` lifetime | **#3817 OPEN**, cited not re-filed | zero removal sites |

`BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux` → 1923 passed at HEAD — green only because no `byroredux`-bin test drives the gate and the router in one process (see SCR-D8-01).

## Findings

**Five HIGH, four MEDIUM, fourteen LOW** (23 total, after cross-dimension
dedup — one LOW merged from Dims 5+6). No CRITICAL.

---

### HIGH

#### SCR-D3-2026-09-06-01: #3783's `MAX_EXPR_DEPTH` cap is per-`rebuild_expression`-call — the control-flow and boolean passes re-fold already-folded trees with a fresh `vec![1; len]` ledger, so a well-formed `.pex` still drives `lower_expr` to a stack-overflow `SIGABRT`
- **Severity**: HIGH (domain table: stack overflow via unbounded recursion in a decompiler tree walk; a `SIGABRT` bypasses `catch_unwind`)
- **Dimension**: Decompiler Control-Flow / Boolean / Lower
- **Untrusted-Input**: **Yes**
- **Location**: `crates/pex/src/decompile/lift.rs:401` (per-call ledger), `crates/pex/src/decompile/control_flow.rs:225` (whole-body re-fold), `crates/pex/src/decompile/boolean.rs:281-292` (`combine` + merged-scope re-fold), `crates/pex/src/decompile/lower.rs:88-120` (the recursion that aborts)
- **Status**: NEW (#3783 CLOSED — its fix is present and works for the single-block shape; this is the same failure class through a different, equally well-formed block shape)
- **Description**: `rebuild_expression` bounds nesting with a ledger initialised to `vec![1; len]` on the premise (`lift.rs:396-400`) that every freshly-lifted node has depth 1. True for the one call in `lift_function`, false for the two later ones: `Reconstructor::rebuild` splices every unconditional block's already-folded scope into `result` and re-folds (`control_flow.rs:225`) — a 256-deep tree from block *k* folds into block *k+1*'s 256-deep tree and the ledger records depth 2; `BoolPass::collapse` nests `left`/`right` under a new `BinaryOp` in `combine` with no depth check, then re-folds the merged scope with a fresh ledger, and the `reprocess` loop repeats once per `&&` link *iteratively*, so `MAX_REBUILD_DEPTH` never sees it. `lower_expr` recurses once per level.
- **Evidence** (Dim 3's scratchpad crate, re-run by the orchestrator on the 8 MB main thread; every count inside the wire format's `u16` ceilings):

  | Shape | Instructions | opt-level 0 | release |
  |---|---|---|---|
  | single block, N=1000 (#3783's own shape) | 1 002 | `Err: ExpressionTooDeep` | same — **the cap works within one block** |
  | `jmp +1` every 250 producers, N=1 000 | 1 005 | `Ok` | `Ok`, max `Expr` depth **1 001** |
  | same, N=20 000 | 20 081 | **SIGABRT** (exit 134) | `Ok`, depth **20 001** |
  | same, N=40 000 | 40 161 | **SIGABRT** | `Ok`, depth **40 001** (orchestrator-confirmed) |
  | same, N=63 000 | 63 251 | **SIGABRT** | **SIGABRT** (orchestrator-confirmed) |
  | left-assoc `&&` chain, 10 000 links | 20 002 | **SIGABRT** | `Ok`, depth 10 000 (orchestrator-confirmed) |

  `gdb`: every frame is `lower_expr` at `lower.rs:120`. Note the scratch crate's debug profile is `opt-level = 0`; the workspace `[profile.dev]` is `opt-level = 1` (`Cargo.toml:251`), so the workspace debug threshold lies between 20k and 63k instructions — the release abort at the ceiling and the 40 001-deep `Ok` tree are the load-bearing numbers.
- **Impact**: identical blast radius to #3783 — `.pex` from a `--scripts-bsa` archive reaches `decompile_script` via `translate_pex` and `populate_quest_fragments_from_pex`; one hostile/corrupt script kills the engine at cell load with no diagnosable error. Where release survives, a 20 000–40 000-deep `ast::Expr` reaches the recognizer chain, and `compose::split_and` (`compose.rs:143-155`, verified recursive on `And` by the orchestrator) recurses on exactly the `&&` shape — the abort moves downstream rather than disappearing. The #3783 commit's claim that the cap "also protects `lower_expr` and every downstream consumer" is not true as landed.
- **Disproof attempted**: boolean/reconstruct only move the tree (`take_scope`, `combine`), no cap; `MAX_REBUILD_DEPTH` bounds recursion into an operand range (depth 1 per link, returns), not the iterative `reprocess` chain; #3783's own shape confirmed still capped; `jmp +1`/`jmpf` sequences pass `checked_target` with no `DecompileError`; aborting frame confirmed via `gdb`. Dim 2 had reasoned the caps compose to "low thousands" — the empirical run refutes that reasoning.
- **Related**: #3783 (CLOSED; incomplete), #1816/#3287 (the net this bypasses), #2667, Dim 5 `split_and`
- **Suggested Fix**: make depth a property of the tree, not of one call — carry a memoised `depth` on `Node` (constructors set `1 + max(children)`; `replace_constant_id` and `combine` update it), seed `rebuild_expression`'s ledger from `scope[i].depth`, and check the cap in `combine` too. Give `lower_expr` its own defensive depth counter returning `ExpressionTooDeep`. Regression guards: `jmp +1`-split shape and `&&`-chain shape at a few thousand, both asserting a clean `Err`.

#### SCR-D5-2026-09-06-01: the provider catalog is consulted before the canonical effect-primitive table, so one manifest alias under `Self`/`Game`/`Utility` declines every fragment that uses `Self.SetStage`/`Game.*`/`Utility.Wait`, and an exact-name alias silently replaces the canonical effect
- **Severity**: HIGH (silent, all-content blast radius under a realistic enabling condition — one installed extension; the substitution leg is a wrong lowering, the domain table's HIGH row)
- **Dimension**: Recognizer-Chain Soundness
- **Untrusted-Input**: No (enabling condition is an installed extension manifest; affected content is every vanilla QF/SF fragment)
- **Location**: `crates/scripting/src/translate/effects.rs:555-574` (`classify_effect_with_providers`: `lower_provider_call` first, `Err(_) => return None`, then `classify_effect`); `crates/scripting/src/papyrus_provider/lower_call.rs:107-115` (`resolve` miss + `is_known_provider_call` → `Err(UnknownFunction)`), `:302-308` (`contains_provider(provider) || classify_static_call(..)`); `crates/scripting/src/papyrus_provider/catalog.rs:83-111` (`insert_route(.., strict_provider = true)` records the provider name), `:119-121`; `byroredux/src/extensions.rs:444-451` (every manifest alias inserted strictly)
- **Status**: NEW
- **Description**: When `catalog.resolve(ident, method)` misses, `is_known_provider_call` returns true if `catalog.contains_provider(ident)` — true for *any* strictly-inserted manifest alias whose provider is `ident` — and `lower_provider_call` returns `Err`, which `effects.rs:570` turns into a whole-fragment decline. (1) **Wholesale decline**: the SDK prescribes `Self` as the reserved provider for receiver-method aliases (`papyrus_provider/mod.rs:7, 67`; `docs/engine/sdk-v0.1-development-plan.md:1528`); one installed extension with one instance method makes `contains_provider("self")` true, and `Self.SetStage(10)` — the most common fragment statement — declines every fragment containing it. Same for `Game` (kills the MQ101 cart primitives) or `Utility` (kills `Utility.Wait`). `PapyrusFunctionAlias::is_valid` checks identifier syntax only; `insert_route` validates the declaration and rejects duplicates only. (2) **Silent substitution**: an alias exactly spelling `Utility.Wait` / `Game.SetPlayerAIDriven` wins over the canonical primitive and lowers to a deferred host barrier; for `Utility.Wait` this also defeats `has_latent` (`effects.rs:448-458` matches only `Effect::Wait`).
- **Evidence**: orchestrator re-read `effects.rs:555-574`, `lower_call.rs:94-128, 302-308`, `catalog.rs:83-125`, `extensions.rs:444-451` — the ordering, the `contains_provider` predicate, the strict insert, and the absence of a reserved-provider rejection are all as described.
- **Impact**: With one ordinary extension active, the whole quest-stage fragment population (742 lowered fragments on vanilla Skyrim) goes inert at cell load with only `debug!` output; the substitution leg changes vanilla semantics without declining.
- **Disproof attempted**: the no-extension path is unaffected — `engine_compatibility()` inserts non-strictly so `contains_provider` is empty, `classify_static_call` lists neither `Utility` nor the `Game.*` control functions, and `provider_aware_fragment_population_resumes_after_native_call` passes with `Utility.Wait` + `Game.GetModCount` + `Self.SetStage`. The defect needs a strict manifest alias — reachable and, for `Self`, prescribed. No test combines a strict `Self.*`/`Game.*`/`Utility.*` alias with a canonical primitive.
- **Related**: #3159; the SDK coverage-gap note
- **Suggested Fix**: consult `EFFECT_PRIMITIVES` first and hand only unclaimed statements to `lower_provider_call`; reject manifest aliases whose provider is a Papyrus-native receiver/static (`Self`, `Game`, `Utility`, `Quest`, `ObjectReference`, `Actor`, `Debug`, …) at `PapyrusProviderCatalog::insert`. Guard: strict `Self.Touch` alias + `Self.SetStage(10)` must still lower to `Effect::SetStage`.

#### SCR-D5-2026-09-06-02: `apply_effects` now recurses on `branch ++ tail` for every `Effect::Conditional`, so dispatch recursion depth is linear in the number of *sequential* Conditionals (O(N²) live clones) — `MAX_CONDITIONAL_DEPTH` bounds nesting only
- **Severity**: HIGH (domain table: unbounded recursion / allocation reachable from untrusted `.pex`; bounded only by the `u16` instruction count)
- **Dimension**: Recognizer-Chain Soundness (the checklist's Conditional-dispatch bullet, item (d)); dispatch-time code owned by Dim 6 — reported once here
- **Untrusted-Input**: **Yes**
- **Location**: `crates/scripting/src/fragment.rs:1549-1562`; introduced by `962c9375` (2026-09-01)
- **Status**: NEW (regression of the premise the 2026-08-30 pass used to drop this candidate — "bounded transitively by whatever `lower_statements` produced")
- **Description**: Before `962c9375` the arm did `apply_effects(branch)` + `continue`, so recursion depth equalled nesting depth (capped at 256 by #3279). Now it builds `ordered_tail = branch ++ effects[index+1..]` (`Vec::with_capacity` + two `extend_from_slice`), recurses, and `break`s — each *sequential* Conditional adds one frame *and* one `Vec` that stays live until unwind: Σ(N−k) ≈ N²/2 `Effect` clones and N frames. `lower_statements` bounds statement *nesting*, not *count*; the `.pex` reader allows 65 535 instructions per function, so N is tens of thousands — hundreds of millions of live `Effect` clones and tens of thousands of frames when the quest reaches the bound stage. The `.psc` frontend has no sequential bound at all.
- **Evidence**: orchestrator re-read `fragment.rs:1510-1568` — `ordered_tail.extend_from_slice(branch); ordered_tail.extend_from_slice(&effects[index + 1..]); advances.extend(apply_effects(&ordered_tail, ..)); break;`.
- **Impact**: hostile/pathological mod content aborts (or OOMs) the engine at *dispatch* time — later and less diagnosable than a load-time failure; vanilla unaffected; benign long fragments pay O(N²) per dispatch.
- **Disproof attempted**: no iterative worklist exists; `break` confirms one frame per Conditional; `lower_statements` is a flat loop over statements; the 08-30 drop relied on the pre-`962c9375` shape.
- **Related**: #3279 (CLOSED); 2026-08-30 report "Stale candidates dropped" #3
- **Suggested Fix**: iterative `apply_effects` over an explicit stack of `(slice, index)` cursors; materialise a tail only at a `ProviderCall`/suspension. Add a ~10k sequential-Conditional AST test.

#### SCR-D6-2026-09-06-01: `scene_package_system`'s Package `Activate` leaf inserts `ActivateEvent` after two of its four consumers have already run — the #2654 class, unpatched for the package producer
- **Severity**: HIGH (domain table: transient marker drained out of stage order)
- **Dimension**: Scripting Runtime Systems
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/package.rs:615-625` (producer); `byroredux/src/boot.rs:1009, 1025, 1060, 1065, 1144, 1904` (schedule)
- **Status**: NEW (predates the last pass — `583a349a`, present at `18a6bc94` — but no `AUDIT_SCRIPTING_*.md` adjudicated it and no open issue matches; #2654, the fragment-side twin, is CLOSED)
- **Description**: `tick_command`'s `TimedInteraction { procedure_type == "Activate" }` arm does `events.insert(target, ActivateEvent { activator: action.actor })` directly. `scene_package_system` is registered at `boot.rs:1060`; `ActivateEvent` is Pattern-A, drained by `event_cleanup_system` at `Stage::Late`. Of the four Update-stage consumers, `rumble_on_activate_dispatch` (`:1009`) and `quest_advance_dispatch` (`:1025`) run *before* the producer and never see the marker; only `two_state_activator_system` (`:1065`) and `mg07_on_activate_dispatch` (`:1144`) do. The registration comment considers only the two-state consumer. This is the exact ordering defect #2654 fixed for fragment `Activate` by introducing `PendingFragmentActivations` + the head-of-frame flush — the fix exists in the same crate and was not applied to this second producer.
- **Evidence**: orchestrator re-read `package.rs:612-628` and the `boot.rs` registration lines; `quest_advance_system` reads `ActivateEvent` once per frame (`quest_advance.rs:348-352`) and its `ActivatorGate::Any` (default) / `BaseForm(u32)` accept a scene actor as activator, so a scene-authored NPC `Activate` on a quest-advance REFR is a modelled input that is unreachable from this producer.
- **Impact**: a scene whose package `Activate` targets a REFR carrying a recognised quest-advance script silently never advances the quest; the marker is consumed by the two-state system and drained the same frame. No log, no fallback. Corpus reachability of this exact shape was not measured.
- **Disproof attempted**: `quest_advance_system` does not re-scan later in the frame; the marker does not survive to the next frame (`drain_component::<ActivateEvent>` at `cleanup.rs:93`, cleanup last); `ActivatorGate` does not reject NPC activators; no prior report adjudicated it; the producer cannot simply move before `quest_advance` because it consumes `ScenePackageEventBatch` from `scene_playback_system`, which must follow the quest-start batch.
- **Related**: #2654 (CLOSED); `boot.rs:2388-2411` order test
- **Suggested Fix**: route the package `Activate` through the existing `PendingFragmentActivations` queue (expose a `pub(crate) fn push(&mut self, target, activator)`) so it is delivered at the next frame's head flush ahead of every consumer, exactly as fragments are; extend the `boot.rs` order test's producer list.


#### SCR-D8-2026-09-06-01: #3838 hoisted the `SceneRegistry` read guard to function scope, so `scene_trigger_actor_approach_system_inner` now inverts the canonical scene/quest lock order and closes a cycle against `actor_quest_trigger_is_in_sequence`
- **Severity**: HIGH (latent — see Impact; `_audit-severity.md` "ECS deadlock potential" and the domain table's "ECS lock held across a second resource/component mutation")
- **Dimension**: Havok Idle / Cinematic Slice
- **Untrusted-Input**: No
- **Location**: `byroredux/src/systems/cinematic.rs:460-681` (introduced by `a3980338`, Fix #3838, 2026-09-05); counter-edge at `crates/scripting/src/trigger.rs:341-369`
- **Status**: NEW
- **Description**: Before #3838 the `SceneRegistry` guard lived inside the block that computed `(awaited, between_scenes)` and was dropped when that block ended (`18a6bc94:cinematic.rs:415`, inside `let (awaited, between_scenes) = { … }`). The scratch rework replaced the block with straight-line `clear()`/`extend()` calls and bound the guard at function scope: `let Some(registry) = world.try_resource::<SceneRegistry>() else { return; };` (`:460`). Rust drops a guard at end of scope, not last use, so `registry` is now alive to `:681` — across `world.query::<QuestAdvanceOnActivate>()` (`:500`), `TriggerVolume` (`:501`), `QuestStageState` (`:502`), `QuestTriggerApproachRegistry` (`:503-505`), `evaluate_condition_list` (`:557`, which re-reads `SceneRegistry`/`ScenePlayer` in its `IsSceneActionComplete` arm), `SceneAliasCandidate`/`RemoteSceneActorStub`/`Transform` reads (`:601-616`), a `Transform` **write** (`:651`), `set_kinematic_translation` → `PhysicsWorld` write (`:660`), and the `OnTriggerEnterEvent` write (`:664`). Every one records a `SceneRegistry → X` edge in the lock tracker. `docs/engine/ecs.md:658` pins the canonical order as `QuestAdvanceOnActivate → ScenePlayer → QuestStageState → SceneRegistry`, and `:668-675` explains that #3580 specifically required the registry guard to be *dropped* before `QuestStageState` in the sibling gate. That gate, `actor_quest_trigger_is_in_sequence`, still holds `QuestAdvanceOnActivate` (`trigger.rs:344`) and `ScenePlayer` (`:358`) while acquiring `SceneRegistry` (`:369`) — unconditionally, on every BaseForm-gated trigger entry. Together: `QuestAdvanceOnActivate → SceneRegistry → QuestAdvanceOnActivate`.
- **Evidence**: orchestrator confirmed `:460` at function scope, `grep drop(registry)` → none, function closes at `:681`, `QuestAdvanceOnActivate` acquired at `:500`; `trigger.rs:344/369` counter-edge; `ecs.md:658` canonical order; pre-#3838 block scoping at `18a6bc94:415`.
- **Impact**: No deadlock today — both systems are `add_exclusive` (`boot.rs:1017`, `:1023`), the "circumstantial" safety `ecs.md:693-696` warns about, one `add_to_with_access` promotion away from a real ABBA. What does fire: with `BYRO_LOCK_ORDER_CHECK=1` in a debug build, the detector panics the process (`lock_tracker.rs:272-290`) the first time the MQ101 cart sequence both runs the approach system and has the horse enter a resident BaseForm trigger. **Not CI-red at HEAD**: `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux` → 1923 passed, because the gate lives in `crates/scripting`'s test binary and no `byroredux`-bin test drives both in one process. The eight new `SceneRegistry → X` edges (incl. two writes and `PhysicsWorld`) each widen the surface for further cycles.
- **Disproof attempted**: no `drop(registry)` or re-scoping block; `try_resource` is lock-tracked (`world.rs:718`); the gate's `advances` guard is used again at `trigger.rs:420` so it is alive at `:369`; ran the full binary suite under the detector (green — and explained why); confirmed pre-#3838 code recorded no edge out of `SceneRegistry`; `a3980338`'s message discusses only the `ScenePlayer`-before-`SceneRegistry` clone rationale — the widening was unintentional.
- **Related**: #3838 (CLOSED — introducing fix), #3580 (CLOSED — same pair fixed in the sibling gate), #3651 (canonical-order doc), #3446 (CLOSED — source-scan guard pattern reusable here). Cross-reference Dim 6 (`trigger.rs`), `/audit-concurrency` Dim 3.
- **Suggested Fix**: end the guard where the old block did — wrap `:460-497` in a block that owns `registry`, or `drop(registry);` right after `between_scenes.extend(…)` at `:497`. Add a `byroredux`-side test that enables the lock tracker, installs a BaseForm trigger + running `ScenePlayer` on one `World`, and runs `trigger_detection_system` then the approach closure — must not panic; or a source-scan test asserting the registry guard's scope closes before the `QuestAdvanceOnActivate` query. Update the `SceneTriggerApproachScratch` doc (`:415-418`), which records only the `ScenePlayer` half of the rationale.

---

### MEDIUM

#### SCR-D1-2026-09-06-01: `Pex::call_sites()` re-scans the whole debug-info table once per function (O(F·D)) and now runs synchronously on the cell-load attach path
- **Severity**: MEDIUM (bounded-CPU hardening gap; cannot panic, OOB, or over-allocate — hence not the domain table's HIGH)
- **Dimension**: PEX Reader & Opcode Decode
- **Untrusted-Input**: **Yes**
- **Location**: `crates/pex/src/call_sites.rs:94` (`debug_lines(pex, &object.name, &scope)` once per function, before any call instruction is found) and `:172-190` (`function_infos.iter().find(..)`, four `eq_ignore_ascii_case` per candidate); consumer seam `crates/scripting/src/translate/mod.rs:152` (`analyze_pex_compatibility(&pex)` before `decompile_catching_panics`), reached from `byroredux/src/cell_loader/references/attach.rs:698` and `byroredux/src/asset_provider/script.rs:373`
- **Status**: NEW (module postdates the 2026-08-30 report)
- **Description**: both dimensions are attacker-controlled and independently `u16`-bounded per container (`function_infos` ≤ 65 535 at 9 bytes each; functions bounded only by file size at 17 bytes each). The reader is linear in file size; this pass is quadratic, executes for every scripted REFR / quest `.pex` at cell load, on the loader's thread, with no budget and no catch.
- **Evidence** (Dim 1's scratchpad harness, release, single thread; F functions of 0 instructions, D = 65 535 debug entries matching object+state but not function name):

  | F | file bytes | `parse()` | `call_sites()` |
  |---:|---:|---:|---:|
  | 60 (vanilla-shaped, D=60) | 1 670 | 22 µs | 15 µs |
  | 4 096 | 659 557 | 6.5 ms | 0.69 s |
  | 16 384 | 868 453 | 7.5 ms | 2.83 s |
  | 65 535 | 1 704 020 | 14.2 ms | **11.63 s** |

  Orchestrator confirmed the call path (`translate/mod.rs:152` precedes the decompile closure) and the per-function linear `find` shape.
- **Impact**: CPU denial-of-service at cell load from a `.pex` that passes every reader check; a second state doubles it. Vanilla/normal mod content is unaffected (15 µs at F=D=60).
- **Disproof attempted**: not memoised; not deferred until a call opcode is seen; attach path is synchronous (no spawn/thread/rayon in `attach.rs`); Champollion's `getFunctionInfo` is also a linear `find_if` but runs offline on one file — the port moved it onto a per-script load path.
- **Related**: #1710 (same "attacker-controlled count" class, different resource); #3783
- **Suggested Fix**: build one `HashMap<(object, state, function, FunctionType), &[u16]>` from `function_infos` (O(D)) and look functions up in O(1); or at minimum defer `debug_lines` until a `Call*` opcode is seen and cache per function. Regression test: F=D=65 535 completes well under a second.

#### SCR-D5-2026-09-06-03: lowering accepts provider barriers against a catalog the dispatcher may be unable to serve — `PapyrusProviderRuntime::default()` pairs a non-empty `engine_compatibility()` catalog with `callback: None`, a tolerated startup error leaves it that way, and accepted fragments run their prefix and drop their tail
- **Severity**: MEDIUM
- **Dimension**: Recognizer-Chain Soundness (lowering/dispatch consistency)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/papyrus_provider/runtime.rs:18-28`; `crates/scripting/src/fragment.rs:641-658`; `byroredux/src/extensions.rs:4892-5000` (13 `?` exits before `sync_extension_script_function_invoker` at `:5000`); `byroredux/src/main.rs:704-709`; `byroredux/src/asset_provider/script.rs:176-178, 258-260`
- **Status**: NEW
- **Description**: `Default` publishes a non-empty catalog with no callback; `populate_*_fragments` lower against `runtime.catalog()` at cell load, so `Game.GetModCount()`/`StorageUtil.*`/`Input.*`/`UI.*` calls become barriers instead of declining. At dispatch `apply_at_depth` finds no callback and drops every barrier *and its tail* after the prefix already mutated quest state. Reachable: `load_requested_extensions` exits through 13 `?` sites before ever syncing the runtime, and `App::new` logs and continues. Also by design (`failed_provider_barrier_aborts_its_native_fragment_tail`) a host `Err` aborts the tail including `SetStage` — divergent from Papyrus, where a native call never halts the script.
- **Impact**: `Game.GetModByName("X.esp"); Self.SetStage(20)` advances nothing past the barrier and never declines — quest stuck mid-fragment with a `warn!`. Pre-seam the same fragment declined wholesale (inert, consistent).
- **Disproof attempted**: the healthy path is consistent (`ExtensionHost::new` seeds `engine_compatibility()`; `sync_` publishes catalog+callback together); host-init failure publishes an *empty* catalog — consistent. Only the early-return path and the `Default` are inconsistent; no test covers callback-`None` with a non-empty catalog.
- **Related**: SCR-D5-01, SCR-D6-05
- **Suggested Fix**: make `Default` publish an empty catalog (or lower with `None` providers when no callback is live) so barrier-needing fragments decline at the boundary; publish the empty state on every `load_requested_extensions` error exit; consider skip-one-call semantics on host `Err`.

#### SCR-D5-2026-09-06-04: `recognize_specific_actor_trigger` collapses a present-but-wrong-typed VMAD `prereqStageOPT`/`disableWhenDone`/`onlyOnce` into the `.psc` default — the two-case collapse #2669 fixed in its sibling — so a mistyped prerequisite drops the gate
- **Severity**: MEDIUM (exposure limited to corrupt / hand-edited plugins — the CK always writes matching type tags)
- **Dimension**: Recognizer-Chain Soundness
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/recognizers/quest_stage_gate.rs:148-169, 191-192`; landed `7473a387` (2026-08-24), never in the checklist
- **Status**: NEW
- **Description**: `int_property("stage")?`/`object_property(..)?` decline correctly, but `int_property("prereqStageOPT").unwrap_or(-1)` maps "present but not `Int32`" onto "no prerequisite" and `bool_property(name, false)` maps "present but not `Bool`" onto `false` — the collapse the crate's own three-case contract (`two_state_activator.rs:71-89`, `effects.rs:1353-1368`) forbids. The wrong state is a missing `GetStageDone(prereq) == 1` condition on a `QuestAdvanceOnActivate`: the trigger fires without its prerequisite / re-fires every entry.
- **Disproof attempted**: CK type discipline bounds exposure but does not remove the contract violation; sibling fixes (#2669, #2023, #1909) were filed for the same shape and exposure.
- **Related**: #2669, #2023, #1909 (all CLOSED)
- **Suggested Fix**: three-case `Option<Option<T>>` closures with `?` on the outer `None`, mirroring `vmad_bool`; add `declines_specific_actor_trigger_on_mistyped_prereq`.

#### SCR-D7-2026-09-06-01: the legacy `SCRI` accessor has no statics-family arm — 560 scripted DOOR/FURN/LIGH/TACT/FLOR base records on Oblivion/FO3/FNV never reach the new ObScript attach lane or the extender-compatibility census
- **Severity**: MEDIUM (the lowerer would decline most vanilla door bodies today; but the compatibility report doesn't decline — it never runs)
- **Dimension**: Engine Attach & Trigger Wiring
- **Untrusted-Input**: No
- **Location**: `crates/plugin/src/esm/records/index.rs:734-767` (`base_record_script`: ACTI → CONT → TERM → items → NPC_ → CREA, first hit — **no `cells.statics` arm**; its own doc at `:722-727` lists this as a "coverage gap to close later", `a459f149`, 2026-05-23); `crates/plugin/src/esm/records/dispatch_world_placement.rs:18-28` (DOOR/FURN/LIGH/TACT/FLOR → `cells.statics` only); `crates/plugin/src/esm/cell/support.rs:60-98` (builder captures `VMAD`, no `SCRI` arm); `crates/plugin/src/esm/cell/mod.rs:805-845` (`StaticObject` has no `script_form_id`). Consumers: `byroredux/src/cell_loader/references/attach.rs:240`, `synth_child.rs:193`.
- **Status**: NEW. #2663 (CLOSED) fixed the **VMAD** half of exactly this family; #521 / #1273 closed ACTI/TERM and NPC_/CREA. No issue exists for the `SCRI`-on-statics half (`gh issue list --search` on `SCRI` / `DOOR script` / `base_record_script`, all states: no match).
- **Description**: `base_record_script` is the only way a REFR reaches the legacy SCPT lane. Until this pass the lane's only consumer was one demo spawner; in-range commits `19050cd9`, `9d5829b8`, `7126aa0a` gave `attach_scpt_script` two real consumers — `record_compatibility_report` (`attach.rs:281-293`) and `attach_legacy_obscript_program` (`:294-295`) — both silently skipped for this family on three games (`return false` at `attach.rs:241`, no log at any level).
- **Evidence** (Dim 7's raw sub-record census of the installed masters — uncompressed records, non-zero `SCRI`; 20-byte headers Oblivion, 24 FO3/FNV):

  | Game | DOOR | FURN | LIGH | TACT | FLOR | **unreachable** | ACTI (reachable, for scale) |
  |---|---|---|---|---|---|---|---|
  | FalloutNV.esm | 136/320 | 16/234 | 4/501 | 18/87 | 0 | **174** | 992/1143 |
  | Fallout3.esm | 117/319 | 8/183 | 3/368 | 22/49 | 0 | **150** | 616/774 |
  | Oblivion.esm | 180/501 | 9/186 | 42/1625 | — | 5/155 | **236** | 927/1252 |

  Orchestrator confirmed the six-arm walk and the absence of any `SCRI`/`script_form_id` capture in `cell/mod.rs` / `cell/support.rs`.
- **Impact**: (1) the `CompatibilityRegistry` aggregate (`commands/world_info.rs:173`) under-reports on every Fallout/Oblivion cell — an xNVSE/OBSE probe in a door/talking-activator script is never seen; (2) a pure load-order handler on a door — the shape `pure_load_order_handler_attaches_without_a_hand_written_spawner` proves lowers — never lowers on 560 base records; (3) every future widening of the ObScript lane inherits the hole. No command-line workaround; the field is dropped at parse time.
- **Disproof attempted**: TACT is not dual-dispatched into `activators`; no `SCRI` anywhere in the cell builder; `LegacyObscriptContentCatalog` is plugin metadata, not a script census; census counted non-zero payloads only and found zero compressed records for the relevant types; ACTI column consistent with #521.
- **Related**: #2663, #521, #1273, #3160
- **Suggested Fix**: add `script_form_id: u32` to `StaticObject`, capture `SCRI` (remapped like the `VMAD` arm), append a `cells.statics` arm to `base_record_script` placed last (mirroring #2663's ordering rationale) with a resolves/declines test pair; replace the `index.rs:722-727` comment with the issue number.

---

### LOW

#### SCR-D1-2026-09-06-02: `rejects_truncation` pins one truncation offset; the "take is the single bounds gate" invariant is re-verified by hand every cycle instead of by an exhaustive-prefix test
- **Dimension**: PEX Reader & Opcode Decode · **Untrusted-Input**: Yes · **Location**: `crates/pex/src/lib.rs:773-779` · **Status**: NEW (test-coverage gap; code is correct)
- **Description**: the four wire-valid builders already in the test module (`build_sample`, `_skyrim_be`, `_starfield_with_guards`, `build_extender_dependent_skyrim_be`) truncated at every prefix `0..len` with `assert!(parse(..).is_err())` would mechanically pin the gate across all three dialects and the debug-info / skip / var-arg paths the current single-offset test never reaches.
- **Suggested Fix**: one `#[test]` looping the cut over the four builders (≈ 4 × 300 iterations).

#### SCR-D3-2026-09-06-02: the smoke harness's `expected_top_level_item_count` still mirrors the pre-#3786 case-SENSITIVE auto-state rule, so the #3017 shape check now disagrees with `decompile_script` on the very input #3786 fixed
- **Dimension**: Decompiler Control-Flow / Boolean / Lower · **Untrusted-Input**: Yes (false harness report, not a crash) · **Location**: `crates/pex/examples/pex_corpus_smoke.rs:95` vs `crates/pex/src/decompile/lower.rs:424` · **Status**: NEW
- **Description**: `88e7dbfc` changed the auto-state match to `eq_ignore_ascii_case`; the harness predicate is still `==`. A mismatched-casing auto state would report a spurious `decompiled_shape_mismatch` and send triage at the decompiler. Latent — none observed in the vanilla corpus.
- **Suggested Fix**: expose `pub fn is_auto_state(object, state) -> bool` from `lower.rs` and call it from both.

#### SCR-D4-2026-09-06-01: the #2668 bisect is correct under duplicate `pp_off` entries, but nothing pins that — the one case where `partition_point` vs `binary_search` differ is untested and the stated invariant ("strictly increasing") is wrong
- **Dimension**: Papyrus Lexer & Pratt Parser · **Untrusted-Input**: Yes (diagnostic offsets only) · **Location**: `crates/papyrus/src/lexer.rs:52-57, 59-74, 154-183` · **Status**: NEW (#2668 CLOSED; follow-up on its fix)
- **Description**: two back-to-back continuations (`"a\\\n\\\nb"`) yield `entries = [(1, 2), (1, 4)]` — non-decreasing, not strictly increasing as the commit body, `ISSUE.md`, and docstring claim. `partition_point(pp_off <= p)` + `idx − 1` picks the *last* duplicate (largest cumulative `removed`), matching the old scan — but `binary_search_by_key`'s index under duplicates is unspecified, and the regression test's fixture (`pp_off ∈ {2,4,6}`) has no duplicates. Dim 4's differential harness: 349 525 inputs, 34 134 maps with adjacent duplicates, 0 mismatches — correct today, unpinned.
- **Suggested Fix**: add a `\\\n\\\n` case, a leading-continuation (`pp_off = 0`) case, a mixed CRLF/lone-CR map, and `to_original(out.len()) == source.len()`; reword the docstring to "non-decreasing … do not replace with `binary_search`".

#### SCR-D4-2026-09-06-02: the "aligned" depth caps in `lift.rs` and `effects.rs` are hand-copies of `pub(crate)` papyrus constants — the alignment their docstrings promise is unenforced
- **Dimension**: Papyrus Lexer & Pratt Parser (consumers in Dims 2/5) · **Untrusted-Input**: Yes (consistency, not safety) · **Location**: `crates/papyrus/src/parser/expr.rs:19`, `stmt.rs:38` (`pub(crate)`); copies at `crates/pex/src/decompile/lift.rs:363` (`usize`) and `crates/scripting/src/translate/effects.rs:373` (`u32`) · **Status**: NEW
- **Description**: three independent literal `256`s, differing types, no `use` and no `const _: () = assert!(..)`; both consumer crates already depend on `byroredux-papyrus`. Dim 4 also measured that the papyrus caps are *additive* across the statement and expression axes (255 nested `If` + 127 paren pairs) — fits 1 MiB at the workspace's `opt-level = 1`, so not a safety gap, but "share one stack-safety budget" (`stmt.rs:35-37`, `lift.rs:348-353`) is loose wording.
- **Suggested Fix**: make the papyrus constants `pub` and reference them, or add a `const` assert beside each copy.

#### SCR-D5-2026-09-06-05 / SCR-D6-2026-09-06-05 (merged): `MAX_PROVIDER_FRAGMENT_BARRIERS = 64` guards a recursion that cannot cycle, and its early return discards the 64th tail's already-queued non-provider deferred mutations
- **Dimension**: Scripting Runtime Systems (dispatch) — also flagged by Dim 5 · **Untrusted-Input**: No · **Location**: `crates/scripting/src/fragment.rs:254, 594-600` · **Status**: NEW
- **Description**: every provider tail is a strict suffix (`effects[index + 1..]`), so `apply_at_depth`'s recursion terminates structurally; the cap fires only on ≥65 barriers (plausible for StorageUtil-heavy init fragments). At the cap the method returns *before* flushing `scene_actor_bindings_dirty`, `activations`, `reference_enable_changes`, and `cinematic_presentation` — but the `deferred` reaching that depth was filled by a tail whose `stages`/`objectives` mutations were already committed under the guard. Partial application with one WARN; `MAX_CASCADE` makes the opposite (correct) choice by checking before applying.
- **Suggested Fix**: check `depth + 1 >= MAX…` before running the tail's `apply_effects` (skip the tail whole), or flush non-provider deferreds before any early return; or move the bound to lowering.

#### SCR-D5-2026-09-06-06: `CanonicalEvent::from_papyrus` has no production caller — `tables.rs` claims it is "the *only* place Papyrus event names are interpreted" while two live sites match names inline
- **Dimension**: Recognizer-Chain Soundness · **Untrusted-Input**: No · **Location**: `crates/scripting/src/translate/tables.rs:28-31, 65-79`; inline interpreters `quest_stage_gate.rs:215-237`, `papyrus_provider/lower_program.rs:63-79` · **Status**: NEW
- **Suggested Fix**: route `find_advance_event`/`lower_event_into` through it, or delete it and fix the module doc.

#### SCR-D5-2026-09-06-07: the #1816 panic net covers `decompile_script` only — `analyze_pex_compatibility` and `lower_provider_program` run on the same untrusted-derived data outside `catch_unwind` on every entry variant
- **Dimension**: Recognizer-Chain Soundness (seam into the unaudited SDK layer) · **Untrusted-Input**: Yes · **Location**: `crates/scripting/src/translate/mod.rs:152-153, 158`; `crates/scripting/src/fragment.rs:1926-1927, 2178-2179` · **Status**: NEW (hardening; no panic demonstrated)
- **Description**: ~7k LOC of unaudited code (incl. the `unreachable!` arms in `papyrus_provider/execute.rs:848-858`) now sits on the cell loader's untrusted path with no net; a panic there aborts cell load exactly as #1816 did.
- **Suggested Fix**: widen `decompile_catching_panics` to the whole decompile → preflight → provider-lower → recognize sequence; route "can it panic?" to the dedicated SDK pass.

#### SCR-D6-2026-09-06-02: `apply_effect`'s doc comment — the inventory the Dim-6 checklist delegates to — describes a lock-nesting shape that no longer exists
- **Dimension**: Scripting Runtime Systems · **Untrusted-Input**: No · **Location**: `crates/scripting/src/fragment.rs:766-796` · **Status**: NEW (#3493 CLOSED re-attached this doc; drift is post-fix)
- **Description**: says the nested acquisitions run "while the caller still holds the `QuestStageFragments`/`QuestStageState`/`QuestObjectiveState` resource locks for the whole cascade loop" and counts "12 component-storage acquisitions". `QuestStageFragments` is a clone, never a guard; since the guard-free rework the two quest guards are scoped per fragment and re-acquired per provider tail; the real count is ~15 storage types across ~25 sites plus `EquipItemCatalog`, `SceneRegistry`, `PapyrusPlayerEntity` ×2, `FormIdPool`, and the `FragmentExecutionQueue` write.
- **Suggested Fix**: rewrite around `apply_fragment_guard_free`'s per-fragment scope; list acquisitions by helper rather than a hand count.

#### SCR-D6-2026-09-06-03: `OnEquipEvent` was deleted but four ground-truth doc lines still describe it as defined/shipped
- **Dimension**: Scripting Runtime Systems · **Untrusted-Input**: No · **Location**: `docs/engine/scripting.md:145`; `docs/engine/m47-0-design.md:103, 162`; `docs/engine/m47-2-design.md:312, 371` · **Status**: NEW
- **Description**: `events.rs:189-210` replaced it with `EquipmentChange` + `EquipmentEventBatch` (wearer-keyed batch, `get_mut`-then-`extend`); the docs the skill names as ground truth still list `OnEquipEvent { wearer }` as shipped.
- **Suggested Fix**: replace the four `OnEquipEvent` lines with `EquipmentEventBatch` / `EquipmentChange` (wearer-keyed batch, `get_mut`-then-`extend`) and name the two emit sites (`crates/scripting/src/equipment.rs:51`, `byroredux/src/inventory.rs:519`).

#### SCR-D6-2026-09-06-04: `papyrus_provider_system` and `legacy_obscript_load_order_system` declare a fraction of what they acquire — documentation-only today, but that is the declared purpose the under-declaration defeats
- **Dimension**: Scripting Runtime Systems · **Untrusted-Input**: No · **Location**: `byroredux/src/boot.rs:1789-1799, 1802-1811` vs `crates/scripting/src/papyrus_provider/execute.rs:31, 35, 71-157, 370, 387`; `obscript_runtime.rs:698-700` · **Status**: NEW
- **Description**: `add_exclusive_with_access` declarations do not affect scheduling (`scheduler.rs:340-353`: the analyzer "only walks parallel-stage pairs today"; exclusives run serially at `:511-512`) — so no deadlock vector. But their stated purpose (#3473: the declaration "to be compared against if either system is ever promoted to a parallel lane") is defeated: `papyrus_provider_system` actually `resource_mut`s `PapyrusProviderContinuationQueue` and `PapyrusModEventRuntime` and reads `OnInitEvent`, `HitEvent`, `EquipmentEventBatch`, `OnTriggerEnterEvent`, `OnUpdateEvent`, `FormIdComponent`, `FormIdPool` — none declared.
- **Suggested Fix**: add the missing entries; optionally a `BYRO_LOCK_ORDER_CHECK` assertion that a declared exclusive's recorded edges ⊆ its declaration.

#### SCR-D6-2026-09-06-06: the load-bearing scene→quest→continuation→cleanup ordering has no regression pin; only the flush/quest_advance half is tested
- **Dimension**: Scripting Runtime Systems · **Untrusted-Input**: No · **Location**: `byroredux/src/boot.rs:2388-2419` (existing test) vs `:1045-1090, 1904` · **Status**: NEW
- **Description**: `activation_flush_is_scheduled_before_every_activate_event_consumer` pins flush < {rumble, quest_advance, two_state} and quest_advance < quest_fragment only; `scene_playback → scene_fragment_dispatch → quest_fragment_dispatch → fragment_continuation` and cleanup-last are unpinned, and #3739's 750-line move is exactly the edit class that reorders them unnoticed.
- **Suggested Fix**: extend the static-source test with those relations and `rfind(event_cleanup_system)` > every other Late `add_exclusive`.

#### SCR-ORCH-2026-09-06-01: `docs/feature-matrix.md` and `ROADMAP.md`'s M47.2 row are silent on the ~23k-LOC SKSE/JContainers/StorageUtil/ObScript compatibility layer
- **Dimension**: Scope / doc-rot (orchestrator) · **Untrusted-Input**: No · **Location**: `docs/feature-matrix.md:165-175` (Scripting section), `:308-322` ("What Doesn't Work Yet — live gaps as of 2026-08-19"); `ROADMAP.md:749` (M47.2 row) · **Status**: NEW (#3847, OPEN, covers `_audit-common.md`'s `crates/sdk` row — a different document)
- **Description**: `grep -i "skse\|jcontainers\|storageutil\|extender\|provider"` over both files returns nothing relevant, while `docs/engine/sdk-v0.1-development-plan.md` (1563 lines) and `sdk-v0.1-next-action-plan.md` describe a shipped vertical slice ("ten SKSE `Game` content calls plus the vanilla Papyrus … executable without an extension package"). The scripting-section line numbers the skill pins (174 / 308 / 322) are otherwise **still exact** — no other doc-rot in that file this cycle.
- **Suggested Fix**: one row in the Scripting section + one sentence in the M47.2 row pointing at the SDK plan, marked as "vertical slice, unaudited" until the dedicated pass lands.

#### SCR-D8-2026-09-06-02: the router (`scene_trigger_actor_approach_system_inner`) and the gate (`actor_quest_trigger_is_in_sequence`) agree for same-quest scene waits but diverge on two inputs — cross-quest `GetStageDone` phase waits and centerless triggers
- **Dimension**: Havok Idle / Cinematic Slice · **Untrusted-Input**: No · **Location**: `byroredux/src/systems/cinematic.rs:470-486, 561-573` vs `crates/scripting/src/trigger.rs:369-395, 420-440` · **Status**: NEW (the 08-30 pass dropped the general "they disagree" candidate after tracing the same-quest case; these two corners were not examined then)
- **Description**: re-traced: between scenes (same quest) router min == gate min; during a running scene router ⊆ gate — routed ⇒ allowed, confirming 08-30. Two asymmetries survive: (a) the router collects `awaited` from **every** running scene's current phase with no `scene.quest_form_id == param_1` filter, while the gate consults only the owning quest's scenes — a scene of quest Q₀ awaiting `GetStageDone(Q, S)` can route Q's actor toward a trigger Q's own between-scenes rule then refuses; (b) the router additionally requires a resolvable center (`TriggerVolume` or catalog entry), the gate's `next_ready` does not — a centerless lowest-stage trigger routes the horse to the next-lowest, which the gate refuses. Both stall the cart silently. Neither input was located in content (no ESM parse run); severity LOW as content-gated.
- **Suggested Fix**: derive both from one `crates/scripting` helper (`next_allowed_base_form_stages(world, quest)`), so they cannot drift; add a cross-quest-wait agreement test.

#### SCR-D8-2026-09-06-03: #3838 left the approach system's doc comment attached to the scratch struct, and the system function itself is now undocumented
- **Dimension**: Havok Idle / Cinematic Slice · **Untrusted-Input**: No · **Location**: `byroredux/src/systems/cinematic.rs:405-421, 436` · **Status**: NEW
- **Description**: the four-line "Bridge offscreen cinematic locomotion…" doc (`:405-408`) now runs straight into the scratch struct's doc (`:409-420`), so both attach to `SceneTriggerApproachScratch`; `fn scene_trigger_actor_approach_system_inner` (`:436`) has no doc.
- **Suggested Fix**: move `:405-408` above `:436`.


## Skill-file drift found this pass (code is right, the checklist is stale)

Per the standing rule — when the skill's premise disagrees with the code, trust
the code and say so. Consolidated across dimensions:

1. **Dim 1** — entry-point list and Dimension enum omit `crates/pex/src/call_sites.rs` and its guard `scans_compiled_extender_dependent_calls_with_debug_lines` (`lib.rs:586`); the pipeline description is one stage short (preflight runs between parse and decompile, *outside* the panic net — that invariant should be stated); the debug-table gating predicate is `self.endian != Endian::Big` (`reader.rs:229`), not `is_skyrim()`.
2. **Dim 2** — `replace_constant_id`'s >1-match is a real fail-closed `Err` (#2666, `lift.rs:429-448`), not the "debug-only assert" the checklist warns about.
3. **Dim 3** — the auto-state match is `eq_ignore_ascii_case` (#3786), not `==`; the recursion-caps bullet should name the third, *data*-depth cap `lift::MAX_EXPR_DEPTH` and — once SCR-D3-01 is fixed — require it to hold across the `control_flow.rs:225` / `boolean.rs:292` re-folds; the pass-order bullet's "which the file documents it *skips*" parenthetical is pre-#1732 wording; the 08-30 "Existing" list's #3501 entry is now fixed.
4. **Dim 4** — lex errors have not been fatal since #2025 (`parse_script` returns `Ok((Script, errors))` with an `IntLit(0)` placeholder; only the header path is `Err`); the guard count is 91 + 4, not "~56"; `ErrorKind::StatementTooDeep` should be named beside `ExpressionTooDeep`.
5. **Dim 5** — the Conditional-dispatch bullet describes `apply_effects(branch)` + `continue`; since `962c9375` it is `branch ++ tail` + `break` and item (d)'s transitive-bound reasoning is invalid (SCR-D5-02). The entry-point list omits `Effect::Enable`, `Effect::ProviderCall`/`FragmentProviderCall`, `attribute_provider_calls`, `classify_effect_with_providers`, `lower_fragment_with_quest_properties_and_providers`, `translate_pex_detailed*`/`PexTranslation`, the `populate_*_detailed*`/`*_with_providers`/`populate_owned_*` family, `OwnedFragmentProviders`, and `recognize_specific_actor_trigger`. `Stmt::ExprStmt → classify_effect(..)?` is now `classify_effect_with_providers`, catalog-first. `DeferredFragmentEffects::apply` returns `Vec<QuestStageAdvanced>`; the per-fragment unit is `apply_fragment_guard_free`.
6. **Dim 6** — "only two resource locks … held across the dispatch loop" is no longer the shape: no quest guard is held across the cascade loop; each fragment runs inside `apply_fragment_guard_free`, then `deferred.apply` guard-free, then `poll_fragment_generated_advances` re-acquires `QuestStageState` alone. `QuestStageAdvancedBatch` is now a compatibility mirror — the authoritative ingress is the `QuestStageState` event journal (`poll_quest_events(FRAGMENT_QUEST_EVENT_SUBSCRIBER)`, `fragment.rs:2455-2480`). The Pattern-A list should read `OnInitEvent`, `EquipmentEventBatch` (not `OnEquipEvent`). The checklist delegates the lock inventory to `apply_effect`'s doc comment, which is itself stale (SCR-D6-02). #3279 is CLOSED (one agent carried it as open).
7. **Dim 7** — the "silent-miss everywhere" bullet describes only the Papyrus lane; `attach_script_for_refr` now has a second load-bearing lane (`attach_scpt_script` → compatibility analysis + `attach_legacy_obscript_program`, dialect-gated on `(GameKind, CharacterRulesProfile)`: Oblivion → `Obse`, FNV → `Xnvse`, FO3/Skyrim+ → none; guards `scpt_compatibility_tests`, 5 unit + 1 `#[ignore]`), and the `M47.2 scripts:` counter also fires on `PapyrusProviderProgram`/`LegacyObscriptProgram` attaches (`attach.rs:173-174 attached = scpt | vmad`). `attach_vmad_scripts` calls `translate_pex_detailed_with_providers`, not `translate_pex`; `extract_pex` is a thin wrapper over `resolve_pex`. The quest-advance "verify nothing can deliver both events to one entity" premise is stale — #2130 added an explicit `signalled` dedup (Activate wins), pinned by `both_activate_and_trigger_enter_in_one_frame_advance_exactly_once`; the named guard `activate_and_trigger_in_same_frame_both_advance` tests two *different* entities. `PlayerEntity` → `PapyrusPlayerEntity` (#3710). `base_record_script_instance` is untouched by the +122 diff (all #3403 / record-metadata work) — seven arms, keyed by `base_form_id`, REFR-own VMAD handled additively by `dedup_vmad_scripts` (`attach.rs:758-778`).
8. **Dim 8** — the "Extra Per-Finding Fields → Dimension" enumeration omits `Havok Idle / Cinematic Slice` (Dim 8 has existed since 2026-08-13). The "Playback lifecycle" bullet says the idle request is "drained" — it is deliberately **retained** (`systems/cinematic.rs:18-20`: unresolved requests stay pending for a later archive install) and restarts are prevented by the `consumed_idle_serial` gate; reword. The retention bullet attributes a two-hop grandchild assertion to `active_tether_retains_horse_cart_rider_and_hierarchy` — it asserts one hop (`unload.rs:695-728`); the walk is transitive by construction (`:38-47`), so this is a test gap not a code bug. The router bullet's "no single canonical test name" — the guard is `awaited_actor_trigger_moves_loaded_matching_base_not_remote_stub` (`cinematic.rs:1221`), whose tail is also the #3838 stale-scratch guard. `cinematic_horse_route_system` (`boot.rs:1099`) is neither new nor renamed — present at `18a6bc94:boot.rs:959`, moved by #3739 only.

## Stale candidates dropped

Candidates considered and disproved this pass (one line each; agents' full
reasoning is in the per-dimension files):

- *Reader `with_capacity` OOM* — every one `u16`-fed except the var-arg vec (`Vec::new()`+push, #1710). Linear amplification only.
- *`lower_binary_op` default arm turns an unknown op into `==`* — producer set is closed (`+ - * / % == < <= > >= is && ||`), `is` intercepted first.
- *Boolean `reprocess` loop spins* — returns `true` only after removing a non-exit rejoin; graph strictly shrinks.
- *Departure (1) false-merge* — the one reachable shape is a `Bool` `t = a; if t { t = b }` ≡ `t = a && b`; semantically identical, no wrong-behaviour vector.
- *#3783's 256 cap doesn't cover `combine`/reconstruct deepening, but the 1024 recursion caps compose to bound it* (Dim 2) — **refuted empirically by Dim 3 and the orchestrator**; became SCR-D3-01.
- *Papyrus `MAX_EXPR_DEPTH` + `MAX_STMT_DEPTH` additive overflow of a 2 MiB thread* — measured: only at `opt-level = 0`; the workspace dev profile is `opt-level = 1` and fits 1 MiB; no production `.psc` parse path exists off the main thread.
- *`to_original` mis-maps span ends abutting a continuation* — pre-existing, cosmetic, `render` uses `span.start` only.
- *Catalog-first precedence breaks `Utility.Wait`/`Game.*` with NO extension installed* — built-in catalog inserts non-strictly; needs a strict manifest alias (that reachable case is SCR-D5-01).
- *`Effect::Enable` mirrors `Disable` incompletely*; *`_and_providers` switch regressed #2538/#2657*; *barriers reorder side effects*; *`has_latent` misses a doubly-nested latent*; *`unsupported` compatibility should force decline* — all disproved by trace or passing guard.
- *Self-deadlock: `DeferredFragmentEffects::apply` under a live quest guard* — all four production sites close the guard block first; the always-on reentrancy tracker would panic.
- *ECS guard held across the provider `callback`* — owned `Arc`; `drop(programs)` before host entry.
- *`fragment_activation_flush_system` double-fires across frames* — `mem::take` + single insert, flush before all consumers, cleanup at Late.
- *A seventh direct `QuestStageAdvancedBatch` writer* — none.
- *`actor_quest_trigger_is_in_sequence` vs the cinematic router disagree* — re-dropped; only the #3580 scope reorder changed since 08-30.
- *`EquipmentEventBatch` from `inventory::apply_action` lands after its Late consumers* — driven from `main.rs:1149`, outside `scheduler.run`.
- *Provider-call `Err` drops the fragment tail* — explicit, tested design; left to the SDK pass (the `Default`/no-callback inconsistency is SCR-D5-03).
- *`legacy_script_principal`'s `.expect` reachable* — unreachable: ≤96 bytes < 128, alnum start, charset within grammar.
- *`populate_*` panics on `world.resource::<PapyrusProviderRuntime>()`* — inserted unconditionally by `papyrus_provider::register`.
- *OOB on hostile SCDA in the new SCPT lane* — every slice guarded by `read_u16` + `checked_add` + explicit length check (`obscript.rs:90-125, 205-218, 290-302`; `obscript_runtime.rs:322-400`).
- *FO3 source-less scripts get the xNVSE table* — gated on `character_rules == FALLOUT_NEW_VEGAS`; pinned by `source_less_fo3_scpt_does_not_apply_xnvse_opcode_table`.
- *`M47.2 scripts:` summary broke in the re-plumb* — `complete.rs` unchanged in range; counters wired on both lanes.
- *#3838 scratch carries last frame's `awaited`/`between_scenes`* — every buffer `clear()`ed before its only write (`cinematic.rs:453/463/470/487`); the #3838 test tail drives the closure twice with every `ScenePlayer` removed.
- *#3838 changed which trigger the horse is routed toward* — `bases` filter, `min_by_key` (`:575`) and the `u16::MAX` `retain` (`:589`) are outside every diff hunk.
- *Closure-captured scratch shared or non-`Send`* — `impl FnMut + Send + Sync` compiler-enforced; one instance per `make_` call; single registration.
- *`strip_retained_cell_root` orphans the entity from `CellRootIndex` or the transform hierarchy* — `drain_cell_victims` already removed the cell entry; REFR sub-entities parent to their own `placement_root`, never the cell root. Residual lifetime gap is #3817.
- *`Vec::dedup` in `idle_animation_candidates` leaves non-adjacent duplicates* — the three stems are pairwise distinct by construction.
- *Re-entrant `SceneRegistry` read inside `evaluate_condition_list` under the held guard deadlocks* — read-under-read is permitted; folded into SCR-D8-01 as one of the widened edges.

## Existing / correctly-tracked — NOT re-filed

All re-verified present against current source:

- **#3487** (OPEN) — `prim_move_to` still `args.len() == 1`; the structural 100% real-corpus `MoveTo` decline stands.
- **#3496** — `prim_set_stage` / `prim_set_objective_*` still have no upper arity guard.
- **#3159** (OPEN) — no `Lock`/`Unlock` effect primitive.
- **#3160** (OPEN) — `m47-triggers.sh` has no assertion that can fail on an attach regression.
- **#3817** (OPEN) — `HorseTetherState`/`ActorCinematicState` never terminate (Dim 8; cited, not re-filed).
- **#3854** (OPEN) — `fragment.rs` 2540 lines / 519-LOC `apply_effect`; the provider seam added six more `populate_*` entry points and SCR-D6-02's doc drift is a symptom.
- **#3892** (OPEN) — `QuestStageState`'s dynamic-subscription methods superseded.
- **#3847** (OPEN) — `_audit-common.md`'s `crates/sdk` row understates the crate (related to SCR-ORCH-01, different document).
- CLOSED and verified in place: #3783 (single-block shape), #3785, #3279, #3278, #3277, #3489, #3254, #3690, #3838, #3441, #3580, #2668, #3501, #3786, #2654 (fragment side), #1710, #1816/#3287, #2024, #2666, #2667, #1732, #2185, #2025.

Noted but too small to file (`/audit-tech-debt` territory): `complete.rs:185` comment understates what `scripts_recognized` now counts; the new `#[ignore]` test `installed_legacy_masters_have_structurally_valid_scda` parses FalloutNV.esm + Oblivion.esm whole inside `-p byroredux` (same memory-spike class as the plugin crate's `--ignored` set — do not run it casually); `try_resource` vs `resource` idiom split for `PapyrusProviderRuntime` between `attach.rs:694-697` and `script.rs:177/259`; `control_flow.rs:27-29` module doc still says the conditional-`last` block is "advanced past"; `pex_corpus_smoke.rs:293-296` exits non-zero on parse failures only; `lib.rs:552/563/573` fixture comments label the `Value::Integer` tag byte as the var-arg count; `model.rs:43` FO4 `game_id` comment; `mod.rs:214` unreachable `expect_eol` arm; a persisted `FragmentExecutionQueue` (`save_io.rs:495`) can carry an `Effect::ProviderCall` whose extension is gone at load (cross-session liveness edge for `/audit-save`).

## Future-Phase Readiness

- **Obscript / `SCTX` (M47.2 Phase 5)** — the `.psc`-side frontend is still unbuilt; the *compiled* `SCDA` reader that landed 2026-09-01 (`obscript.rs`) is a different thing and is in the coverage gap. The invariants a third frontend must inherit are now sharper than last pass: the untrusted-input contract must be bounds-safe, allocation-bounded, **recursion-bounded as a property of the tree, not of one pass** (SCR-D3-01's lesson — a per-call ledger is not a bound), and **time-bounded** (SCR-D1-01's lesson — a total but quadratic preflight on the load path is a DoS); and the decline-on-unmodeled rule must hold at *every* consumer of the AST, including any provider/extension seam (SCR-D5-01's lesson — a second table consulted ahead of the canonical one inverts the invariant).
- **The fragment lowerer (b2)** — fully wired and live-verified; the open soundness items are now the provider seam (SCR-D5-01/03), the sequential-Conditional recursion (SCR-D5-02), and the cap-then-partial-apply shape (SCR-D5/D6-05). These should be settled before any further widening of what a fragment may contain.
- **M47.1 condition resolvers** — all 13 catalog functions remain implemented with correct safe-default sentinels (`GetActorValue` 0.0, `GetDistance` `f32::MAX` on unresolved target); the #3441/#3580 lock-cycle fixes hold. Live-headless-cell re-verification against real CTDA data remains outstanding (not attempted — no engine launch).
- **M47.3 Phase 4+** — Created Object alias spawn, Story Manager event fills, true `LCTN` traversal, reference-collection aliases, unloaded-world Find-Matching search, and the injected packages/spells/keywords overlay families remain parsed-and-exposed rather than applied. Documented, not silent; not re-filed.
- **Corpus instrumentation** — `AddItem`/`MoveTo` yield question closed 2026-08-27 (#3487 tracks the structural `MoveTo` zero); not re-run. The smoke harness gains one LOW (SCR-D3-02) and still cannot gate a decompile regression via its exit code.
- **The SDK / extender layer** — needs its own audit skill, designed with the same rigor as Dim 8 was for `hkx`: untrusted `SCDA` decode discipline, the provider front-end/back-end typing contract (the `unreachable!` arms), `Wasm` host-boundary guard discipline, and the `Access` declarations. This pass's seam findings (SCR-D5-01/02/03/07, SCR-D6-04) are the entry points.

## Findings Count

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 5 |
| MEDIUM | 4 |
| LOW | 14 |
| **Total** | **23** |

Dimensions producing **no new findings**: **2** (Decompiler CFG & Lift — the prior HIGH #3783 is closed and its fix verified for its own shape).
Dimensions producing findings: **1** (1 MEDIUM, 1 LOW), **3** (1 HIGH, 1 LOW), **4** (2 LOW), **5** (2 HIGH, 2 MEDIUM, 3 LOW — one LOW merged with Dim 6), **6** (1 HIGH, 4 LOW), **7** (1 MEDIUM), **8** (1 HIGH, 2 LOW), orchestrator (1 LOW).

Three of the five HIGHs are regressions introduced by fixes landed since the last pass (`88e7dbfc` #3783 → SCR-D3-01 bypass; `962c9375` → SCR-D5-02; `a3980338` #3838 → SCR-D8-01); one is a pre-existing ordering defect no prior pass adjudicated (SCR-D6-01); one is the new provider seam's precedence (SCR-D5-01).
