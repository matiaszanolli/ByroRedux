# Scripting Subsystem Audit — 2026-07-03

**Scope**: M30 `.psc` parser (`crates/papyrus`), M47.2 `.pex` reader + 5-phase
decompiler (`crates/pex`), the AST→ECS recognizer chain + ECS scripting runtime
(`crates/scripting`), and the engine-side cell-loader attach path
(`byroredux/src/cell_loader/references.rs`, `byroredux/src/asset_provider/script.rs`).

**Depth**: deep · **Dimensions**: all 7 · **HEAD**: `8498e559`.

This is the fourth scripting audit (prior: 2026-06-23, 2026-06-27, 2026-07-02).
HEAD is only 3 commits past the 07-02 audit (two test-hardening commits + one
skill-doc refresh — no functional scripting changes), so this pass verifies all
four residual 07-02 findings against the current tree and then goes hunting for
new defects in code paths the first three passes didn't examine as closely:
the `Globals` (#1668) resource lifecycle and the `QuestStageAdvanced`
same-frame multi-producer path. Both turned up a real, previously-unreported
defect class — a shared single-entity event sink silently overwrites earlier
same-frame events — plus a resource-refresh asymmetry between the interior and
exterior cell-load paths.

---

## Executive Summary

**What shipped** (re-verified against source, unchanged from 07-02): M30.2
`.psc` lexer + Pratt parser with dual recursion caps; M47.2 `.pex` reader +
5-phase decompiler (cfg → lift+copy-prop → boolean-collapse → control-flow →
lower), now with a recursion cap on **every** untrusted tree walk including the
boolean pass; the decline-on-unmodeled recognizer chain; the ECS runtime
(events/timers/conditions/triggers/quest-stages/fragments/recurring-updates);
the dynamic VMAD attach path + XPRM trigger volumes; a `Globals` (#1668) World
resource resolving CTDA "Use Global" comparands.

**Verified fixed since 07-02** (all four residual findings from that pass):

| 07-02 finding | Fix commit | Verified at |
|---|---|---|
| SCR-D2-01 (boolean-pass unbounded recursion) | `7fdb694b` (#1815) | `boolean.rs:39,97-102,120` `MAX_REBUILD_DEPTH` threaded through `rebuild`, depth-checked before any CFG access |
| SCR-D5-NEW-02 (`translate_pex` no panic net) | `8b04c492` (#1816) | `translate/mod.rs:104-116` wraps `decompile_script` in `catch_unwind(AssertUnwindSafe(...))`, maps to the same `None` as an `Err` |
| SCR-D6-NEW-01 (feature-matrix CTDA count) | `1d3190fb` (#1818) | `docs/feature-matrix.md:137` now says 13 |
| SCR-D5-03 (no `.pex`-side DA10 byte-equality gate) | `2f0b99fa` (#1740) | `crates/scripting/tests/pex_recognize_e2e.rs::da10_pex_reproduces_hand_builder_byte_for_byte`, asserts every field against the same `da10_main_door` hand-builder the `.psc` path uses |

**Still open, correctly tracked (not re-filed)**: **#1817** (SCR-D6-NEW-02,
`occupant_inside` not seeded from initial containment — confirmed still
hardcoded `false` at `references.rs:1476`), **#1769** (VMAD attach dedup is
case-sensitive — confirmed at `references.rs:1621,1627`, a raw-case
`HashSet<&str>`), **#1743** (`--scripts-bsa` override order is
first-listed-wins — confirmed at `asset_provider/script.rs:39,71`), **#1742**
(trigger-box rotation-frame ambiguity). **#1739** (fragment lowerer
unpopulated pending the QUST-VMAD decoder) remains a documented Phase-3 design
gap, not a defect.

**New this pass**: two MEDIUM defects and one LOW doc-integrity issue —
see Findings below. None are untrusted-input-reachable; all are ECS
runtime-lifecycle bugs in the quest-stage-advance / GLOB-resource paths.

### Untrusted-input robustness verdict — **CLEAN**

Every decompiler tree walk that recurses on attacker-controlled structure now
carries a depth cap (`cfg`'s jump-bounds check, `lift`'s copy-prop restart
bounded by scope size, `control_flow::Reconstructor::rebuild`
`MAX_REBUILD_DEPTH`, and now `boolean::BoolPass::rebuild` `MAX_REBUILD_DEPTH`),
and `translate_pex` catches both a `decompile_script` `Err` and a
`decompile_script` panic. The one theoretical gap noted by every prior audit
still applies and is out of scope for a caught-`panic!`: a stack overflow is
not caught by `catch_unwind` regardless of the wrapper — but with the boolean
pass now capped, no known reachable recursion path can trigger one. The `.psc`
parser (`MAX_EXPR_DEPTH`/`MAX_STMT_DEPTH`) and the `.pex` reader (`take`'s
single bounds gate, geometric-growth var-arg reads, the `#[repr(u8)]`
contiguous-discriminant opcode `transmute`) remain sound.

### 99.996% decompile-rate claim — unchanged, still **VERIFIED HONEST**

No change to `pex_corpus_smoke.rs` since 07-02; re-confirmed the harness counts
both `Err` and a caught panic as failures, `Ok(Ok(_))` alone as success.

### `.psc`-vs-`.pex` fidelity gate — now **FULLY CLOSED (both sides)**

`quest_stage_gate.rs::recognizes_da10_and_reproduces_hand_builder` (`.psc`
side) and `pex_recognize_e2e.rs::da10_pex_reproduces_hand_builder_byte_for_byte`
(`.pex` side, #1740, `#[ignore]`-gated on Skyrim SE game data) both assert
field-level equality against the same `da10_main_door(...)` hand-builder. This
closes the gap the 07-02 report flagged as still one-sided.

---

## Runtime Lifecycle Invariant Matrix (delta from 07-02)

| Invariant | State |
|-----------|-------|
| Edge-triggered enter seed | ❌ still unseeded — **Existing: #1817** |
| **Same-frame multi-event sink collision** | ❌ **NEW** — `QuestStageAdvanced` markers from >1 simultaneous quest-advance / cascade in one frame collapse onto one shared entity; only the last survives (see SCR-D7-NEW-01 below) |
| **GLOB resource refresh consistency** | ⚠️ **NEW** — interior cell load unconditionally rebuilds `Globals`; exterior streaming guards with `is_none()` (see SCR-D6-NEW-03 below) |
| Everything else in the 07-02 matrix (marker-drain coverage, two-phase lock-drop, cascade bound, CTDA OR-precedence, scheduler registration) | ✅ unchanged, re-spot-checked, still holds |

---

## Findings

### SCR-D7-NEW-01: `QuestStageAdvanced` markers collide on a shared single-entity sink — simultaneous same-frame quest advances silently overwrite all but the last
- **Severity**: MEDIUM
- **Dimension**: Scripting Runtime Systems / Engine Attach & Trigger Wiring
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/papyrus_demo/quest_advance.rs:304-330`
  (`quest_advance_system`, phases 2–3) and
  `crates/scripting/src/fragment.rs:197-240`
  (`quest_fragment_dispatch_system`'s chained re-emission)
- **Status**: NEW
- **Description**: `QuestStageAdvanced` is declared
  `impl Component for QuestStageAdvanced { type Storage = SparseSetStorage<Self>; }`
  (`crates/scripting/src/quest_stages.rs:267-269`) — one component instance per
  entity, and `SparseSetStorage::insert` **overwrites in place** on a repeat
  insert to the same entity
  (`crates/core/src/ecs/sparse_set.rs:65-67`: `if let Some(dense_idx) = ...
  { self.data[dense_idx] = component; }`, confirmed by the
  `insert_bulk_duplicate_ids_last_writer_wins` regression test on the sibling
  packed storage). Both live producers of this marker write **multiple**
  events onto the **same** fixed sink entity (`PlayerEntity`) within one system
  call:
  - `quest_advance_system` builds `advances_emitted: Vec<QuestStageAdvanced>`
    from every `(entity, triggerer)` pair whose `ActivateEvent` /
    `OnTriggerEnterEvent` this frame resolves to a `QuestAdvanceOnActivate`
    that passes its gate + condition list (phase 1, lines 248-291) — this is
    legitimately >1 whenever two different scripted doors/triggers fire in
    the same tick (e.g. the player activates one door while an NPC crosses an
    unrelated trigger volume, or two players... single-player but two AI
    actors independently cross two different volumes on the same tick). Phase
    3 then does `for ev in advances_emitted { q.insert(player_entity, ev); }`
    (lines 328-330) — only the **last** `ev` survives.
  - `quest_fragment_dispatch_system`'s cascade re-emission does the identical
    pattern for `chained: Vec<QuestStageAdvanced>` (lines 236-239), gated
    behind the not-yet-populated `QuestStageFragments` (#1739).
- **Evidence**: `crates/core/src/ecs/sparse_set.rs:65-67`; the doc comment on
  `advances_emitted`'s emission (`quest_advance.rs:317-324`) explicitly
  acknowledges the single-sink design ("We co-opt the `PlayerEntity` target
  here... until a dedicated `QuestEventBus` entity lands") but does not
  address that >1 event in the same `Vec` collapses to one observable value.
  `crates/scripting/src/fragment/tests.rs::dispatch_cascades_chained_set_stage`
  only asserts the **canonical state** (`QuestStageState`/`QuestObjectiveState`,
  which is unaffected — `apply_effects` writes those through a `HashMap`, not
  an ECS single-instance component) and never inspects the re-emitted marker
  content, so the collision is untested and would pass CI silently.
- **Impact**: The authoritative quest state (`QuestStageState`, what
  `GetStage`/`GetStageDone` conditions actually read) is **not** corrupted —
  every pending advance's `set_stage` call runs before the marker-emission
  step, so game logic that re-evaluates conditions next frame sees the
  correct stage. The defect is confined to the **notification** layer: any
  future consumer that reacts to the specific `(quest, previous_stage,
  new_stage)` payload of a `QuestStageAdvanced` event — a journal-update UI,
  further-frame quest-fragment dispatch, telemetry — observes only the last
  of N same-frame advances and silently misses the others. Currently
  zero-observable in practice because the only two consumers are
  `event_cleanup_system` (drains unconditionally, doesn't read the payload)
  and `quest_fragment_dispatch_system` (a no-op today per #1739) — but the
  `quest_advance_system` half of this bug is **already live and wired** in
  the engine scheduler (`byroredux/src/main.rs:765`), reachable the moment two
  independently-recognized quest-advance REFRs fire in one frame, independent
  of the fragment-decoder gap.
- **Related**: Architecturally adjacent to #1817 (both are event-fidelity
  gaps in the trigger/quest-advance runtime slice) but a distinct root cause
  (single-sink overwrite vs. missing containment seed). Not a duplicate of
  any tracked issue (checked `#1739`, `#1742`, `#1743`, `#1769`, `#1817` — none
  address multi-event collision).
- **Suggested Fix**: Either (a) fan the marker out across per-source
  entities instead of one shared sink (each `QuestAdvanceOnActivate`-bearing
  entity already exists — insert the resulting `QuestStageAdvanced` there
  instead of on `player_entity`), or (b) switch the sink to an
  accumulating resource (`Vec<QuestStageAdvanced>` behind a `Resource`,
  drained wholesale) rather than a single-instance ECS component keyed by one
  fixed entity. Add a regression test that emits two advances for two
  different quests in one frame and asserts both are observable before
  cleanup.

### SCR-D6-NEW-03: `Globals` (#1668) resource is unconditionally rebuilt on every interior cell load but guarded on the exterior streaming path — asymmetric, a landmine for the pending `SetGlobalValue` writer
- **Severity**: MEDIUM
- **Dimension**: Scripting Runtime Systems
- **Untrusted-Input**: No
- **Location**: `byroredux/src/cell_loader/load.rs:372-378` vs.
  `byroredux/src/cell_loader/exterior.rs:268-279`
- **Status**: NEW
- **Description**: `crates/scripting/src/globals.rs` mirrors parsed `GLOB`
  values into a runtime `Globals` World resource specifically so a future
  `SetGlobalValue`-style Papyrus call has "a home" to mutate (the module doc:
  "Globals are script-mutable at runtime; the map is therefore owned... so
  `SetGlobalValue`-style mutations have a home"). The two cell-load call
  sites disagree on refresh policy: `exterior.rs` explicitly guards —
  `if world.try_resource::<Globals>().is_none() { world.insert_resource(...) }`
  — with a comment explaining why ("build the lean `Globals` map only when it
  isn't already present rather than rebuilding it each cell"), preserving any
  runtime mutation across the many-cells-per-session exterior streaming loop.
  `load.rs`'s interior path calls `world.insert_resource(Globals::from_records(&index.globals))`
  **unconditionally** on every interior cell load — including a door
  transition between two interior cells, or a return trip through the same
  door — silently discarding any accumulated runtime mutation and resetting
  every GLOB back to its ESM-parsed default.
- **Evidence**: `grep -rn "Globals::set\|SetGlobalValue" crates/ byroredux/`
  returns only the two `globals.rs`/`condition.rs` unit tests — **no
  production code path currently calls `Globals::set`**, so this asymmetry has
  zero observable effect today (both call sites currently rebuild from the
  same static ESM data, so an unconditional vs. guarded insert produces an
  identical map either way). The defect is latent: the moment a
  `SetGlobalValue` runtime write lands (the exact scenario `globals.rs`'s doc
  comment is building toward), every interior-cell transition will silently
  revert player-mutated GLOBs, while the exterior path will correctly
  preserve them — an inconsistency that will be much harder to root-cause
  once real mutations exist than it is to fix now.
- **Impact**: None today (dormant). Once a live `SetGlobalValue` write path
  exists, this becomes a real, silent game-state reset on every interior cell
  transition — e.g. a quest-scripted GLOB toggle set by the player would
  revert the instant they walk through an interior doorway, while the same
  toggle would correctly persist across exterior worldspace streaming.
- **Related**: Not tracked by any open issue (searched for "glob"/"GLOB" in
  the issue list — only #1805, an unrelated static-mesh performance finding,
  matched).
- **Suggested Fix**: Make `load.rs`'s interior insert conditional on
  `try_resource::<Globals>().is_none()`, mirroring `exterior.rs` exactly (both
  paths already source from the same `index.globals` / `wctx.record_index.globals`
  map, so the fix is a one-line guard, not a behavior redesign). Low urgency
  given the current dormancy, but cheap to fix now rather than after
  `SetGlobalValue` lands and the regression becomes live and hard to spot.

### SCR-D6-NEW-04: `TriggerVolume::occupant_inside` doc comment describes the #1817 fix as already implemented — misleading, compounds the open bug
- **Severity**: LOW
- **Dimension**: Scripting Runtime Systems (documentation)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/trigger.rs:51-55` vs.
  `byroredux/src/cell_loader/references.rs:1476`
- **Status**: NEW (compounds **Existing: #1817**)
- **Description**: The `occupant_inside` field doc comment (introduced in the
  original M47.2 trigger-detection commit, `1712959b`) reads: *"Seeded `true`
  for a player that loads *already inside* the volume so a quest trigger the
  player is standing on at cell entry doesn't fire spuriously on frame 1... The
  cell loader seeds this from the player's spawn position."* This describes
  the exact fix #1817 asks for — but the spawn site added in a later commit
  (`c8c8e5e9`, `trigger_volume_from_primitive`) hardcodes
  `occupant_inside: false` unconditionally and never reads the player's spawn
  position. The two commits never reconciled: the runtime module documents
  a contract the cell-loader spawn path never fulfilled.
- **Evidence**: `git log --oneline -S"occupant_inside: false" --
  byroredux/src/cell_loader/references.rs` → `c8c8e5e9`; the doc comment
  predates that commit (present since `1712959b`, the module's introduction).
- **Impact**: Documentation only, but actively misleading: a future engineer
  grepping `trigger.rs` for the seeding contract would read "the cell loader
  seeds this" and reasonably conclude #1817 is already fixed, potentially
  causing them to look for the spurious-fire bug in the wrong place (the
  detection system, which is correct) instead of the actual gap (the spawn
  site, which never wired it).
- **Related**: **Existing: #1817** — this is a doc-accuracy note on the same
  underlying defect, not a separate code bug; fix in the same PR as #1817.
- **Suggested Fix**: When #1817 lands, no comment change needed (the doc will
  become accurate). Until then, either implement the seeding now or soften
  the doc comment to "intended to be seeded... — see #1817" so it doesn't
  read as already-fixed.

---

## Decline-Invariant Audit — unchanged, re-spot-checked

Re-verified `compose::classify_guard_atom`'s per-atom `?`-decline,
`split_and`'s deliberate `||`-non-split, `effects::lower_fragment`'s
control-flow decline, and the `quest_stage_gate` quest cross-check all still
match the 07-02 report's findings — no regression. The load-bearing invariant
(no recognizer emits a component on a partial/approximated match) holds.

---

## Confirmed-fixed prior-audit findings (all three prior passes)

Cumulative across 2026-06-23 / 06-27 / 07-02, every actionable finding except
the four still-open issues above is fixed and verified in place at this HEAD
— see the 07-02 report's own "Confirmed-fixed" table for the full list
(var-arg OOM #1710, nested-statement recursion #1712, recovery-to-EOF #1734,
`||`-skip silent drop #1732, control-flow recursion #1729, guarded-sibling
drops #1719/#1766, `name[..2]` panic #1765, `or_next` OOB #1767, scheduler
registration #1768, marker-drain gaps #1727, per-REFR VMAD #1737,
feature-matrix doc-rot); this pass adds SCR-D2-01 (#1815), SCR-D5-NEW-02
(#1816), and SCR-D6-NEW-01 (#1818) to the confirmed-fixed list.

---

## Future-Phase Readiness

- **Obscript / SCTX (Phase 5)**: unchanged — `attach_scpt_script` is the live
  pre-Skyrim mechanism, correctly still wired.
- **Fragment lowerer (b2)**: unchanged — proven but unpopulated pending #1739.
  **New consideration**: once #1739 lands, fix SCR-D7-NEW-01 *before* or
  *alongside* it — the fragment cascade's re-emission half of that bug is
  currently dormant specifically because `QuestStageFragments` is empty, and
  will start silently dropping cascade notifications the day it isn't.
- **GLOB mutation (#1668 follow-up)**: `Globals::set` has no production
  caller yet. When `SetGlobalValue` (or any other write path) is wired, fix
  SCR-D6-NEW-03 first — it's a one-line guard, and the bug becomes much
  costlier to diagnose once real mutations are flowing.
- **Condition resolvers (#1663–#1668, #1316)**: unchanged — 13-function
  catalog, remaining stubs return documented safe-defaults.

---

## Summary

**Total findings: 3** — HIGH 0, MEDIUM 2 (SCR-D7-NEW-01, SCR-D6-NEW-03), LOW 1
(SCR-D6-NEW-04). All NEW. All four residual findings from the 07-02 audit were
re-verified: three fixed (#1815, #1816, #1818), one still open and correctly
tracked (#1817, not re-filed). The untrusted-input robustness verdict is now
**CLEAN** (the last recursion-cap gap, SCR-D2-01, is closed) and the
`.psc`-vs-`.pex` fidelity gate is now fully closed on both sides (#1740). The
two new MEDIUM findings are both same-day-reachable ECS lifecycle bugs
(same-entity event-marker collision; an interior/exterior resource-refresh
asymmetry) rather than untrusted-input hazards — neither has live observable
impact today (no consumer reads the collided marker payload; no production
writer mutates `Globals` yet), but both are real defects in already-shipped,
already-wired code that will become active the moment their respective
future features (#1739's fragment population; a `SetGlobalValue` write path)
land, and are cheap to fix now while dormant.
