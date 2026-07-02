# Scripting Subsystem Audit — 2026-07-02

**Scope**: M30 `.psc` parser (`crates/papyrus`), M47.2 `.pex` reader + 5-phase
decompiler (`crates/pex`), the AST→ECS recognizer chain + ECS scripting runtime
(`crates/scripting`), and the engine-side cell-loader attach path
(`byroredux/src/cell_loader/references.rs`, `byroredux/src/asset_provider/script.rs`).

**Depth**: deep · **Dimensions**: all 7.

This is the third scripting audit (prior: 2026-06-23, 2026-06-27). The dominant
finding of this pass is that **every actionable bug from the two prior audits has
been fixed and verified in place** — the untrusted-input hardening, the
decline-invariant leaks, the marker-drain gaps, the scheduler-registration gap,
and the `or_next` OOB panic are all closed. The residual findings are two
untrusted-input robustness gaps the prior audits did not surface, one runtime
edge-trigger semantics bug, and minor doc-rot.

---

## Executive Summary

**What shipped** (verified against source):
- M30.2 `.psc` lexer + Pratt parser → AST, with recursion-depth caps on **both**
  the expression axis (`MAX_EXPR_DEPTH = 256`) and the statement/block axis
  (`MAX_STMT_DEPTH = 256`).
- M47.2 `.pex` reader (`crates/pex/src/reader.rs`) + `#[repr(u8)]` opcode
  `transmute` decode (`opcode.rs`), + the 5-phase decompiler (cfg → lift+copy-prop
  → boolean-collapse → control-flow → lower).
- The recognizer chain (`crates/scripting/src/translate/`) with the
  decline-on-any-unmodeled-term invariant enforced per-atom and per-effect.
- The ECS runtime (events / timers / conditions / triggers / quest stages /
  fragments / recurring updates) — all systems wired into the engine scheduler.
- The dynamic cell-loader attach path (base + per-REFR VMAD → `.pex` → recognizer)
  and XPRM trigger-volume spawn.

**Deferred (not defects)**: Obscript/SCTX frontend (Phase 5); the QUST-VMAD
fragment-section decoder that would *populate* `QuestStageFragments` (the fragment
lowerer + dispatcher exist and are tested but see no live data — tracked #1739);
the M47.1 condition-resolver refinements (#1663–#1668, #1316 — several now
implemented, e.g. `GetFactionRank` returns the `-1` sentinel).

**Test state**: `cargo test -p byroredux-pex -p byroredux-papyrus -p
byroredux-scripting` = **280 passed, 0 failed**.

### Untrusted-input robustness verdict — **NOT FULLY CLEAN**

A corrupt/hostile `.pex` reaches the cell loader via `attach_vmad_scripts →
translate_pex → decompile_script`. Two paths can still take the engine down:

1. **`boolean.rs::BoolPass::rebuild` has no recursion-depth cap** (SCR-D2-01,
   HIGH). Every other decompiler tree walk that recurses on untrusted structure
   was capped — `control_flow.rs` got `MAX_REBUILD_DEPTH` (#1729), the `.psc`
   parser got `MAX_EXPR_DEPTH`/`MAX_STMT_DEPTH`. The boolean pre-pass was missed.
   A `.pex` with deeply-chained `&&`/`||` short-circuits recurses once per nesting
   level with no bound → stack overflow (uncatchable abort).
2. **`translate_pex` calls `decompile_script` without `catch_unwind`**
   (SCR-D5-NEW-02, MEDIUM). The corpus-smoke harness wraps decompile in
   `catch_unwind` precisely because a hostile `.pex` can trip an internal
   `.expect()`; the live boundary does not, so any decompiler `expect`/`unwrap`
   panics the cell loader instead of degrading to a silent `None`.

The *parse* layer (`reader.rs`) is clean: `take(n)` is the single bounds gate
(`checked_add` + `<= len`), the var-arg OOM (#1710) is fixed with geometric
growth, and the opcode `transmute` is sound (`#[repr(u8)]`, contiguous 0..=50,
`byte >= MAX_OPCODE(51)` guard, every discriminant round-trip-tested).

### 99.996 % decompile-rate claim — **VERIFIED HONEST**

`crates/pex/examples/pex_corpus_smoke.rs` counts a decompile `Err` *and* a
`catch_unwind` panic as failures, and only `Ok(Ok(_))` as success; the reported
rate's denominator is `ok + err + panic`. The one caveat (unchanged from prior
audits): the rate is conditional on *parse* success — parse failures aren't in
the denominator. Honest, but narrow.

### `.psc`-vs-`.pex` fidelity gate — **VERIFIED**

`quest_stage_gate.rs::recognizes_da10_and_reproduces_hand_builder` asserts the
recognizer output against the hand-built `da10_main_door` component field-by-field
(quest, target_stage, per-condition function_index/param_1/param_2/comparand). It
runs on the `.psc` source; the decompiled-`.pex` parity for the same script is the
still-open ignored e2e (#1740, needs game data on disk).

### Doc-rot

The prior "transpiler unstarted" feature-matrix rot is **fixed** (lines 139/180
now annotate the shipped `.pex` recognizer slice). One residual: line 137 still
says CTDA "✓ 7 functions" — the catalog now ships **13** (SCR-D6-NEW-01, LOW).

---

## Decompiler Soundness Matrix

| Pass | Bounds-safe? | Terminates? | Total (no panic on valid)? | Fidelity-tested? |
|------|-------------|-------------|---------------------------|------------------|
| reader (`reader.rs`) | ✅ single `take` gate; var-arg OOM fixed | ✅ | ✅ all-or-`Err` | ✅ handbuilt FO4 + truncation/magic |
| cfg (`cfg.rs`) | ✅ `checked_target` 0..=count; `split(0)` unreachable | ✅ | ✅ | ✅ diamond/loop/OOB |
| lift + copy-prop (`lift.rs`) | ✅ `a[n]` bounded by arg_count contract | ✅ (restart-on-fold, but bounded by scope) | ✅; `>1`-consumer = `Err` | ✅ chained-temp / double-use |
| **boolean (`boolean.rs`)** | ✅ operand/rejoin keyed | ⚠️ termination guard OK, **but no recursion-depth cap** (SCR-D2-01) | ✅ | ✅ `&&`/`||` collapse |
| control-flow (`control_flow.rs`) | ✅ `MAX_REBUILD_DEPTH` cap | ✅ | ✅ **fails closed** on `\|\|`-skip (#1732) | ✅ if/else/while |
| lower (`lower.rs`) | ✅ `name.get(..2)` boundary-safe (#1765) | ✅ | ✅ total; lossy lowerings recognizer-irrelevant | ✅ event/function classify |

**Documented Champollion departures — adjudication**:
- *No debug-line guard in `boolean.rs`* — benign. `take_operand` requires the
  fall-through operand block to recompute the *same* condition variable
  (`result == cond`), a strong structural signal; validated by the corpus rate.
- *The `||`-skip in `control_flow.rs`* — was a silent-drop bug, now **fixed**
  (#1732): the branch returns `ControlFlowFailed` (fail-closed) so the recognizer
  cleanly declines rather than matching a truncated body.

---

## Decline-Invariant Audit

| Decline point | Verified conservative? |
|---------------|------------------------|
| `compose::classify_guard_atom` (`?` per atom in `classify_if_condition`) | ✅ unmatched atom → whole handler declines |
| `compose::split_and` keeps `\|\|` whole | ✅ disjunction is one un-split atom no primitive claims |
| `quest_stage_gate::extract_stage_gate` post-peel body must be `[only]` | ✅ (#1719) — sibling statement declines |
| `quest_stage_gate::single_set_stage` inner `If` body must be `[only]` | ✅ (#1766 / SCR-D5-NEW-01) — inner-sibling before/after declines |
| `quest_stage_gate` quest cross-check (`quest_via != set_via → None`) | ✅ mismatched quest declines |
| Hole binding (`OwningQuest`/`Property`/`SelfRef`) | ✅ each unbound → `None`, never form-id 0 |
| `effects::lower_fragment` (`_ => return None` on control flow / valued return) | ✅ only `ExprStmt`/`Return(None)`/quest-bind accepted |
| `translate_pex` bad bytes → clean `None` | ✅ parse/decompile `Err` → `log::debug` + `None` (but see panic gap SCR-D5-NEW-02) |

The load-bearing decline invariant is intact. No recognizer emits a component on
a partial or approximated match.

---

## Runtime Lifecycle Invariant Matrix

| Invariant | State |
|-----------|-------|
| Marker drain coverage (`cleanup.rs`) | ✅ every emitted marker drained (cross-checked all `insert` sites): Activate/Hit/TimerExpired/AnimTextKey/OnUpdate/QuestStageAdvanced/CameraShake/Rumble/UiMessage/OnTriggerEnter/OnCellLoad/OnEquip |
| Cleanup runs last | ✅ `Stage::Late`, after all `Stage::Update` scripting systems |
| Two-phase lock-drop | ✅ `timer_tick`, `trigger_detection`, `recurring_update_tick` each collect-then-`drop()`-then-insert |
| Fragment dispatch resource locks | ✅ `QuestStageFragments`+`QuestStageState`+`QuestObjectiveState` in one scoped block, no component lock held across |
| Cascade bound | ✅ `MAX_CASCADE = 64` + WARN; no-op re-set skipped |
| Producer→consumer order | ✅ `quest_advance_dispatch` (emit) before `quest_fragment_dispatch` (consume) in the Update stage |
| CTDA OR-precedence + trailing-`or_next` clamp | ✅ block scan; `i.min(len-1)` clamp (#1767) |
| Scheduler registration | ✅ `recurring_update_tick_system` + `quest_fragment_dispatch_system` now added (#1768) |
| Edge-triggered enter seed | ❌ `occupant_inside` spawned `false`, not seeded from initial containment (SCR-D6-NEW-02) |

---

## Findings

### SCR-D2-01: Decompiler boolean-collapse pass has no recursion-depth cap — stack overflow from untrusted `.pex`
- **Severity**: HIGH
- **Dimension**: Decompiler Control-Flow / Boolean / Lower
- **Untrusted-Input**: Yes
- **Location**: `crates/pex/src/decompile/boolean.rs:110-145` (`BoolPass::rebuild`)
- **Status**: NEW
- **Description**: `BoolPass::rebuild` recurses on `self.rebuild(block.on_true(),
  block.on_false)` / `self.rebuild(block.on_false, block.on_true())` for each
  conditional block whose fall-through edge is a short-circuit operand. Unlike
  every other decompiler tree walk that consumes untrusted structure, it carries
  **no depth guard**: `control_flow.rs::reconstruct` was capped with
  `MAX_REBUILD_DEPTH = 1024` (#1729 / SAFE-2026-06-23-02), and the `.psc` parser
  has `MAX_EXPR_DEPTH` / `MAX_STMT_DEPTH`. The boolean pre-pass — which runs on the
  same untrusted CFG (`lower.rs::decompile_body` line 216) *before* the capped
  control-flow pass — was missed.
- **Evidence**: `grep -c depth crates/pex/src/decompile/boolean.rs` → `0`. The
  recursion at lines 127/131 has no `depth` parameter and no `MAX_*` check;
  `mod.rs::DecompileError` has a `RecursionLimit` variant used only by
  `control_flow.rs`.
- **Impact**: A hostile/corrupt `.pex` in a modded `--scripts-bsa` archive with
  deeply-chained `&&`/`||` short-circuit conditionals recurses one frame per
  nesting level with no bound. A sufficiently deep chain overflows the stack — an
  **uncatchable abort** (`catch_unwind` does not catch a stack overflow), taking
  the whole engine down during cell load. Same bug class as the already-fixed
  #1729, one pass upstream.
- **Related**: #1729 (the control-flow-pass sibling, fixed); SCR-D5-NEW-02.
- **Suggested Fix**: Thread a `depth: usize` through `BoolPass::rebuild` (and
  `collapse`, which calls back into `rebuild`), return `DecompileError::RecursionLimit`
  past the same `MAX_REBUILD_DEPTH` cap the control-flow pass uses. Add a
  pathological-nesting regression test mirroring `control_flow`'s
  `rebuild_rejects_excessive_recursion_depth`.

### SCR-D5-NEW-02: `translate_pex` decompiles untrusted `.pex` without `catch_unwind` — a decompiler panic aborts the cell loader
- **Severity**: MEDIUM
- **Dimension**: Recognizer-Chain Soundness
- **Untrusted-Input**: Yes
- **Location**: `crates/scripting/src/translate/mod.rs:88-110` (`translate_pex`)
- **Status**: NEW
- **Description**: `translate_pex` handles `parse`/`decompile_script` **`Err`**
  gracefully (`log::debug` + `None`) but does not guard against a **panic** from
  `decompile_script`. The decompiler carries internal `.expect()`/`.unwrap()`
  invariants (`cfg.rs::split_block` `"split target block exists"`,
  `control_flow.rs` `"conditional block has a condition"`, `lift.rs`
  `"non-final node has a result"`, the boolean-pass `.expect`s). The corpus-smoke
  harness wraps `decompile_script` in `std::panic::catch_unwind` *specifically
  because* a bad `.pex` can trip one; the live attach boundary omits that net.
- **Evidence**: `grep -c catch_unwind crates/scripting/src/translate/mod.rs` →
  `0`; `crates/pex/examples/pex_corpus_smoke.rs:144` wraps the same call in
  `catch_unwind`. The module doc claims "never a panic escaping into the cell
  loader" — true for `Err`, not for `panic!`.
- **Impact**: A hostile/corrupt `.pex` that trips a decompiler `expect` panics
  through `attach_vmad_scripts` and aborts cell load. Vanilla content is clean
  (0/26 640 corpus panics), so blast radius is modded/corrupt archives — hence
  MEDIUM not HIGH. (A stack overflow via SCR-D2-01 is *not* caught by
  `catch_unwind` regardless; that path stays HIGH.)
- **Related**: SCR-D2-01; the `translate_pex_on_*_is_a_clean_none` tests cover
  `Err`, not panic.
- **Suggested Fix**: Wrap the `decompile_script` call in
  `std::panic::catch_unwind(AssertUnwindSafe(...))`, mapping a caught panic to the
  same `log::debug` + `None` the `Err` arm uses — matching the corpus harness's
  own defense. Add a garbage-`.pex`-that-panics-decompile regression once such an
  input is characterized.

### SCR-D6-NEW-02: Trigger volume's `occupant_inside` not seeded from initial containment — spurious enter-fire when player loads already inside
- **Severity**: MEDIUM
- **Dimension**: Engine Attach & Trigger Wiring (runtime consequence)
- **Untrusted-Input**: No
- **Location**: `byroredux/src/cell_loader/references.rs:1455-1461`
  (`trigger_volume_from_primitive`) + `crates/scripting/src/trigger.rs:114-120`
- **Status**: NEW
- **Description**: `trigger_volume_from_primitive` hardcodes
  `occupant_inside: false` at spawn. `trigger_detection_system` fires
  `OnTriggerEnterEvent` on the `inside && !occupant_inside` edge. When the player
  begins a cell/save load *already standing inside* a trigger volume, frame-1
  detection sees `inside == true` against the seeded `false` and fires a spurious
  enter — i.e. level-triggered-on-load rather than edge-triggered. Bethesda's
  `OnTriggerEnter` semantics fire only on an actual outside→inside crossing.
- **Evidence**: spawn site sets `occupant_inside: false` unconditionally; the
  SKILL's Dim-6 seed contract ("a player loaded already inside a volume must NOT
  spuriously fire on frame 1 — `occupant_inside` seeded true") is unmet. Distinct
  from #1742 (which is about the *rotation frame* of the permuted half-extents).
- **Impact**: A quest gated on `OnTriggerEnter` can advance the instant the player
  loads a save while inside the trigger box, even though they never crossed the
  boundary that frame — silent game-logic corruption on load. Realistic for
  autosaves taken inside a scripted trigger region.
- **Related**: #1742 (trigger-box rotation frame), #1727 (drain, fixed).
- **Suggested Fix**: Seed `occupant_inside` from the volume's containment of the
  player's initial world position at spawn (or run one silent "prime" pass of
  `trigger_detection_system` that updates `occupant_inside` without emitting
  markers before the first gameplay frame).

### SCR-D6-NEW-01: `feature-matrix.md` understates the CTDA condition catalog (says 7, ships 13)
- **Severity**: LOW
- **Dimension**: Scripting Runtime Systems (documentation)
- **Untrusted-Input**: No
- **Location**: `docs/feature-matrix.md:137`
- **Status**: NEW
- **Description**: The matrix row reads *"CTDA condition evaluation with
  OR-precedence (M47.1) | ✓ 7 functions"*. `condition.rs` ships **13** catalogued
  functions (GetDistance, GetActorValue, GetStage, GetStageDone, GetIsClass,
  GetIsRace, GetIsID, GetFactionRank, GetLevel, HasPerk, GetXPForNextLevel,
  GetReputation, GetReputationThreshold — the file header table + the `from_index`
  arms at `condition.rs:143-155`).
- **Evidence**: `grep -cE "ConditionFunction::(…)" condition.rs` → `13`;
  `from_index` maps indices 1/14/58/59/68/69/72/73/80/448-449/533/573/575.
- **Impact**: Documentation only; understates shipped capability.
- **Related**: the prior "transpiler unstarted" doc-rot (now fixed).
- **Suggested Fix**: Update the count to 13 (or drop the count and reference the
  `condition.rs` header catalog).

---

## Confirmed-fixed prior-audit findings (verified in place)

These were reported in the 2026-06-23 / 2026-06-27 audits, filed as issues, and
are now **CLOSED with the fix verified present** — no regression:

| Prior ID / Issue | Fix verified at |
|------------------|-----------------|
| SCR-D1-01 var-arg OOM (#1710) | `reader.rs:474-488` geometric growth + `hostile_vararg_count_errors` test |
| SCR-D4-01 nested-statement stack overflow (#1712) | `stmt.rs:38,54-65` `MAX_STMT_DEPTH` + guard tests |
| SCR-D4-02 recovery-to-EOF (#1734) | `script.rs:631-637` `skip_to_next_line` walks raw tokens |
| SCR-D3-01 `\|\|`-skip silent drop (#1732) | `control_flow.rs:185-196` fails closed + `conditional_predecessor_fails_closed` |
| SCR-D3-02 boolean-pass doc-rot (#1738) | `control_flow.rs:22-31` corrected module doc |
| SAFE control-flow recursion (#1729) | `control_flow.rs:39,96-102` `MAX_REBUILD_DEPTH` |
| SCR-D5-01 guarded sibling-drop (#1719) | `quest_stage_gate.rs:178,232-260` exactly-one-statement |
| SCR-D5-NEW-01 inner-body sibling-drop (#1766) | `single_set_stage` `[only]` + `declines_guarded_inner_sibling_*` tests |
| D3-NEW-01 `name[..2]` panic (#1765) | `lower.rs:273` `name.get(..2)` + non-ASCII test |
| SCR-D6-NEW-01 `or_next` OOB panic (#1767) | `condition.rs:623` `i.min(len-1)` clamp + tests |
| SCR-D6-NEW-02 systems not scheduled (#1768) | `main.rs:769,775` both systems added |
| SCR-D6-01 OnTriggerEnter not drained (#1727) | `cleanup.rs:47` |
| SCR-D6-02 OnCellLoad not drained | `cleanup.rs:48` |
| SCR-D7-01 per-REFR VMAD never resolved (#1737) | `references.rs:1584,1606` REFR-own VMAD first |
| SCR-D8-01 feature-matrix "transpiler unstarted" | `feature-matrix.md:139,180` annotated as shipped |

---

## Still-open, correctly-tracked (NOT re-filed — dedup)

- **#1769** (D7-NEW-01): VMAD attach dedup is case-sensitive; Papyrus names are
  case-insensitive. Confirmed still present — `references.rs:1611` uses a
  `HashSet<&str>` keyed on `script.name.as_str()` (raw case). **Existing: #1769.**
- **#1743** (SCR-D7-03): `--scripts-bsa` override order is first-listed-wins.
  Confirmed at `asset_provider/script.rs:39,71`. **Existing: #1743.**
- **#1742** (SCR-D7-02): trigger-box rotation frame may not match the permuted
  half-extents. **Existing: #1742** (distinct from SCR-D6-NEW-02 above).
- **#1740** (SCR-D5-03): no decompiled-`.pex` DA10 parity test (needs game data).
  **Existing: #1740.**
- **#1739**: the fragment lowerer is complete + tested but unwired pending the
  QUST-VMAD fragment-section decoder — a **designed Phase-3 gap**, documented in
  `translate/mod.rs:35-44` and `fragment.rs:11-26`, not a defect.

---

## Future-Phase Readiness

- **Obscript / SCTX (Phase 5)**: the pre-Skyrim attach path
  (`references.rs::attach_scpt_script`) resolves `SCRI → SCPT editor_id →
  ScriptRegistry` and runs a hand-written spawner. This is the **live** mechanism
  for FO3/FNV/Oblivion (there is no Obscript decompiler yet), so
  `papyrus_demo::register_spawners` is correctly still called at engine init —
  *not* a leftover to retire. When the SCTX frontend lands it slots in as a second
  `ScriptSource` arm behind the same `translate_script` boundary.
- **Fragment lowerer (b2)**: pinned by `effects.rs` + `fragment.rs` unit tests;
  gated on the QUST-VMAD decoder (#1739). The dispatcher, cascade bound, and
  quest-ref resolution are proven; only the *population* path is absent.
- **Condition resolvers (#1663–#1668, #1316)**: the catalog has grown to 13
  functions with real ECS reads (e.g. `GetFactionRank` → `-1` sentinel,
  `GetActorValue` → composed `ActorValues`); the remaining stubs return documented
  safe-defaults. Not re-flagged.

---

## Summary

**Total findings: 4** — HIGH 1 (SCR-D2-01), MEDIUM 2 (SCR-D5-NEW-02,
SCR-D6-NEW-02), LOW 1 (SCR-D6-NEW-01). All NEW. The two prior audits' findings are
fully remediated; the residual gaps are the one un-capped decompiler recursion
pass, the missing panic-net on the live decompile boundary, and the un-seeded
trigger edge state.
