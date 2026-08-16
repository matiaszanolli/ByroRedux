# Scripting Subsystem Audit — 2026-08-16

Twelfth full pass over the M30/M47 Papyrus / `.pex` / ECS scripting domain
(prior reports: `AUDIT_SCRIPTING_2026-06-23.md`, `_06-27.md`, `_07-02.md`,
`_07-03.md`, `_07-06.md`, `_07-16.md`, `_07-21.md`, `_07-25.md`, `_08-03.md`,
`_08-07.md`, `_08-12.md`). Run as part of the `comprehensive` audit-suite sweep,
single-agent (no sub-agents), covering `crates/pex/`, `crates/papyrus/`,
`crates/scripting/`, `crates/hkx/`, and the engine-side attach + runtime-install
path (`byroredux/src/cell_loader/references/`, `byroredux/src/cell_loader/load.rs`,
`byroredux/src/asset_provider/{script,animation}.rs`, `byroredux/src/scene.rs`,
`byroredux/src/app_step.rs`, `byroredux/src/debug_load.rs`, `byroredux/src/boot.rs`).

**Scope note — Dimension 8 is a first pass.** `crates/hkx` has never been
examined by any prior report in this series
(`grep -l "crates/hkx" docs/audits/AUDIT_SCRIPTING_*.md` returns nothing).
Four of this pass's ten findings come from it.

**Dedup baseline**: `/tmp/audit/issues.json` (269 open issues, fetched
2026-08-16) plus a full-history `gh issue list --state all --search "SCR-"`
sweep (89 SCR-* issues, 10 open), plus direct reads of
`AUDIT_SCRIPTING_2026-08-12.md`. Every finding below was grepped against both.

**Test baseline** (all green, this tree, `main` @ `adbc3f77` + working tree):
`cargo test -p byroredux-pex` — 55 unit + 1 doc, 0 failed, 1 ignored
(`da10_main_door_decompiles_to_the_r5_reference_shape`).
`cargo test -p byroredux-papyrus` — 90 unit + 4 integration, 0 failed.
`cargo test -p byroredux-hkx` — **4** unit, 0 failed (see SCR-D8-2026-08-16-04:
one of the four is vacuous without game data).
`cargo test -p byroredux-scripting` — 288 passed, 0 failed, 3 ignored
(`pex_recognize_e2e`, game-data-gated) + 2 ignored doc-tests.

## What changed since 2026-08-12

Eight of the twelve `SCR-*` findings filed on 2026-08-12 were fixed and closed in
this window — `635e1d18` (#2664–#2667), `d48193db` (#2660–#2663), `20d74b05`
(#2656–#2659), plus #2653/#2654/#2655. All were spot-re-verified in source
during this pass and none has regressed:

- `crates/pex/`: `boolean.rs` **+231** (the #2667 local-decline rewrite + the new
  self-referential-edge guard), `node.rs` **+167** (its first tests — a
  `child_nodes`/`child_nodes_mut` parity check over all 16 `NodeKind` variants
  behind an exhaustive match), `lift.rs` **+21** (`debug_assert!` → fail-closed
  `ExpressionRebuildFailed`), `mod.rs` **+17** (`RecursionLimit { pass }`).
- `crates/papyrus/`: `parser/script.rs` **+142** (the #2656 property-flag
  newline-crossing fix).
- `crates/scripting/`: `fragment.rs` (`DeferredFragmentEffects`, the
  pre-lock snapshot that closed #2660), `quest_stages.rs` (the sequenced
  quest-event journal), `scene.rs` split into `scene/{playback,quest_alias}.rs`,
  `condition.rs` (#2933 `DerivedOutput::Absolute` gate).
- `byroredux/src/cell_loader/references/`: split into `complete.rs` +
  `synth_child.rs` + six `*_tests.rs` siblings (`64fed2c4`).

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
behavior-graph execution (`crates/hkx` deliberately decodes only, and that
scope claim still holds — nothing in the crate walks a behavior graph).

**Findings this pass: 10 new — 0 CRITICAL / 2 HIGH / 6 MEDIUM / 2 LOW.**
Per dimension: **Dim 1 — 0. Dim 2 — 0. Dim 3 — 2 (1 MEDIUM, 1 LOW). Dim 4 — 0.
Dim 5 — 0. Dim 6 — 1 MEDIUM. Dim 7 — 3 (1 HIGH, 2 MEDIUM). Dim 8 — 4 (1 HIGH,
2 MEDIUM, 1 LOW).**

The two HIGHs are both *wiring*, not logic: the M47.2/M47.3 fragment pipeline is
inert on every exterior launch because its one populator is called from one of
four runtime-install sites (SCR-D7-2026-08-16-01), and the never-before-audited
Havok reader has an unbounded `Vec::with_capacity` driven by a raw `u32` file
field (SCR-D8-2026-08-16-01).

**Untrusted-input robustness verdict — CLEAN for `.pex` and `.psc`, NOT CLEAN
for `.hkx`.** No panic, OOB index, or unbounded allocation is reachable from
hostile `.pex` or `.psc` bytes: every `.pex` primitive read funnels through
`take()`; the `OpCode::from_u8` transmute guard is `>=` over contiguous `0..=50`
discriminants with full 51-value coverage; hostile var-arg counts never feed
`Vec::with_capacity` (#1710); all four recursion caps
(`MAX_REBUILD_DEPTH = 1024` ×2, `MAX_EXPR_DEPTH`/`MAX_STMT_DEPTH = 256`) are
present and threaded; `translate_pex` still `catch_unwind`s the decompiler. The
`.hkx` path does **not** meet that bar — see SCR-D8-2026-08-16-01, which is a
`handle_alloc_error` abort (not even a catchable panic) from one unvalidated
`u32`.

**The 99.996% decompile-rate claim — HONEST but still robustness-only.**
`crates/pex/examples/pex_corpus_smoke.rs:142-160` genuinely calls
`decompile_script` inside `catch_unwind` and feeds both the panic arm and the
`Err` arm into the failure tally. But `Ok(Ok(_)) => stats.decompiled_ok += 1`
discards the resulting `Script` with no shape check, so the rate measures
robustness, not fidelity — unchanged from 2026-08-12, and now honestly
documented in `boolean.rs`'s own module doc.

**The `.psc`-vs-`.pex` fidelity gate — still does not execute.** The `.psc` half
(`recognizes_da10_and_reproduces_hand_builder`) passes and pins byte-equality
against `da10_main_door(...)`, but never touches `decompile_script`. Both `.pex`
halves remain `#[ignore]`-gated on Skyrim SE data and did not run. Filed this
pass as SCR-D3-2026-08-16-01, with a concrete fix (`PexWriter` already builds
hand-made `.pex` in-tree).

## Decompiler Soundness Matrix

| Pass | Bounds-safe | Terminates | Total (no panic) | Fidelity-tested |
|------|:---:|:---:|:---:|:---:|
| Reader (`reader.rs`) | Yes | Yes | Yes | Yes (5 negative-path tests) |
| CFG (`cfg.rs`) | Yes | Yes | Yes | Yes |
| Lift + copy-prop (`lift.rs`) | Yes | Yes (#2024 linear chain intact) | Yes | Yes — the release-build producer-drop hole is now a fail-closed `Err` (#2666) and `node.rs` has a variant-parity test |
| Boolean (`boolean.rs`) | Yes | Yes (`MAX_REBUILD_DEPTH`, own `pass` tag) | Yes — both inherited `.expect`s are declines, plus a new self-referential-edge guard (#2667) | Partly — the #2655 `falls_through_to_rejoin` requirement is the fix; no *executing* end-to-end gate (SCR-D3-2026-08-16-01) |
| Control-flow (`control_flow.rs`) | Yes | Yes, same cap | Yes (fail-closed #1732 intact) | Partly — same gap |
| Lower (`lower.rs`) | Yes | Yes | Yes | Partly — same gap. `lower_binary_op`'s `_ => Eq` default arm re-confirmed structurally unreachable for the sixth consecutive pass |

The two documented Champollion departures are adjudicated **benign as
currently guarded**: departure 1 (no debug-line guard) is now carried
structurally by `collapse`'s `falls_through_to_rejoin` requirement, and
departure 2 (termination guard) is correct for the iterative loop with
`MAX_REBUILD_DEPTH` bounding the recursion the module doc now admits it does
not cover.

## Decline-Invariant Audit

| Decline point | Verdict |
|---|---|
| `classify_guard_atom` `?` inside `classify_if_condition`'s per-atom loop | Conservative — an unclaimed atom propagates `None`, no silent skip |
| `split_and` refusing to split `\|\|` | Conservative, deliberate |
| `lower_fragment`'s `_ => return None` statement arm | Conservative; the one `Stmt::While` exception is still exactly `lower_3d_loaded_wait` (OR-tree of `!Is3DLoaded` + one positive `Utility.Wait`) |
| `receiver_object` | Conservative — explicit `key == "self"` plus `quest_locals` / `player_locals` / `decl_locals` / `known_quest_properties` rejections (#2538/#2657 both live) |
| `receiver_quest` → `quest_via` | Conservative for the shared-method-name primitives: `prim_reset_quest` / `prim_set_quest_active` now use `explicit_quest_receiver` (#2653 fix verified in source) |
| `bool_arg` three-case contract | Conservative — present-but-non-literal declines the whole primitive |
| `AddItem` 4th-arg / `MoveTo` offset-arg declines | Intact |
| `translate_pex` on bad bytes **or** a decompiler panic | Clean `None`, `catch_unwind` still present |
| `QuestRef::Property` on an alias-bound entry | Still declines (correct — quests are not alias-fillable) |
| `SceneActorBindings::resolve` on an unfilled alias | Returns `None`, never fabricates an entity |
| FO4 `ALCS` collection aliases | Excluded from the single-entity fill loop (#2661) |

No leak found. Dimension 5 is clean this pass.

## Runtime Lifecycle Invariant Matrix

| Invariant | Verdict |
|---|---|
| Marker drain coverage (`event_cleanup_system`) | 14 markers drained; the 10 batch/request components that self-drain in their consumer are #2672, still open, not re-filed |
| Two-phase lock-drop — `timer_tick_system`, `recurring_update_tick_system` | Explicit `drop()` before the second acquisition |
| Two-phase lock-drop — `trigger_detection_system` | Block-scoped phase 1, phase 2 after |
| `quest_fragment_dispatch_system` clone-before-lock | Intact; `DeferredFragmentEffects` now also snapshots `QuestDefinitionRegistry` (Arc bump) and `SceneActorBindings` before the guards (#2659/#2660) |
| Residual nested component locks inside `apply_effect` | Documented in-source with its exclusive-scheduling justification; every quest-resource system is `add_exclusive` in `boot.rs` — verified |
| Cascade bound | `MAX_CASCADE = 64` with WARN; #2124 guard compares `adv.previous_stage != adv.new_stage` |
| Quest-event journal | **Defect — polled destructively before the `frags.is_empty()` bail (SCR-D6-2026-08-16-01)** |
| Edge-trigger seed (`occupant_inside: None`) | Intact at both the producer (`trigger_volume_from_primitive`) and the consumer |
| CTDA OR-precedence | Intact (block scan, `.any()`, early-return on a false block, empty list → `true`) |
| `set_stage` history retention | Intact |

## Findings

### HIGH

#### SCR-D7-2026-08-16-01: `populate_quest_fragments` runs at one of four runtime-install sites — every exterior launch has an empty `QuestStageFragments`, so no QF_ fragment ever executes

- **Severity**: HIGH
- **Dimension**: Engine Attach & Trigger Wiring (Dimension 7)
- **Untrusted-Input**: No
- **Location**: `byroredux/src/cell_loader/load.rs:441` (the only call site);
  the three sites that lack it —
  `byroredux/src/scene.rs:607`, `byroredux/src/app_step.rs:789`,
  `byroredux/src/debug_load.rs:392`; producer
  `byroredux/src/asset_provider/script.rs:85-149`; the silent consumer bail at
  `crates/scripting/src/fragment.rs:1575-1577`
- **Status**: NEW
- **Description**: `populate_scene_runtime`
  (`byroredux/src/asset_provider/script.rs:159-197`) is the engine's
  quest-runtime install hook: it installs `StartGameQuestRegistry`, the Skyrim
  MQ101 `install_engine_start_quest` bootstrap, `install_scene_quest_aliases`,
  `install_scene_records`, the IMAD table and the equip catalog. It is called
  from four places — the interior cell loader, the exterior world bootstrap, the
  exterior worldspace transition, and the `byro-dbg` exterior load.
  `populate_quest_fragments` — the **only** writer of `QuestStageFragments`
  anywhere in the engine — is called from the interior one alone.
  On any exterior session the quest lifecycle therefore comes up fully (quests
  start, stages advance, the journal records transitions, aliases fill) while the
  fragment table stays empty, and `quest_fragment_dispatch_system` bails at its
  `frags.is_empty()` guard. Every `QF_*` stage fragment the M47.2 lowerer
  produces is inert outdoors.
- **Evidence**:
  ```
  $ grep -rn "populate_scene_runtime\|populate_quest_fragments" byroredux/src | grep -v tests
  byroredux/src/asset_provider/script.rs:85:pub(crate) fn populate_quest_fragments(
  byroredux/src/cell_loader/load.rs:441:    crate::asset_provider::populate_quest_fragments(world, &index);
  byroredux/src/cell_loader/load.rs:507:    crate::asset_provider::populate_scene_runtime(world, &index);
  byroredux/src/scene.rs:607:                    crate::asset_provider::populate_scene_runtime(world, &wctx.record_index);
  byroredux/src/app_step.rs:789:                        crate::asset_provider::populate_scene_runtime(
  byroredux/src/debug_load.rs:392:    crate::asset_provider::populate_scene_runtime(world, &wctx.record_index);
  ```
  `load.rs:441`/`:507` are both inside `load_cell_with_masters` — the interior
  path. `install_engine_start_quest` for MQ101 (`script.rs:186-197`) runs on the
  exterior path, so the Skyrim opening quest *does* start there and its stage-0
  fragment *is* the thing that never runs.
- **Impact**: Silent and total for the exterior half of the game. No log line
  exists on either side: `populate_quest_fragments` logs only on success
  (`total > 0`), and the dispatcher's `frags.is_empty()` return is silent. The
  smoke gate cannot catch it — `docs/smoke-tests/m47-triggers.sh` launches with
  `--cell`, i.e. the one path that does call the populator. This makes every
  "0% real yield" / "no fragments dispatched" measurement taken on an exterior
  cell an artifact of wiring rather than of recognizer coverage.
- **Related**: SCR-D6-2026-08-16-01 (the destructive poll that makes the loss
  unrecoverable rather than deferred); #2541
- **Suggested Fix**: Move the `populate_quest_fragments` call inside
  `populate_scene_runtime` so it cannot drift from its three siblings again —
  it already takes the same `(world, &EsmIndex)` pair and is already idempotent
  ("re-registering a `(quest, stage)` on a later load simply overwrites with the
  identical lowering"). Add a source-pin test of the same shape as
  `byroredux/src/cell_loader/exterior.rs`'s existing
  `SRC.contains("references::spawn_logical_quest_reference(")` pin.

#### SCR-D8-2026-08-16-01: `HkxAnimation::num_frames` is an unvalidated `u32` that drives a `Vec::with_capacity` — a crafted `.hkx` aborts the process

- **Severity**: HIGH
- **Dimension**: Havok Idle / Cinematic Slice (Dimension 8)
- **Untrusted-Input**: **Yes**
- **Location**: `crates/hkx/src/animation.rs:126` (read), `:131-145` (the
  validation block), `:231` (the allocation), `:232-239` (the sample loop);
  consumer `byroredux/src/asset_provider/animation.rs:83`
- **Status**: NEW
- **Description**: `decode_spline_animation` validates almost every other
  dimension it reads — `transform_count <= 4096`, `float_count <= 4096`,
  `num_blocks <= 4096`, `max_frames_per_block >= 2`, `mask_size ==
  transform_count * 4 + float_count`, finite non-negative `duration`, positive
  `frame_duration`. `num_frames` gets only `num_frames == 0`. Nothing
  cross-checks it against `duration`/`frame_duration` or against
  `num_blocks * (max_frames_per_block - 1)`, because `:233`'s
  `.min(num_blocks - 1)` deliberately *clamps* the block index rather than
  erroring on an out-of-range frame.
- **Evidence**:
  ```rust
  // :126
  let num_frames = pack.u32(object + 0x38, "animation frame count")?;
  // :138 — the only check
  || num_frames == 0
  // :231
  let mut tracks = vec![Vec::with_capacity(num_frames as usize); transform_count];
  // :233 — clamps instead of rejecting, so a huge num_frames stays "legal"
  let block_index = ((frame / block_stride) as usize).min(num_blocks - 1);
  ```
  `HkxTransform` is 10 × `f32` = 40 B, so `num_frames = u32::MAX` requests
  ~171 GB in one `Vec::with_capacity`. That path is `handle_alloc_error` →
  **abort**, not an unwind, so the consumer's error handling
  (`asset_provider/animation.rs:88-96`, a `log::debug!` on `Err`) cannot help
  and a `catch_unwind` would not either. A milder hostile value (10⁸) instead
  drives `num_frames × transform_count` Cox-de Boor evaluations — a multi-minute
  freeze at cell load. `convert_hkx_clip` then amplifies it with three more
  `Vec::with_capacity(samples.len())` per track
  (`asset_provider/animation.rs:180-182`).
- **Impact**: Hard process termination (or an unbounded hang) from a `.hkx`
  under `meshes\actors\character\animations\` in any loaded BSA — the modded
  archive surface the crate's own "minimal, safe Havok packfile reader"
  docstring claims to cover. Every other count in both `crates/hkx` files is
  bounded; this is the one hole.
- **Related**: `_audit-severity.md`'s "unbounded alloc reachable from untrusted
  input" row; the same class as #1710 in `crates/pex`
- **Suggested Fix**: Reject unless
  `num_frames <= num_blocks * (max_frames_per_block - 1) + 1` — the exact
  relation `:230-234` already assumes — and add a plausibility ceiling next to
  the existing `4096` caps. Add a negative-path test alongside
  `hostile_vararg_count_errors_instead_of_ooming`'s `crates/pex` precedent.

### MEDIUM

#### SCR-D6-2026-08-16-01: the quest-event journal is polled (destructively claimed) *before* the `frags.is_empty()` early return, so transitions are consumed and discarded

- **Severity**: MEDIUM
- **Dimension**: Scripting Runtime Systems (Dimension 6)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment.rs:1534` (frags already in hand),
  `:1548` (the poll), `:1575-1577` (the bail);
  `crates/scripting/src/quest_stages.rs:608-626`
  (`QuestEventRuntime::poll`)
- **Status**: NEW
- **Description**: `QuestEventRuntime::poll` is not a peek — it ends with
  `self.subscribers.insert(subscriber, next_sequence)`, i.e. polling *is* the
  claim (`acknowledge` afterwards only re-stamps the cursor past events the
  cascade itself pushed). `quest_fragment_dispatch_system` polls at `:1548` and
  only then checks `if queue.is_empty() || frags.is_empty() { return; }`. On any
  frame where `QuestStageFragments` is empty, the transitions it just claimed
  are gone: they are not replayed when fragments register later, and the
  `missed_events` telemetry cannot see it either (the cursor was advanced
  legitimately, so `oldest.saturating_sub(requested)` stays 0).
- **Evidence**:
  ```rust
  // quest_stages.rs:608-626 — poll() claims
  let next_sequence = self.next_sequence;
  self.subscribers.insert(subscriber, next_sequence);
  QuestEventRead { events, missed_events, next_sequence }
  ```
  ```rust
  // fragment.rs:1534 — frags is available one statement before the poll
  let frags = world.resource::<QuestStageFragments>().clone();
  ...
  // :1548
  let journal_read = stages.poll_quest_events(FRAGMENT_QUEST_EVENT_SUBSCRIBER);
  ...
  // :1575
  if queue.is_empty() || frags.is_empty() { return; }
  ```
- **Impact**: On its own, bounded — it matters only while the fragment table is
  empty. But combined with SCR-D7-2026-08-16-01 (where the table is
  *permanently* empty on exterior sessions) it is why that bug leaves no trace
  at all: the transitions look consumed and acknowledged from the journal's
  point of view. It also means any future lazy/streamed fragment registration
  silently loses everything that happened before it landed.
- **Related**: SCR-D7-2026-08-16-01
- **Suggested Fix**: Move `frags.is_empty()` above the poll (it is already
  bound at `:1534`); leave the `queue.is_empty()` half where it is, since an
  empty queue means nothing was claimed.

#### SCR-D7-2026-08-16-02: the trigger-volume spawn branch is the only one that runs for non-primary synthetic children, and builds each volume from the *outer* REFR's `XPRM` at the *child's* transform

- **Severity**: MEDIUM
- **Dimension**: Engine Attach & Trigger Wiring (Dimension 7)
- **Untrusted-Input**: No
- **Location**: `byroredux/src/cell_loader/references/synth_child.rs:145-171`
- **Status**: NEW
- **Description**: Inside `spawn_synth_child`'s invisible-trigger branch,
  `stamp_quest_reference` is correctly gated on `is_primary_synth` (`:152-154`),
  but the entity spawn, the `TriggerVolume` insert, `attach_script_for_refr`
  and `accum.trigger_volumes += 1` are not. The volume is built from
  `placed_ref.primitive` — a property of the **outer** REFR — composed with the
  **child's** `(ref_pos, ref_rot, ref_scale)`. `has_script` (`:133-144`) is
  satisfied for a non-primary child by its own base record's
  `base_record_script` / `base_record_script_instance`, so the branch is
  genuinely reachable with `is_primary_synth == false`.
- **Evidence**:
  ```rust
  if !has_mesh && has_script {
      if let Some(prim) = placed_ref.primitive.as_ref() {           // outer REFR's XPRM
          if let Some(volume) = trigger_volume_from_primitive(prim, ref_pos, ref_rot, ref_scale) {
              let entity = world.spawn();                            // ← every child
              ...
              if is_primary_synth {
                  stamp_quest_reference(world, entity, placed_ref, load_order);
              }
              if attach_script_for_refr(world, entity, child_form_id, record_index, refr_script_instance) {
  ```
  Contrast the LIGH-only (`:238-248`), fxlight (`:333-343`), marker (`:289-302`)
  and stat-miss (`:189-204`) branches, all of which gate both the stamp *and*
  the attach.
- **Impact**: A SCOL/PKIN expansion whose outer REFR carries an `XPRM` and whose
  children include N mesh-less scripted base records spawns N trigger volumes at
  N different positions, all with the outer primitive's extents, all
  script-attached, all counted in the `M47.2 scripts:` summary — and none
  carrying `FormIdComponent`/`SceneAliasCandidate`, so they are invisible to
  alias fill. Even for child 0 the volume is centred on the first *piece's*
  composed transform rather than the authored REFR's. A quest gated on such a
  volume advances at the wrong position, or several times. Rare on vanilla
  (needs an `XPRM` on an expanding REFR), but a wrong/multiple quest advance is
  precisely the silent-game-logic class this domain escalates.
- **Related**: #2026 (the REFR-own-VMAD half of the same gate),
  SCR-D7-2026-08-16-03
- **Suggested Fix**: Gate the whole branch on `is_primary_synth` — an `XPRM`
  belongs to the authored REFR, so exactly one volume should exist per REFR
  regardless of how many pieces its base record expands into.

#### SCR-D7-2026-08-16-03: base-record script attach is gated on `is_primary_synth` in two spawn branches and ungated in three, with nothing recording which policy is intended

- **Severity**: MEDIUM
- **Dimension**: Engine Attach & Trigger Wiring (Dimension 7)
- **Untrusted-Input**: No
- **Location**: ungated —
  `byroredux/src/cell_loader/references/synth_child.rs:599` (main static mesh),
  `:155` (trigger volume),
  `byroredux/src/cell_loader/references/mod.rs:610` (actor);
  gated — `synth_child.rs:238-248` (LIGH light-only), `:333-343` (fxlight)
- **Status**: NEW
- **Description**: `refr_script_instance_for_synth_child` correctly restricts
  the *outer REFR's own* VMAD to `synth_idx == 0` (#2026). Orthogonally to that,
  each synthetic child has its own `child_form_id` and therefore its own base
  record's `SCRI`/`VMAD`. Three spawn branches attach that base-record script
  for every child; two attach it only for child 0 — even though in those two the
  entity itself *is* spawned for every child. Nothing in the code or comments
  states which behaviour is intended.
- **Evidence**: the LIGH light-only branch spawns and fully configures the light
  entity at `synth_child.rs:216-237` unconditionally, then:
  ```rust
  if is_primary_synth {
      stamp_quest_reference(world, entity, placed_ref, load_order);
      attach_quest_reference_script(world, entity, child_form_id, record_index,
                                    refr_script_instance, accum);
  }
  ```
  while the static-mesh path at `:599` calls `attach_script_for_refr(world,
  placement_root, child_form_id, …)` with no gate at all.
- **Impact**: A scripted `LIGH` inside a SCOL/PKIN expansion gets a rendered
  light with its base-record script never attached, while a scripted `STAT`
  sibling in the same expansion does get its script. Quest-controlled /
  scripted lamps in FO4 pack-ins are the realistic case. Silent either way — the
  `scripts_recognized` counter simply doesn't increment.
- **Related**: #2026, #2541, SCR-D7-2026-08-16-02
- **Suggested Fix**: Pick one policy, apply it at all five sites, and state it in
  `refr_script_instance_for_synth_child`'s docstring next to the existing
  REFR-own-VMAD rule. Given the base record is per-child, "attach per child"
  looks correct — which means widening the two LIGH branches, not narrowing the
  three others.

#### SCR-D3-2026-08-16-01: the `.psc`-vs-`.pex` fidelity gate does not execute in a default `cargo test`, and a checked-in fixture is already feasible

- **Severity**: MEDIUM
- **Dimension**: Decompiler — Control-Flow / Boolean / Lower (Dimension 3)
- **Untrusted-Input**: No (a test-coverage gap on an untrusted-input pipeline)
- **Location**: `crates/pex/tests/r5_fidelity.rs` (`#[ignore]`),
  `crates/scripting/tests/pex_recognize_e2e.rs:37`, `:80`, `:120` (all
  `#[ignore]`), `crates/pex/examples/pex_corpus_smoke.rs:145`
- **Status**: NEW (the "no parity test" issue #1740 was closed by *adding* the
  ignored test; that it never runs is a distinct, unfiled gap)
- **Description**: The only fidelity instrument that executes is
  `recognizes_da10_and_reproduces_hand_builder`, and it never calls
  `decompile_script` — it runs the `.psc` frontend. All four tests that do
  exercise the decompiler end-to-end are `#[ignore]`d on Skyrim SE game data.
  The corpus smoke harness is not a substitute: `Ok(Ok(_)) => decompiled_ok += 1`
  throws away the `Script` without any shape check, so a decompile that succeeds
  with a wrong AST scores as a success. A default `cargo test` therefore has
  **zero** coverage of "does the decompiler produce the right tree".
- **Evidence**: measured this pass —
  `cargo test -p byroredux-pex` → `1 ignored` (`r5_fidelity`);
  `cargo test -p byroredux-scripting` → `0 passed; 0 failed; 3 ignored` for
  `pex_recognize_e2e`. Both #2655 (a `While` loop silently erased) and #2657 (a
  fix that was inert on the `.pex` path) survived three consecutive "clean"
  passes through exactly this hole.
- **Impact**: The domain's highest-bug-density component has no CI-visible
  correctness gate. Every fidelity claim in this report series rests on manual
  runs against a specific developer's disk.
- **Related**: #1740 (closed), #2655, #2657, #2542
- **Suggested Fix**: `crate::PexWriter` already builds hand-made FO4/Skyrim-BE/
  Starfield `.pex` in-tree (`parses_a_handbuilt_fo4_pex` and siblings). Emit a
  DA10-shaped `.pex` with it, check it in, and assert
  `decompile_script → translate_script → da10_main_door(...)` byte-equality in a
  non-ignored test. Keep the game-data test as the wider corpus check.

#### SCR-D8-2026-08-16-02: an out-of-range `track_to_bone` entry silently drops a whole animation track — nothing validates the Havok binding against the skeleton

- **Severity**: MEDIUM
- **Dimension**: Havok Idle / Cinematic Slice (Dimension 8)
- **Untrusted-Input**: **Yes**
- **Location**: `crates/hkx/src/animation.rs:243-262` (the binding decode),
  `byroredux/src/asset_provider/animation.rs:173-179` (the only consumer)
- **Status**: NEW
- **Description**: `decode_spline_animation` builds `track_to_bone` either as
  the identity map (empty binding) or verbatim from `u16` file bytes
  (`raw.chunks_exact(2).map(u16::from_le_bytes)`), with no range check — it has
  no skeleton to check against at that point, and the field's own doc
  (`:41-43`) promises only that empty bindings are expanded. The two are joined
  in `convert_hkx_clip`, where both possible misses are a bare `continue` with
  no log:
  ```rust
  let Some(&bone_index) = animation.track_to_bone.get(track_index) else { continue; };
  let Some(bone) = skeleton.bones.get(bone_index as usize) else { continue; };
  ```
- **Impact**: A clip authored against a different rig — an FNIS/Nemesis-extended
  or XPMSE skeleton, or a creature clip resolved by the candidate-name search —
  installs with its mismatched tracks silently absent. The affected limb stays
  at bind pose while the rest of the body animates, and the only observable
  symptom is a clip that plays. This is the "silently short zip = a limb frozen
  at bind pose" case Dimension 8's charter asks to check.
- **Related**: SCR-D8-2026-08-16-01
- **Suggested Fix**: Count the dropped tracks in `convert_hkx_clip` and
  `log::warn!` once per clip with the path and the offending indices; range-check
  `track_to_bone` against `transform_count` inside `decode_spline_animation` so
  an obviously-corrupt binding is an `Err` rather than a silent truncation.

#### SCR-D8-2026-08-16-04: `crates/hkx`'s only integration test passes vacuously without game data, leaving the crate with three real tests and no negative-input coverage

- **Severity**: MEDIUM
- **Dimension**: Havok Idle / Cinematic Slice (Dimension 8)
- **Untrusted-Input**: No (coverage gap on an untrusted-input parser)
- **Location**: `crates/hkx/src/animation.rs:887-899`
- **Status**: NEW
- **Description**: `cargo test -p byroredux-hkx` reports `4 passed`. One of the
  four, `skyrim_cart_player_idle_decodes_when_assets_are_available`, opens with:
  ```rust
  let archive_path = data_dir.join("Skyrim - Animations.bsa");
  if !archive_path.is_file() {
      return;
  }
  ```
  — a bare early `return`, so on any machine without Skyrim SE installed it is
  counted as a pass rather than as `ignored`. The sibling crates use
  `#[ignore = "needs Skyrim SE game data on disk"]` for exactly this
  (`crates/pex/tests/r5_fidelity.rs`,
  `crates/scripting/tests/pex_recognize_e2e.rs`), which at least surfaces in the
  summary line. The remaining three tests are two `decode_three_component_40`
  unit checks and one linear-B-spline check: **`Packfile::parse`,
  `decode_skeleton` and `decode_spline_animation` have no test of their own at
  all**, and there is no malformed / truncated / hostile-input test anywhere in
  the crate — the discipline `crates/pex` has five of (`rejects_bad_magic`,
  `rejects_truncation`, `rejects_bad_value_type`, `rejects_bad_string_index`,
  `hostile_vararg_count_errors_instead_of_ooming`).
- **Impact**: The permanently-green test is the class #2430–#2433 were filed to
  eliminate. The missing negative coverage is how SCR-D8-2026-08-16-01 got in.
- **Related**: #2430–#2433 (closed), SCR-D8-2026-08-16-01, #2267
- **Suggested Fix**: Convert the asset test to `#[ignore]` with the same message
  the two sibling crates use, and add byte-level negative tests for the header,
  the section table, the fixup tables, the spline dimensions block and
  `num_frames` — the last of which is the direct regression guard for
  SCR-D8-2026-08-16-01.

### LOW

#### SCR-D3-2026-08-16-02: `decompile/mod.rs`'s pipeline docstring is first-commit-era and states the wrong pass order

- **Severity**: LOW
- **Dimension**: Decompiler — Control-Flow / Boolean / Lower (Dimension 3)
- **Untrusted-Input**: No
- **Location**: `crates/pex/src/decompile/mod.rs:7-14`
- **Status**: NEW (distinct from #2542, which is `docs/feature-matrix.md`)
- **Description**: The module doc still reads "Pipeline, built up across
  commits: 1. `cfg` — basic-block control-flow graph (**this commit**). 2.
  *opcode → node-tree lifting + copy-propagation* (**next**). 3. *control-flow +
  boolean-operator reconstruction* (**next**). 4. *lower the node tree →
  `byroredux_papyrus::ast::Script`* (**next**)." All four passes shipped long
  ago. Beyond the stale markers, item 3 collapses the two separate passes into
  one bullet and names control-flow first — the same wrong-order statement
  #2542 tracks in `docs/feature-matrix.md`, but here in the decompiler's own
  module doc, which #2542 does not cover.
- **Impact**: Documentation only, but it is the first thing a reader opens to
  learn the pipeline, and the boolean-before-control-flow order is load-bearing:
  `boolean.rs`'s own module doc spends two paragraphs on why it must run first,
  and `control_flow.rs`'s `||`-skip only fails closed because it does.
- **Related**: #2542
- **Suggested Fix**: Replace with the live order —
  `cfg → lift (+copy-prop) → boolean → control_flow → lower`, matching
  `decompile_body` — and drop the "(this commit)"/"(next)" markers.

#### SCR-D8-2026-08-16-03: one out-of-range annotation timestamp hard-fails the entire clip decode

- **Severity**: LOW
- **Dimension**: Havok Idle / Cinematic Slice (Dimension 8)
- **Untrusted-Input**: Yes
- **Location**: `crates/hkx/src/animation.rs:331-333`, contrast `:341`
- **Status**: NEW
- **Description**: `read_annotations` returns
  `Err(InvalidData("annotation time is out of range"))` when any annotation's
  time is non-finite, negative, or `> duration + 0.001`. That `Err` propagates
  out of `decode_spline_animation`, so a clip whose *pose* data is entirely
  valid becomes undecodable because of one bad metadata timestamp; the consumer
  logs at `debug` and the idle silently fails to install. Eight lines later the
  accepted times are already clamped (`time: time.min(duration)`), so the
  clamp-vs-reject choice is inconsistent inside one function, and the `0.001`
  tolerance is unsourced.
- **Impact**: Bounded — no vanilla cart clip trips it (all 16 decode in the
  asset test). It is a robustness asymmetry, not a live bug.
- **Suggested Fix**: Skip (or clamp, matching `:341`) the offending annotation
  and keep the clip; reserve `Err` for structural failures.

## Existing / correctly-tracked — NOT re-filed

Verified still open and still accurate against current code:
**#2289** (new effect primitives with no decline-path tests), **#2290**
(`translate/source.rs` module doc claims no `.pex` parser exists), **#2540**
(widened `SetObjective*` `i32` has no range test), **#2541** (no test pins the
`is_primary_synth` gate — note SCR-D7-2026-08-16-02/-03 are *behavioural*
divergences at that gate, not the missing test), **#2542**
(`feature-matrix.md` pass order), **#2668** (`OffsetMap::to_original` linear
scan), **#2669** (`two_state_activator::vmad_bool` fallback), **#2670**
(inventory-grant rekey drops a grant with no `SceneAliasCandidate`), **#2671**
(alias match-CTDAs read the previous refresh's table), **#2672** (`cleanup.rs`
drain contract vs the 10 self-draining markers), **#2267** (`crates/hkx`
`global_target` dead accessor, no tests), **#2153** / **#2270** (lock-discipline
documentation).

## Considered and disproved / dropped

- **`EntityId` reuse hazard in `QuestAliasInjectionState::factions`** — the
  ledger is keyed `(EntityId, u32)` and a recycled id could make
  `apply_alias_injections` skip the `FactionRanks` push. **Disproved**:
  `crates/core/src/ecs/world.rs:388` — "Entity IDs are never reused."
- **`lower_binary_op`'s `_ => BinaryOp::Eq` default arm** — re-examined in light
  of #2666/#2667 converting two sibling "unreachable" invariants into
  fail-closed declines. Still structurally unreachable (only `create_node`'s
  fixed op set and `boolean::combine`'s `"&&"`/`"||"` ever reach it), as five
  prior passes concluded. Not filed.
- **Nondeterministic `FactionRanks.0` ordering** from iterating the
  `desired_factions` `HashMap` in `apply_alias_injections`. Real, but
  `FactionRanks::rank` is an order-independent linear find and the component is
  not in the save registry, so there is no observable consequence. Dropped.
- **`refresh_scene_actor_bindings`'s four `.expect("… storage registered")`** —
  `FactionRanks` and `Inventory` are registered by `scene::register`
  (`crates/scripting/src/scene.rs:110-111`), which `crate::register` calls.
  Sound.
- **`preprocess` trailing-`\`-at-EOF** — the backslash is emitted, not swallowed;
  `chars.peek()` returning `None` falls through to `output.push(ch)`. Correct.
- **`bspline_weights`'s span-search `while` loop** — the NURBS-book `FindSpan`
  algorithm; `read_spline_header`'s monotonic-knot check plus the
  `low_knot < time < high_knot` precondition on the branch that reaches it
  guarantee termination, and every index (`middle + 1`, `span + degree`,
  `span + 1 - order`, `first + index`) is in range. Sound.
- **`OnTriggerEnterEvent` only ever firing for the player** —
  `trigger_detection_system` tests one entity. Deliberate v0 scope, documented
  in the module header. Not a defect.

## Future-Phase Readiness

- **SCR-D7-2026-08-16-01 is the highest-priority item on this list** and the
  cheapest: one call moved inside `populate_scene_runtime`. Until it lands,
  every exterior measurement of fragment yield — including the
  `fragment_coverage` re-measurement `docs/engine/m47-3-quest-alias-design.md`
  still lists unchecked — is measuring an unwired pipeline, not recognizer
  coverage.
- **SCR-D3-2026-08-16-01 is the durable one.** Three consecutive passes over
  unchanged decompiler code found nothing; the two real defects that did exist
  (#2655, #2657) were found by pathological-input probing and by running the
  harness, not by reading. A checked-in `PexWriter` fidelity fixture converts
  that from a per-pass heroic into a CI invariant, and is the precondition for
  trusting the Obscript/SCTX (Phase 5) frontend when it lands.
- **`crates/hkx` needs a parser-discipline dimension of its own, or an owner.**
  `_audit-common.md` already lists it as un-owned with `/audit-scripting` Dim 8
  as the nearest thing. This pass — its first — found an abort-class bug, a
  silent-drop bug and a vacuous test in ~1,250 LOC. The `/audit-nif` Dim 1
  checklist applies to it almost verbatim.

## Findings Count

**10 new: 0 CRITICAL / 2 HIGH / 6 MEDIUM / 2 LOW.**

By dimension — **Dim 1** (`.pex` reader & opcode decode): 0. **Dim 2**
(decompiler CFG & lift): 0. **Dim 3** (control-flow / boolean / lower): 1 MEDIUM
+ 1 LOW. **Dim 4** (`.psc` lexer & Pratt parser): 0. **Dim 5** (recognizer-chain
soundness): 0. **Dim 6** (scripting runtime systems): 1 MEDIUM. **Dim 7** (engine
attach & trigger wiring): 1 HIGH + 2 MEDIUM. **Dim 8** (Havok idle / cinematic
slice): 1 HIGH + 2 MEDIUM + 1 LOW.
