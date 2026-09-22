# Scripting Subsystem Audit — 2026-09-22

Twentieth pass over the M30/M47 scripting domain, and the first under the current 7-dimension split
(`.pex`/`.psc` frontends moved to `/audit-papyrus`; `crates/hkx` decode moved to `/audit-parsers`,
keeping only cinematic playback here). Run as a single orchestrator with **no sub-agents** (an earlier
suite run silently dropped a HIGH finding when a dimension's sub-agent result never reached its
orchestrator — see `feedback_audit_suite_nested_agent_relay`); every dimension was analysed directly,
one at a time, writing `/tmp/audit/scripting/dim_N.md` before starting the next. A prior attempt at this
exact run was killed by a server-side error early with no scratch progress saved; this is a clean
restart.

**HEAD**: `ee6d3fb39` · **Baseline**: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md` (HEAD `5278e163`) ·
**Audited (full)**: Dim 1 (Recognizer Chain), Dim 2 (Fragment/Quest-Stage/Lock Nesting), Dim 3 (Event
Runtime), Dim 4 (Engine Attach Path), Dim 5 (Scene/Package/Cinematic), Dim 6 (Legacy ObScript — first
dedicated deep pass, new code) · **Unchanged since baseline (skimmed)**: Dim 7 (Provider/Extender
Layer — zero commits since baseline; the one open question the 09-14 report left about it was resolved
by direct code reading, not a full re-derivation).

**Scope**: `crates/scripting/` (~46k LOC after the M47.3 ObScript-interpreter addition) and its
engine-side wiring (`byroredux/src/cell_loader/references/*`, `asset_provider/script.rs`,
`systems/cinematic.rs`, `boot/schedule/*`, `commands/quest.rs`). `crates/pex`/`crates/papyrus` are
`/audit-papyrus`; `crates/hkx` decode is `/audit-parsers`; AI-package procedures and
`systems/combat_ai.rs` are `/audit-gameplay`.

**Dedup baseline**: `gh issue list --state all --limit 6000` refreshed at run start (4,506 issues),
searched by keyword per finding, plus a search of `docs/audits/` (including `AUDIT_GAMEPLAY_2026-09-21.md`
and `AUDIT_CHARACTER_2026-09-21.md`, both cited by the orchestrating task as sibling findings to
extend rather than re-report).

## What changed since 2026-09-14

`crates/pex`/`crates/papyrus` are no longer this skill's territory (moved to `/audit-papyrus` in the
current SKILL.md split), so their post-baseline commits are out of scope here. Within scripting's own
territory:

| Commit | Effect on this domain |
|---|---|
| `5162829a3`, `37ed10277` | Fix #4318/#4322/#4323/#4328/#4329/#4330 — all six 09-14 recognizer/lock-ledger findings |
| `358999c40`, `c60a7b6bd` | Fix #4326/#4327/#4112/#4331 — all four 09-14 attach-path enable-gate findings |
| `c2067ae58`, `b6aa82555` | Fix #4333, #4190 — router/gate predicate sharing, ScenePlayer deep-clone |
| `ad62a5ce0` | #4340 — `apply_effect`'s 29-arm match split into 8 per-family helpers (nested-lock doc guard re-verified across the split) |
| **`6229e7d6e`** | **feat(scripting): execute compiled ObScript quest scripts (M47.3)** — new `crates/scripting/src/obscript_vm.rs` (1,101 LOC) + `obscript_quests.rs` (483 LOC): Oblivion/FO3/FNV quests now actually run their `SCDA` bytecode on the vanilla 5 s cadence. First dedicated review of this code. |
| `c1f38e3da` | Fix #4546 — lock-order reorder in `cinematic.rs` (Transform before cinematic-state guards) |
| `f1fdc0ee6`, `0f0287519` | Fix #4573/#4574 — hardened the scheduler access-declaration guard (concurrency/gameplay-owned; this dimension's own systems were unaffected, spot-checked) |
| `479163836` | New `ItemEventBatch` marker (Pattern A, compliant) |
| `2e2f40b23` | New `IsHardcore` (CTDA 586) condition function (compliant) |

Every one of the six 09-14 findings this domain filed (2 HIGH, 4 MEDIUM/LOW carried across Dims 1, 6, 7
under the old numbering) is now fixed, and each fix was independently re-verified against the source,
not just trusted from the commit message.

## Build & test state — CLEAN

```
$ cargo test -p byroredux-scripting -j 4                       480 passed; 0 failed; 1 ignored
$ cargo test -p byroredux --bin byroredux -- boot::schedule     13 passed; 0 failed
$ cargo test -p byroredux --bin byroredux -- cell_loader::references  16 passed; 0 failed; 1 ignored
$ cargo test -p byroredux-scripting -- obscript                29 passed; 0 failed; 1 ignored
$ cargo test -p byroredux-scripting -- papyrus_provider compatibility  50 passed; 0 failed
```

## Headline verdicts

### All six 2026-09-14 findings — FIXED and re-verified
`SetEnemy` now declines on any non-`(false, false)` flags; `SetInChargen` fields carry the real
Papyrus names; `StartCombat` declines a player receiver; `ShowRaceMenu`/`RequestSave` are tagged
`is_placeholder()`; the lock ledger's `SetLockLevel` records nothing on an untouched reference (rather
than inventing a keyless override) and reads back the key when a live lock exists; a non-resident
`SetLocked(false)` now records through `resolve_property_form_id`; invisible trigger volumes and
disabled actor/LIGH spawns now consult `ReferenceEnableState` ahead of every branch; both
identity-only actor-stub paths (`exterior.rs`, `materialize_scene_actor_alias_stubs`) now attach
scripts through a shared, dedup'd helper; the cinematic router calls the gate's shared eligibility
predicate instead of an inline copy; `ScenePlayer` per-frame cloning is now a 3-field `Copy` snapshot.
Full detail and code citations in each dimension's write-up (Dims 1, 2, 4, 5).

### New code this cycle — M47.3 ObScript execution — HIGH finding: two unbounded-recursion vectors
The new `obscript_vm.rs` pure bytecode interpreter has **no depth cap on either of its two recursive
routines** (nested `If`/`ElseIf` via `exec_if_chain`↔`run_arm_body`, and nested command-call arguments
via `decode_args`'s self-recursive `'X'` handling), unlike its sibling `obscript_runtime.rs`
(`MAX_LEGACY_OBSCRIPT_NESTING = 32`, added proactively when that older interpreter gained `If` support).
Both are reachable from mod-supplied `SCDA` bytecode with no realistic per-level byte cost, and a Rust
stack overflow is an uncatchable process abort — the exact class of untrusted-input hazard this domain
has hit twice before in the `.pex`/`.psc` frontends (#1712, #1815, #3783, and 09-14's own
SCR-D4-2026-09-14-01). See **SCR-D6-2026-09-22-01** (HIGH).

### Cross-audit extension: the player-targeting gap in `resolve_entity_by_global_form_id` is wider than one call site
`/audit-gameplay`'s 09-21 report (GAME-D5-2026-09-21-01) named one call site where
`resolve_entity_by_global_form_id` can never resolve the player (its `FormIdComponent` carries the
engine's reserved sentinel `local = 1`, never Bethesda's `0x14`) and asked whether other scripting-side
callers share the gap. They do: `RunOn::Reference`, `GetDistance`, `GetVMScriptVariable` (all in
`condition.rs`), quest-objective `Reference` targets (`quest_stages.rs`), and direct-FormID VMAD
property binding (`fragment/effects.rs`) all silently fail against the player, while alias fills
(a separate mechanism keyed on `SceneAliasCandidate`) are unaffected. `GetIsID` has the same root cause
reaching it through a different, non-resolver code path. See **SCR-D3-2026-09-22-01** (HIGH, extends
GAME-D5-2026-09-21-01).

## Decline-Invariant Audit (Dims 1, 6)

| Decline point | Verdict |
|---|---|
| `classify_guard_atom`/`split_and` (`||` non-split, per-atom decline) | Holds, unchanged |
| `lower_statements` three-map `bind_local`; narrow `While`/`If` exceptions; `MAX_CONDITIONAL_DEPTH` | Holds, unchanged |
| `classify_effect_with_providers` canonical-first (#3934) | Holds, unchanged |
| `translate_pex*` panic net (#1816/#3948) | Holds for panics (out of scope: the `.pex` OOM abort is now `/audit-papyrus`'s) |
| `SetEnemy` literal-only decline (#4318) | **Fixed** — was the 09-14 HIGH |
| `StartCombat` player-receiver decline (#4323) | **Fixed** |
| `SetInChargen` real Papyrus names (#4322) | **Fixed** |
| Placeholder tagging (`is_placeholder()`, #4328) | **Fixed** |
| ObScript unknown-command handling | Holds — traced no-op via `ObScriptQuestTimers.unknown_commands`, never faked |
| ObScript recursion bounds (`exec_if_chain`/`run_arm_body`, `decode_args`) | **Violated — new HIGH, SCR-D6-2026-09-22-01** |

## Runtime Lifecycle Invariant Matrix (Dims 2, 3, 4, 5)

| Invariant | Verdict |
|---|---|
| Nested-lock safety = exclusive scheduling; `apply_effect` split into 8 helpers, doc-inventory guard re-verified | Holds |
| Quest journal (3 subscribers, `set_stage` sole writer, cascade FIFO bound to `is_cascade`) | Holds, unchanged |
| M47.3 ObScript quest tick shares the same journal via `QuestStageState`'s own methods, exclusive scheduling | Holds — new code, verified correctly integrated |
| Lock ledger (#4136/#4329/#4330): outcome recorded, key survives, non-resident unlock recorded, non-resident re-lock correctly declined | **Fixed** |
| `HitEvent` two-producer overwrite (#4324) / `PhysicsWorld` hold order (#4325) | **Fixed** (gameplay-owned file, spot-checked) |
| `ItemEventBatch` (new, Pattern A) drain coverage | Compliant |
| `IsHardcore` (new CTDA 586) | Compliant, safe default |
| `RunOn::Reference`/`GetDistance`/`GetVMScriptVariable`/quest-objective-target/VMAD-property resolution of `PlayerRef 0x14` | **New HIGH, SCR-D3-2026-09-22-01** — extends GAME-D5-2026-09-21-01 |
| Trigger enable-gate coverage (triggers, actors, LIGH-only) — #4326/#4327 | **Fixed** |
| Logical-actor-stub script attach (both paths) — #4112/#4331 | **Fixed** |
| Router/gate shared eligibility predicate — #4333 | **Fixed**, narrowly and correctly scoped |
| `ScenePlayer` per-frame clone — #4190 | **Fixed** |
| Cinematic retention (#3817) | Still open, cited, unchanged |
| Papyrus-provider runtime panic net | Absent (confirmed); no reachable panic found under the four `unreachable!()` arms — traced the invariant to `ScriptValue::matches`' strict tag equality and confirmed it holds |

## Findings

**2 NEW HIGH, 0 NEW MEDIUM, 1 NEW LOW** (3 new findings total). Zero CRITICAL. Six carried-over
findings from 09-14 (2 HIGH + 4 MEDIUM/LOW) are all fixed and re-verified, not re-filed. One existing
open issue (#3817) re-verified still open, cited, not re-filed. Four other known-open issues (#4113,
#4115, #4116, #4334) unchanged, cited per the SKILL's known-open list, not re-verified in depth this
pass (no commits touched their code).

---

### HIGH

#### SCR-D6-2026-09-22-01: `obscript_vm.rs`'s two recursive routines have no depth cap — unlike the sibling `obscript_runtime.rs` (`MAX_LEGACY_OBSCRIPT_NESTING = 32`) — so mod-supplied `SCDA` bytecode can drive an uncatchable stack-overflow abort via nested `If`/`ElseIf` or nested command-call arguments

- **Severity**: HIGH
- **Dimension**: Legacy ObScript Execution
- **Untrusted-Input**: Yes — `SCDA` bytecode from mod-supplied plugins, executed every 5 s for every running quest
- **Location**: `crates/scripting/src/obscript_vm.rs::Vm::exec_if_chain` (`:288-317`) ↔
  `Vm::run_arm_body` (`:322-344`); `Vm::decode_args` (`:557-643`, the `b'X'` arm at `:621-633`
  recursing into itself at `:629`, reachable from any ordinary statement call via
  `run_or_skip_statement` at `:264`/`:276`, no `If` required). Contrast:
  `crates/scripting/src/obscript_runtime.rs:41` `MAX_LEGACY_OBSCRIPT_NESTING: usize = 32`, enforced at
  `:290`/`:523` with a passing regression test (`source_compiler_rejects_excessive_nesting`).
- **Status**: NEW. No existing issue for `obscript_vm.rs`, `exec_if_chain`, or `decode_args` (the file
  postdates every issue currently in the tracker — it landed `6229e7d6e`, 2026-09-18). Not a duplicate
  of #4113 (that finding is `.pex`/`.psc`-side, a different mechanism).
- **Description**: Both nested `If`/`ElseIf` blocks and nested inline command-call arguments are legal,
  arbitrarily-deep shapes in the on-disk compiled-script format; nothing caps how many times either
  nests inside one `SCDA` blob, only how large each individual frame's payload is (`u16`-bounded).
  Recursion depth in Rust means Rust call-stack depth, and a stack overflow aborts the process — it
  cannot be caught by `catch_unwind`, the same uncatchable-abort class the 09-14 report identified for
  the `.pex` string-amplification bug. The engine's own established lesson for this exact class
  (#3933, and the sibling interpreter's own `MAX_LEGACY_OBSCRIPT_NESTING`) was not carried into this
  new interpreter.
- **Evidence**: Derived from the code (no engine launch or new isolated `cargo` project used, per this
  run's constraints — consistent with how the 09-14 report labelled its `PhysicsWorld` lock-order
  finding "derived from the code, not observed"). Confirmed: no `MAX_*` constant anywhere in
  `obscript_vm.rs`; the only existing nested-`If` test
  (`nested_if_and_skip_do_not_leak_statements`) covers 2 levels for correctness, not depth limits; no
  `decode_args`-nesting test exists at all. Minimum per-level on-disk cost is under 20 bytes for either
  vector (an `If`/`EndIf` pair's 8-byte header + minimal expression + 4-byte `EndIf`; a `decode_args`
  `'X'` frame's 5-byte tag+cmd+len before the next nested `'X'`), so a few KB of crafted `SCDA` reaches
  hundreds of nesting levels, and the SKILL's own recorded estimate for a 64 KB blob puts it in the
  thousands.
- **Impact**: A single malicious or corrupted quest script's plugin crashes the engine process with no
  fallback, on the routine 5-second quest tick — not necessarily at load time, so a session can run for
  a while before the crafted quest happens to be reached while running.
- **Related**: #3933 (recursion-bounds precedent), `obscript_runtime.rs`'s own
  `MAX_LEGACY_OBSCRIPT_NESTING` (same crate, same class, already fixed there), SCR-D1-2026-09-14-01
  (uncatchable untrusted-input abort, different mechanism, same failure shape)
- **Suggested Fix**: Add `MAX_OBSCRIPT_VM_NESTING` (32, matching the sibling, is a reasonable start) and
  thread a depth counter through both `exec_if_chain`↔`run_arm_body` and `decode_args`'s `'X'`
  self-call, returning `BlockOutcome::Malformed` past the cap. Add two regression tests mirroring
  `obscript_runtime::tests::source_compiler_rejects_excessive_nesting`: one for `If` nesting, one for
  nested `'X'` call arguments.

#### SCR-D3-2026-09-22-01: the player-targeting gap in `resolve_entity_by_global_form_id` reaches five more scripting-side consumers, plus a structurally distinct `GetIsID` gap with the same root cause

- **Severity**: HIGH (matches GAME-D5-2026-09-21-01's rating)
- **Dimension**: Event Runtime (condition/quest-stage/fragment resolution)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/condition.rs:361-363` (`RunOn::Reference`), `:542`
  (`GetDistance`), `:828` (`GetVMScriptVariable`); `crates/scripting/src/quest_stages.rs:804-820`
  (`resolve_target_entities`, quest-objective `Reference` targets); `crates/scripting/src/fragment/
  effects.rs:88-118` (`resolve_object`, VMAD `Property { form_id, alias: -1 }` binding); root cause at
  `crates/core/src/form_id.rs:149-152` (`PLAYER_FORM_ID_PAIR.local = LocalFormId(1)`, never `0x14`,
  stamped onto the player body at `byroredux/src/scene.rs:1104-1111`); `GetIsID` at
  `condition.rs:746-768` (separate code path, same sentinel mismatch).
- **Status**: NEW facts extending **GAME-D5-2026-09-21-01** (`AUDIT_GAMEPLAY_2026-09-21.md`, not yet
  published as a GitHub issue). Not a duplicate of #2123 (closed — that fixed `RunOn::Reference` being
  *unwired* to the resolver at all; the resolver is now called correctly, but still cannot reach the
  player for `0x14` specifically). Not a duplicate of #3099 (closed — the analogous base-FormID gap for
  inventory, a different mechanism).
- **Description**: `resolve_entity_by_global_form_id` scans `FormIdComponent`-tagged entities for one
  whose interned pair's `local.0` equals the requested FormID. The player body's own pair uses the
  engine's reserved sentinel (`local = 1`), which by design can never collide with a real plugin-derived
  FormID like `0x14` — so no special-case at the *input* side can be avoided; the fix has to live in the
  resolver itself. `crates/scripting/src/package.rs` already carries this exact special-case at its two
  call sites (`reference_position`/`input_target_entity`, both substitute a passed-in `player:
  Option<EntityId>` before falling back to the resolver) — the pattern is proven, just not applied at
  the five call sites above. `GetIsID` doesn't call the resolver at all; it compares the Run-On entity's
  *own* `FormIdComponent` against `param_1` directly, so `GetIsID(0x14)` evaluated on the player itself
  returns false for the identical sentinel-mismatch reason, via a structurally different path.
- **Evidence**: Traced each call site's failure mode by reading the code: `RunOn::Reference` → `None` →
  `evaluate_condition` returns `false` unconditionally (`condition.rs:402-404`, "missing reference fails
  the predicate"); `GetDistance` → `f32::MAX` (infinite distance, so a proximity gate against the player
  always fails); `GetVMScriptVariable` → `0.0`; `resolve_target_entities` → the objective target is
  never added to the resolved set, so a quest objective pointed directly at PlayerRef (not through an
  alias) can never be tracked/completed; `resolve_object` → the fragment effect logs "object ref has no
  live entity" and declines. Ruled out: quest-alias `ForcedReference` fills, which key on
  `SceneAliasCandidate::reference_form_id` (a wholly separate mechanism) — the player is stamped with
  `SceneAliasCandidate { reference_form_id: 0x14, .. }` at spawn and resolves correctly there.
- **Impact**: Any vanilla CTDA, quest objective target, or VMAD property authored directly against
  `PlayerRef` (as opposed to through a quest alias) silently mis-evaluates across every supported game,
  with no diagnostic — the same class GAME-D5 already rated HIGH, now known to be five call sites wide
  plus a sixth structurally-distinct one.
- **Related**: GAME-D5-2026-09-21-01, #2123 (closed, different half of the same file), #3099 (closed,
  analogous gap in a different mechanism)
- **Suggested Fix**: Special-case `form_id == 0x14` inside `resolve_entity_by_global_form_id` itself
  (read the `PlayerEntity` resource and return it directly), closing GAME-D5's cited site and all five
  additional call sites in one place instead of five. Separately, give `GetIsID` a `param_1 == 0x14`
  branch that checks entity identity against `PlayerEntity` instead of `FormIdComponent`.

---

### LOW

#### SCR-D6-2026-09-22-02: `obscript_quests.rs`'s module-doc table links a nonexistent `ObScriptDiagnostics` type

- **Severity**: LOW
- **Dimension**: Legacy ObScript Execution
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/obscript_quests.rs:23`
- **Status**: NEW as a filed finding (the current SKILL.md text already names this exact gap as a dated
  observation, but no GitHub issue exists for it and no prior report filed it)
- **Description**: The doc table links `[`ObScriptDiagnostics`]`; the real counting type is
  `ObScriptQuestTimers` (`:78`, `:87`, written at `:285-292`). `ObScriptDiagnostics` does not exist
  anywhere in the workspace.
- **Impact**: Cosmetic doc-link rot only.
- **Suggested Fix**: Change the link target to `[`ObScriptQuestTimers`]`.

---

## Existing findings re-verified (not re-filed)

| Issue | Finding | State at HEAD |
|---|---|---|
| #4113 | `MAX_EXPR_DEPTH`/`MAX_REBUILD_DEPTH` uncorrelated (`.pex`/`.psc`, now `/audit-papyrus`) | Still open, not re-verified in depth (out of this skill's scope post-split) |
| #4115 | `control_flow.rs` module doc stale (`/audit-papyrus`) | Still open, not re-verified (out of scope) |
| #4116 | `ActivateEvent` consumer-order test omits `mg07_on_activate_dispatch` | Still open; no commits touched the consumer list |
| #4334 | `onlyOnce` actor-base triggers re-arm after reload | Still open, cited; the sibling `disableWhenDone` half is fixed (#4326/#4327) |
| #3817 | `HorseTetherState`/`ActorCinematicState` never terminate | Still open, cited; no commits touched retention-set lifetime |

## Future-Phase Readiness

- **M47.3 ObScript, phase 2**: the interpreter is live but needs the recursion-bound fix
  (SCR-D6-2026-09-22-01) before it is safe against untrusted plugin content the way every other
  untrusted-input parser in this codebase already is. Command-ID coverage (2,349/2,393 vanilla scripts)
  and the traced-no-op-never-faked discipline are both solid; the gap is purely the depth cap.
- **The player as a scripting target**: SCR-D3-2026-09-22-01 (and its parent GAME-D5-2026-09-21-01)
  should land as one resolver-level fix rather than being special-cased at each of six-plus call sites
  as they're discovered one at a time — `package.rs` already shows the pattern.
- **M47.1 condition resolvers**: unchanged this pass; re-verification against a live headless cell with
  real CTDA data is still outstanding (no engine launch this cycle, per this run's constraints).
- **The SDK/extender layer**: Dim 7 remains a large (~10k LOC scripting-side) surface with only its
  seams re-checked each pass. This pass resolved one specific open question (no panic net around
  runtime execution, but no reachable panic under it today) — a future pass should still budget a full
  first-principles review, the way this pass finally gave the ObScript VM one.

## Findings Count

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 2 |
| MEDIUM | 0 |
| LOW | 1 |
| **Total (new)** | **3** |

New findings by dimension: Dim 3 (1 HIGH), Dim 6 (1 HIGH, 1 LOW). Dims 1, 2, 4, 5, 7 found no new
issues — every carried-over finding is fixed and re-verified, and no regression was found in any of the
verified fixes.

---
*Generated by the `/audit-scripting` skill (single-agent run, no sub-agents). Suggested next step:*
`/audit-publish docs/audits/AUDIT_SCRIPTING_2026-09-22.md`
