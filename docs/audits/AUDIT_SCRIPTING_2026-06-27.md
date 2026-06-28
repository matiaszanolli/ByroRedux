# Scripting Subsystem Audit — 2026-06-27

**Domain**: M30 / M47.0 / M47.1 / M47.2 — `.pex` decompiler (`crates/pex`),
`.psc` Papyrus parser (`crates/papyrus`), AST→ECS recognizer chain + ECS
scripting runtime (`crates/scripting`), and the cell-loader REFR-attach path
(`byroredux/src/cell_loader/references.rs`, `byroredux/src/asset_provider/script.rs`).

**Depth**: deep, all 7 dimensions. **This is the SECOND audit of this domain** —
an incremental re-audit four days after the first
([`AUDIT_SCRIPTING_2026-06-23.md`](AUDIT_SCRIPTING_2026-06-23.md), 16 findings).
**Method**: (1) verify each of the prior audit's fixes is correct/complete —
nearly all 16 findings were closed since, several *today*; (2) audit the new code
those fixes introduced (var-arg bound, statement-depth guard, the five just-landed
condition functions, marker drains, per-REFR VMAD); (3) fresh-eyes adversarial
sweep for what the first pass missed. Every finding re-read against current code,
the two untrusted-input panics reproduced standalone, and an attempt made to
disprove each. Dedup against the cached open-issue list and the prior report.

## Executive Summary

**Shipped (verified)**: M30.2 `.psc` lexer+Pratt parser → AST; M47.0 ECS event
hooks; M47.1 CTDA condition eval (OR-precedence + GetDistance/GetFactionRank/
GetIsID/HasPerk/GetStage/GetStageDone/Global, GetActorValue still stubbed #1663);
M47.2 `.pex` reader + 5-phase decompiler (Champollion port) + the compositional
recognizer chain + the dynamic cell-loader attach path (now incl. per-REFR VMAD)
+ XPRM trigger volumes.

**Prior-fix verdict — 10 of 11 PASS, 1 INCOMPLETE.** The prior audit's HIGH/MED
findings were re-verified against the closing commits:

| Prior finding | Issue | Fix verdict |
|---|---|---|
| Var-arg OOM (`reader.rs`) | #1710 | **PASS** — `Vec::new()`+push, file-length-bounded; regression pinned |
| Statement-depth guard (`stmt.rs`) | #1712 | **PASS** — `MAX_STMT_DEPTH=256`, balanced inc/dec, no bypass |
| Guarded-shape sibling drop | #1719 | **INCOMPLETE** — handler-level gate added, inner-`If` hole survives (SCR-D5-NEW-01) |
| OnTriggerEnter undrained | #1727 | **PASS** — drained, cleanup is last (`Stage::Late`) |
| control_flow `‖`-skip drop | #1732 | **PASS** — now `return Err(fail())`; regression pinned |
| Error recovery to EOF | #1734 | **PASS** — raw-token next-line resume, progress-guaranteed |
| OnCellLoad undrained | #1736 | **PASS** — drained |
| Per-REFR VMAD ignored | #1737 | **PASS** — REFR-first + `seen` guard, no double-attach, no-panic decode |
| control_flow doc-rot | #1738 | **PASS** |
| Fragment lowerer unwired | #1739 | documented (still unwired — see SCR-D6-NEW-02) |
| Stale Rapier comment | #1741 | **PASS** |
| GetDistance/FactionRank/IsID/HasPerk/Global | #1664–#1668 | **PASS** — sentinels + RunOn decline + remap all correct |

**Findings**: **5 total — 3 HIGH, 1 MEDIUM, 1 LOW.** Three are NEW bugs the first
pass missed; one is an incomplete fix of a prior HIGH; one is a LOW cosmetic nit.

### Untrusted-input robustness verdict — REGRESSED TO NOT-CLEAN (two new panics)

The prior audit flipped this to "clean after the #1710/#1712 fixes land." Both
fixes are confirmed solid — but this pass found **two new panic vectors that
unwind past the whole fail-closed design**, so the verdict is NOT-CLEAN again:

- **D3-NEW-01 (HIGH)**: `build_handler` byte-slices `name[..2]`; a `.pex` function
  name beginning with the 3-byte U+FFFD that `from_utf8_lossy` emits for invalid
  Win-1252 bytes panics (`not a char boundary`). Reachable end-to-end via
  `decompile_script` → `translate_pex` (which catches `Err`, *not* a panic).
- **SCR-D6-NEW-01 (HIGH)**: `condition::evaluate` indexes `conditions[len]` and
  panics when a CTDA list's last condition has `or_next == true` and the OR-block's
  preceding members all evaluate false. `or_next` is decoded raw from the plugin
  type byte with no clamp.

Everything else on the untrusted paths remains correctly bounded (`take` is the
single `.pex` read gate, the opcode `transmute` is sound, the statement/expression
depth caps hold, `ScriptInstanceData::parse` is `take`-based, the array-property
decode is `min(4096)`-capped). Fix the two panics above and the verdict flips back.

### 99.996% decompile-rate claim — still HONEST, and D3-NEW-01 shows its blind spot

`pex_corpus_smoke.rs` genuinely runs `decompile_script` inside `catch_unwind` and
counts panics+`Err` (Dim 1/3 re-confirmed). D3-NEW-01 survived precisely because
real vanilla `.pex` function names are ASCII — the corpus contains no U+FFFD name,
so a panic-only harness over real archives never hit it. This is the documented
limitation restated: the smoke gate measures "no panic on real data," not
"robust against crafted/corrupt data," and gives zero protection against a
wrong-but-non-panicking AST. The recognizer fidelity test remains the only
AST-correctness gate (and #1740 — no decompiled-`.pex` parity test — is still open).

### Decline-invariant — STILL ONE LEAK (the #1719 fix was incomplete)

The load-bearing "decline on any unmodeled term" invariant is honored at every
composer/effect/hole-binding point. The single leak the prior audit found
(SCR-D5-01, guarded `quest_stage_gate` shape dropping siblings) was *partially*
closed by #1719 — the handler-body gate is correct, but `match_guarded_if` still
uses `find_set_stage` (first-of-many) inside the guarded `If`, so a sibling
statement *inside* the `If` body is silently dropped (SCR-D5-NEW-01, HIGH).

### Doc-rot — none new

`docs/feature-matrix.md` "transpiler unstarted" (prior SCR-D8-01) was the only
matrix doc-rot; it is tracked separately and unchanged. No new stale comments
found — the #1738/#1741 comment fixes are accurate.

---

## Decompiler Soundness Matrix (re-verified)

| Pass | Bounds-safe | Terminates | Total (no panic) | Fidelity-tested | Notes |
|------|-------------|------------|------------------|-----------------|-------|
| reader (`reader.rs`) | ✅ via `take` | ✅ | ✅ var-arg now file-bounded (#1710) | FO4-LE only (#1728 open) | transmute sound; byte-exact to Champollion |
| cfg (`cfg.rs`) | ✅ `checked_target` inclusive | ✅ | ✅ no `split(0)` underflow | ✅ | jmpf/jmpt polarity verified end-to-end |
| lift + copy-prop (`lift.rs`) | ✅ `a[n]` bounded by arg_count | ✅ (fold shrinks scope) | ✅ Cast unwrap short-circuit-guarded | ✅ | `child_nodes`/`child_nodes_mut` symmetric → no release-build producer drop |
| boolean (`boolean.rs`) | ✅ | ✅ (merge strictly shrinks graph) | ✅ | corpus | no-debug-line-guard departure = accepted |
| control-flow (`control_flow.rs`) | ✅ | ✅ | ✅ | ✅ | `‖`-skip now **fails closed** (#1732 verified) |
| lower (`lower.rs`) | ✅ | ✅ | ❌ **panic on non-ASCII fn name (D3-NEW-01)** | ✅ | `lower_binary_op` default arm genuinely unreachable |

## Runtime Lifecycle Invariant Matrix (re-verified)

| Invariant | Status |
|-----------|--------|
| Marker drain coverage (all 12 transient markers) | ✅ complete (#1727 + #1736 verified; OnEquip drained ahead of emitter) |
| Two-phase lock-drop (timer / trigger / recurring / fragment / quest_advance) | ✅ explicit `drop()` before 2nd acquire; fragment 3-resource lock single-scoped |
| Cascade bound (`MAX_CASCADE=64` + no-op skip) | ✅ |
| Edge-trigger seed (`occupant_inside`) | ✅ |
| CTDA OR-precedence grouping | ✅ semantics correct — ❌ **OOB panic on trailing `or_next` (SCR-D6-NEW-01)** |
| Quest stage history (`stages_done`) | ✅ |
| Condition-function sentinels (GetDistance/FactionRank/IsID/HasPerk/Global) | ✅ all correct; RunOn declines on unresolvable target |
| Per-frame system scheduling | ❌ **recurring_update + fragment_dispatch never scheduled (SCR-D6-NEW-02)** |

---

## Findings (by severity)

### D3-NEW-01: `build_handler` byte-slices `name[..2]` — panics on a non-ASCII `.pex` function name
- **Severity**: HIGH
- **Dimension**: Decompiler Control-Flow / Boolean / Lower (AST lowering / event classification)
- **Location**: `crates/pex/src/decompile/lower.rs:266`
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: The event-vs-function classifier slices the first two *bytes* of
  the function name:
  `let is_event = (name.len() > 2 && name[..2].eq_ignore_ascii_case("on") && is_event_name(name)) || name.starts_with("::remote_");`
  `.pex` strings are decoded with `String::from_utf8_lossy` (names are kept lossy,
  "Windows-1252-ish"). Any invalid input byte (a Win-1252 byte ≥ 0x80 not valid
  UTF-8) becomes U+FFFD `�`, a **3-byte** sequence `EF BF BD`. A function name whose
  first source byte is invalid begins with this 3-byte char: `name.len() > 2` is
  satisfied, but byte index 2 lands *inside* the replacement char (not a char
  boundary). `name[..2]` then panics: `byte index 2 is not a char boundary; it is
  inside '�' (bytes 0..3)`.
- **Evidence**: Reproduced standalone — `String::from_utf8_lossy(&[0x81])` yields a
  3-byte `[239,191,189]` string with `len()==3`; `name[..2]` panics. The name flows
  unfiltered: `decompile_script` → per-state `build_handler(object, f, &f.name)`
  (`lower.rs:379/384`), `f.name` straight from the lossy-decoded string table. The
  only `[..2]` site in the crate; no upstream ASCII guard.
- **Impact**: A single malformed/adversarial (or merely non-ASCII-corrupted) `.pex`
  in a modded `--scripts-bsa` panics the decompiler instead of returning a
  `DecompileError`. The panic unwinds past the entire error-returning design
  (`DecompileError`, the fail-closed #1732 work) — `translate_pex` catches `Err`,
  not panics, so it reaches the cell loader. Per the domain rule "panic from
  untrusted `.pex` → HIGH."
- **Related**: Same class as why the corpus-smoke 99.996% claim can't catch it
  (real names are ASCII).
- **Suggested Fix**: Use a boundary-safe check —
  `name.get(..2).is_some_and(|p| p.eq_ignore_ascii_case("on"))`, or gate on
  `name.is_char_boundary(2)`. `is_event_name(name)` already lowercases safely.

### SCR-D5-NEW-01: guarded `If` inner body drops sibling statements (the #1719 leak, one level deeper)
- **Severity**: HIGH
- **Dimension**: Recognizer-Chain Soundness
- **Location**: `crates/scripting/src/translate/recognizers/quest_stage_gate.rs:244` (`match_guarded_if`) + `find_set_stage` (`:311-321`)
- **Status**: Incomplete fix of #1719
- **Untrusted-Input**: Yes (mod/vanilla `.psc` + decompiled `.pex` reach `translate_script`)
- **Description**: #1719 enforced exactly-one-statement at the **handler-body** level
  (`extract_stage_gate:178` `if let [only] = body`), so a sibling *next to* the
  guarded `If` now declines. But `match_guarded_if` resolves the target stage with
  `find_set_stage(body)?` (`:244`), which `find_map`s the guarded `If`'s **inner**
  body and returns the **first** `SetStage`, ignoring any other statement in that
  inner body. Shape 3 (`single_set_stage:299`) does NOT have this hole — it requires
  the body be exactly `[SetStage]`. The two shapes are asymmetric.
- **Evidence** (probe against the live `translate_script` boundary, instance binds `MyQuest`):
  ```
  after-sibling   If GetStageDone(10)==1 / SetStage(20) / Self.Disable() / EndIf   -> EMITTED  (LEAK: Self.Disable() dropped)
  before-sibling  If GetStageDone(10)==1 / Self.Disable() / SetStage(20) / EndIf   -> EMITTED  (LEAK: unmodeled stmt BEFORE the advance dropped)
  player-gated    If player / If GetStageDone(10)==1 / SetStage(20) / Self.Disable() / EndIf / EndIf -> EMITTED (LEAK)
  clean           If GetStageDone(10)==1 / SetStage(20) / EndIf                    -> EMITTED  (correct, no false-decline)
  ```
  The `before-sibling` case is the worse half: `find_set_stage` scans the whole inner
  body, so an unmodeled statement placed *ahead* of the SetStage is dropped too —
  precisely the failure mode #1719's commit message cites (`Self.Disable()` "silently
  dropped"), just nested inside the `If`.
- **Impact**: A guarded quest-door/activator whose authored intent is "advance the
  stage **and** disable/move/enable/notify" lowers to a bare quest advance with the
  second-or-later effect silently discarded — a false-positive lowering that corrupts
  game logic with no fallback. It also defeats the quest-disagreement guard (`:248`):
  a guarded body with `MyQuest.SetStage(20)` + `OtherQuest.SetStage(5)` only checks the
  first, dropping the second quest's advance.
- **Related**: #1719 (the fix this completes); SCR-D5-01 (prior).
- **Suggested Fix**: In `match_guarded_if`, replace `find_set_stage(body)?` with the
  exactly-one-statement form — require the guarded `If` body be a single `ExprStmt`
  that is a `SetStage` (reuse/generalize `single_set_stage`), decline otherwise. Add
  `declines_guarded_inner_sibling_{after,before}` + `declines_player_gated_inner_sibling`
  next to `declines_guarded_with_extra_statements`.

### SCR-D6-NEW-01: `condition::evaluate` panics (index OOB) when a CTDA list ends with `or_next == true`
- **Severity**: HIGH
- **Dimension**: Scripting Runtime Systems (condition evaluator / CTDA OR-precedence)
- **Location**: `crates/scripting/src/condition.rs:405-417` (panic fires at the `:416-417` range index)
- **Status**: NEW (pre-existing miss — the OR-block loop dates to 2026-06-09, before the first audit's ✅ on OR-precedence)
- **Untrusted-Input**: Yes (`or_next` decoded raw from the plugin CTDA type byte; no guard)
- **Description**: The OR-block discovery loop walks `i` while `conditions[i].or_next`,
  then sets `block_end_inclusive = i` and evaluates `(block_start..=block_end_inclusive)`.
  When the **final** condition has `or_next == true`, the loop walks `i` to
  `conditions.len()`, so `block_end_inclusive == len` and the inclusive range indexes
  `conditions[len]` → out of bounds. The `.any()` short-circuit masks it only when an
  earlier OR-block member evaluates `true`; if every preceding member is `false`,
  evaluation reaches the OOB index and **panics**.
- **Evidence**: Injected test (reverted): `evaluate(&vec![cond(99999, Ne, 0.0, /*or_next=*/true)], …)`
  → `panicked at crates/scripting/src/condition.rs: index out of bounds: the len is
  1 but the index is 1`. The plugin parser sets `or_next` straight from
  `type_byte & 0x01` with no clamp; nothing clears a trailing OR flag. `evaluate` is
  live on `quest_advance_system` and is the shared entry point for every future CTDA
  consumer (perks, dialogue INFOs, AI packages, magic effects).
- **Impact**: A malformed / hand-edited / truncated ESP whose condition tail leaves the
  OR bit set crashes the engine the first frame the predicate is evaluated — one bad
  CTDA byte takes down cell load / activation / quest advance. Silent in `cargo test`
  (no test exercises a trailing-OR list).
- **Related**: The prior matrix marked CTDA OR-precedence ✅ — that verified the
  grouping *semantics*, not this trailing-`or_next` boundary.
- **Suggested Fix**: Clamp after the inner loop:
  `let block_end_inclusive = i.min(conditions.len() - 1);` (the `while i < len`
  guarantees `len ≥ 1`). A trailing OR flag then harmlessly terminates the final block
  at its last real member, matching the "last condition's or_next is meaningless"
  contract the doc-comment already asserts. Add a trailing-`or_next` regression whose
  members all evaluate false.

### SCR-D6-NEW-02: `recurring_update_tick_system` and `quest_fragment_dispatch_system` are never added to the engine scheduler
- **Severity**: MEDIUM
- **Dimension**: Scripting Runtime Systems (lifecycle / stage wiring)
- **Location**: `byroredux/src/main.rs` (no registration); systems defined at `crates/scripting/src/recurring_update.rs:152` and `crates/scripting/src/fragment.rs:177`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `main.rs` schedules `timer_tick_system` (`:674`),
  `trigger_detection_system` (`:715`), `quest_advance_system` (`:718`), and
  `event_cleanup_system` (`:967`) — but **not** `recurring_update_tick_system` (the
  only `RecurringUpdate` token in `main.rs` is a comment) nor
  `quest_fragment_dispatch_system`. `lib.rs::register` calls
  `recurring_update::register(world)`, which registers the *component/resource*, not
  the per-frame *system*. Confirmed by exhaustive grep: both systems appear only in
  their own modules, `lib.rs` re-exports, and unit tests.
- **Impact**: `RecurringUpdate` subscriptions never count down in-engine, so
  `OnUpdateEvent` never fires at runtime (the inverse of an undrained marker — it *is*
  drained at `cleanup.rs:36`, but has no live emitter); `QuestStageAdvanced` never
  dispatches fragments. Today's blast radius is limited (no `RegisterForUpdate` caller
  ships, and the fragment resource is empty pending the QUST-VMAD decoder per #1739),
  so nothing real depends on either system yet — but the moment any script uses
  `RegisterForUpdate`, OnUpdate silently never fires. The systems' internal lock
  discipline and logic are correct; the defect is purely scheduling.
- **Related**: #1739 (fragment lowerer staged-not-wired — this is its scheduling half).
- **Suggested Fix**: Register `recurring_update_tick_system` next to `timer_tick_system`
  and `quest_fragment_dispatch_system` in `Stage::Update` after `quest_advance` and
  before cleanup, mirroring the existing closure-wrapper pattern. If the omission is
  deliberate (demos-only until the fragment population path lands), add an explicit
  `main.rs` comment recording it so the dead-handler state is documented, not latent.

### D7-NEW-01: VMAD attach dedup is case-sensitive; Papyrus names are case-insensitive
- **Severity**: LOW
- **Dimension**: Engine Attach & Trigger Wiring
- **Location**: `byroredux/src/cell_loader/references.rs:1594-1604` (`attach_vmad_scripts`)
- **Status**: NEW
- **Untrusted-Input**: Yes (VMAD script names from a modded archive/plugin)
- **Description**: The collision-dedup added by the #1737 per-REFR VMAD fix keys on the
  raw byte string — `seen: HashSet<&str>; … seen.insert(script.name.as_str())`. Papyrus
  identifiers are case-insensitive, and the codebase honors that everywhere else
  (`ScriptInstance::property`/`ScriptInstanceData::script` use `eq_ignore_ascii_case`;
  the `.pex` path normaliser lowercases; `translate/tables.rs` lowercases). If a REFR's
  own VMAD names `"MyScript"` and the base record names `"myscript"` (the same script
  under Papyrus rules), the case-sensitive `seen` set does not treat them as equal, so
  the base copy is attached a second time.
- **Evidence**: `script.name.as_str()` inserted verbatim; contrast `script_instance.rs`
  name comparisons via `eq_ignore_ascii_case`.
- **Impact**: A redundant second `extract_pex` + `translate_pex` + `(recognized.spawn)`
  for the same logical script. Because the recognizer spawn closures insert into
  `SparseSetStorage` (overwrite, not append), the second insert overwrites the first
  with identical data — ECS outcome is idempotent. Wasted-work / contract-inconsistency,
  not a double-advance or corruption bug.
- **Related**: #1737 (the fix that introduced the dedup set).
- **Suggested Fix**: Lowercase the key before insertion
  (`seen.insert(script.name.to_ascii_lowercase())`, set becomes `HashSet<String>`) to
  match the case-insensitive script-name contract used by the rest of the VMAD/recognizer code.

---

## Decline-Invariant Audit (re-verified)

| Decline point | Verdict |
|---------------|---------|
| Handler body must be exactly the guarded `If` (#1719) | ✅ conservative |
| **Guarded `If` *inner* body → SetStage** | ❌ leaks — drops inner siblings (SCR-D5-NEW-01) |
| Unconditional body must be exactly one `SetStage` (`single_set_stage`) | ✅ conservative |
| Unmodeled guard atom → decline whole (`classify_guard_atom?`) | ✅ conservative |
| `‖` not split → one unmatchable atom (`split_and`) | ✅ conservative |
| Effect: any non-ExprStmt/Return(None)/decl/assign → None | ✅ conservative |
| Effect: unmodeled call / non-Ident assign / non-quest receiver → None | ✅ conservative |
| Hole binding `OwningQuest`/`Property`/`SelfRef` → decline, never form-0 | ✅ conservative |
| quest-of-condition vs quest-of-SetStage disagree → decline | ⚠️ checks only the *first* SetStage (consequence of SCR-D5-NEW-01) |
| `translate_pex` bad bytes → debug-log + None | ✅ conservative (3 guard tests pass) |
| Unknown event → `CanonicalEvent::Unknown` (case-insensitive) | ✅ conservative |

---

## Cross-Dimension Verdicts

| Dimension | Verdict | New findings |
|-----------|---------|--------------|
| 1 — `.pex` Reader & Opcode Decode | **CLEAN** (#1710 fix verified; transmute sound; byte-exact to Champollion) | none (#1728 coverage gap still open) |
| 2 — Decompiler CFG & Lift | **CLEAN** (copy-prop sound; prior all-green row holds) | none |
| 3 — Control-Flow / Boolean / Lower | **DIRTY** (`lower.rs`); #1732 + #1738 fixes PASS | **D3-NEW-01 (HIGH)** |
| 4 — Papyrus Lexer & Pratt Parser | **CLEAN** (#1712 + #1734 fixes verified) | none |
| 5 — Recognizer-Chain Soundness | **DIRTY** (guarded shape) | **SCR-D5-NEW-01 (HIGH, incomplete #1719)** |
| 6 — Scripting Runtime Systems | **DIRTY** (cond eval + scheduling); #1727 + #1736 + condition fns PASS | **SCR-D6-NEW-01 (HIGH)**, **SCR-D6-NEW-02 (MED)** |
| 7 — Engine Attach & Trigger Wiring | **substantially CLEAN**; #1737 fix PASS | **D7-NEW-01 (LOW)** |

---

## Future-Phase Readiness

- **Obscript/SCTX (Phase 5)**: the recognizer chain remains source-agnostic
  (`ScriptSource`); the SCR-D5-NEW-01 fix (exactly-one-statement guarded body) should
  land before SCTX exposes more authored content. The statement-depth guard (#1712) and
  expression cap already protect a future SCTX parser.
- **Fragment lowerer (b2)**: engine + dispatch + decline are ready, but the dispatch
  system is unscheduled (SCR-D6-NEW-02) — wire it together with the QUST-VMAD
  fragment-section decoder and the `RECOGNIZERS` entry (#1739).
- **Condition resolvers**: GetDistance/FactionRank/IsID/HasPerk/Global verified correct;
  GetActorValue (#1663) remains the one stub, slotting into `evaluate_function` without
  touching the (now OOB-fixed, post-SCR-D6-NEW-01) OR-precedence core.

---

## Notes on dedup

- The four still-open prior issues (#1728 BE/Starfield round-trip coverage, #1740
  decompiled-`.pex` DA10 parity, #1742 trigger-box rotation frame, #1743 `--scripts-bsa`
  order) were each re-examined and **not re-filed** — no change in situation.
- The M47.1 condition stubs #1663 (GetActorValue) and the #1316 stub-branch tracker were
  not re-filed; the other five stub branches are now implemented and verified.
