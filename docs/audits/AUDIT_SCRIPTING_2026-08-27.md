# Scripting Subsystem Audit — 2026-08-27

Fifteenth full pass over the M30/M47 Papyrus / `.pex` / ECS scripting domain
(prior reports: `AUDIT_SCRIPTING_2026-06-23.md` … `_08-20.md`, `_08-24.md`).
Run single-agent, no sub-agent fan-out, per this session's explicit
instruction. Comprehensive — no `--focus` filter; all 8 dimensions covered
directly via source reads, greps, `cargo check`/`cargo test`, git archaeology,
and **three live game-data corpus runs** (Skyrim SE + Fallout 4 + Starfield).

**Scope**: `crates/pex`, `crates/papyrus`, `crates/scripting` (owner crates)
plus `crates/hkx` (Dim 8, folded in) and the engine-side attach / cinematic /
cell-loader wiring (Dim 7).

**Dedup baseline**: `gh issue list --repo matiaszanolli/ByroRedux --limit 400`
(139 open, saved to `/tmp/audit/scripting/issues.json`, cleaned up per Phase 4),
`docs/audits/AUDIT_SCRIPTING_2026-08-24.md`, and `git log --since=2026-08-24`
over every path in scope.

**Cross-audit note**: a concurrent `/audit-concurrency` pass filed a HIGH on
`crates/scripting/src/condition.rs:470-509` (the `GetActorValue` arm holding an
`ActorValues` read guard across `try_resource::<CharacterRuleset>()`, forming a
lock-order cycle with `crates/core/src/character/regen.rs:176-180`). That is
**not** re-filed here. This pass's Dim 6 lock analysis takes it as given and
notes one adjacent site with the same *shape* but a different, already-ordered
pair (`resolve_entity_by_global_form_id`, `condition.rs:441-446`: a
`FormIdComponent` read guard held across `try_resource::<FormIdPool>()`, ordered
per #313) — mentioned for the concurrency audit's benefit, not filed.

## What changed since 2026-08-24

Twelve commits touch the domain. Unlike the 08-24 pass — where six same-day
feature commits were the churn — this range is **almost entirely remediation
of previously-filed findings**, plus one behavioural change to the fragment
walk:

| Commit | Effect on this domain |
|---|---|
| `c88f2356` | Fix #2289 / #2540 — decline-path tests for 14 previously-untested effect primitives; negative-index + i32-overflow decline tests for the three objective primitives. Also added the three missing `fragment_coverage.rs` match arms that broke `cargo test --workspace` on 08-24 |
| `3770e33d` | Fix #2541 — pins the `is_primary_synth` gate on every synth-child identity-stamp call site |
| `d54c7a51` | Fix #3014 / #3018 — `crates/hkx`'s vacuous integration test is now `#[ignore]`d; `PackfileBuilder` promoted to `packfile::fixtures` so two new malformed-input tests exist; an out-of-range annotation timestamp now skips that annotation instead of discarding the whole clip |
| `1d9a5041` | Fix #3250 — `copied_transform` helper replaces the two simultaneous `ComponentRef<Transform>` read guards in `Effect::SetVehicle` / `Effect::TetherToHorse` |
| `911ac31f` | Fix #3312 — residual lock-order edges in `scene_trigger_actor_approach_system` (`ScenePlayer` / `SceneAliasCandidate` snapshotted to `Vec` before the `Transform` guard) |
| `149e9c03` | Fix #2290 / #3019 — scripting-pipeline doc drift in `translate/source.rs` and `pex/decompile/mod.rs`. **The `decompile/mod.rs` half replaced a stale order with a differently-wrong one — see SCR-D3-2026-08-27-01** |
| `fbd6286e` | Fix #3287 — `decompile_catching_panics` extracted so `translate_pex`'s `catch_unwind` (#1816) is reachable from a test |
| `e60951a2` | Fix #3161 — the quest-fragment walk is folded into `populate_scene_runtime` and self-latches on a new `QuestStageFragments::populated` flag |
| `a5ed4bf5` | Fix #3112 — `GetEquipped` now scans `slots.equipped_indices()` (biped occupants **and** the wielded weapon) instead of a bare `occupants` scan |
| `98eea9b3`, `4e1afcbe`, `0c74a2b8` | Exterior session-reload / actor-value-keyspace / WEAP-VATS refactors, incidental to this domain |

`crates/pex` and `crates/papyrus` have **no functional change** since
2026-08-19 — the only edit in range is the `decompile/mod.rs` docstring above.
Dims 1–4 got a standing-invariant spot-check rather than a full re-read.

## Build & test state — CLEAN (both 08-24 cross-references now closed)

```
$ cargo check -p byroredux-scripting -p byroredux-pex -p byroredux-papyrus -p byroredux-hkx --all-targets
    Finished `dev` profile [optimized + debuginfo] target(s) in 2.77s

$ cargo test -p byroredux-scripting -p byroredux-pex -p byroredux-papyrus -p byroredux-hkx
    331 passed (scripting lib) + 90 + 56 + 19 + 4 + … ; 0 failed
```

- **SAFE-BUILD-2026-08-24-01** (the `fragment_coverage.rs` non-exhaustive
  `match` that aborted `cargo test --workspace`) — **FIXED** in `c88f2356`.
- **ECS-2026-08-24-01 / #3250** (double `ComponentRef<Transform>` read guard in
  `fragment.rs`) — **FIXED** in `1d9a5041`; the new `copied_transform` helper
  copies out of each guard before the next lookup, guarded by
  `copied_transform_releases_each_component_guard`.

## Live-corpus measurement — the M47.3 Phase 2 checkbox this audit closes

`docs/engine/m47-3-quest-alias-design.md`'s Phase 2 checklist has carried an
**unchecked** item since 2026-08-07: *"live-corpus re-measurement of
`fragment_coverage`'s `AddItem`/`MoveTo` yield shows a real (non-zero) hit
rate"*. The 2026-08-24 pass could not run it (the harness did not compile).
It was run this pass, against all three shipped Papyrus corpora:

```
$ cargo run --release -p byroredux-scripting --example fragment_coverage -- \
    ".../Skyrim Special Edition/Data/Skyrim - Misc.bsa"
  behavioral fragments: 15652 · claimed 6666 (42.6%) · declined 8986 (57.4%)

$ ... -- ".../Fallout 4/Data/Fallout4 - Misc.ba2" ".../Starfield/Data/Starfield - Misc.ba2"
  behavioral fragments: 28166 · claimed 9841 (34.9%) · declined 18325 (65.1%)
```

**Verdict on the open question**: `AddItem` yield is now genuinely **non-zero**
— 12 emissions on Skyrim, 42 on FO4+Starfield (54 total). `MoveTo` yield is
**still exactly zero**, on all three games, and this pass established that it is
*structurally* zero rather than incidentally zero — see
SCR-D5-2026-08-27-01. The checkbox can be ticked with that qualification.

Effect histogram (43,818 behavioral fragments, both runs combined) — the ten
largest: `SetStage` 8,506 · `SetObjectiveDisplayed` 3,024 ·
`SetObjectiveCompleted` 2,473 · `StopQuest` 1,622 · `StartScene` 1,522 ·
`SetGlobalValue` 1,148 · `Conditional` 871 · `Disable` 488 · `StopScene` 388 ·
`CompleteAllObjectives` 308. `MoveTo`: **0**.

## Decompiler Soundness Matrix (Dims 1–4)

| Pass | Bounds-safe | Terminates | Total (no panic) | Fidelity-tested |
|------|:---:|:---:|:---:|:---:|
| Reader (`reader.rs`) | Yes | Yes | Yes | Yes |
| CFG (`cfg.rs`) | Yes | Yes | Yes | Yes |
| Lift + copy-prop (`lift.rs`) | Yes | Yes (#2024 linear chain) | Yes (#2666 fail-closed) | Yes |
| Boolean (`boolean.rs`) | Yes | Yes (`MAX_REBUILD_DEPTH = 1024`) | Yes (#2667) | Partly |
| Control-flow (`control_flow.rs`) | Yes | Yes, same cap | Yes (#1732 fail-closed) | Partly |
| Lower (`lower.rs`) | Yes | Yes | Yes | Yes for straight-line/property/event shape |

Re-verified by direct grep this pass (no source changed): `MAX_OPCODE = 51`
with `#[repr(u8)]` contiguous `0..=50` and the `transmute` guarded
`byte >= MAX_OPCODE` (`crates/pex/src/opcode.rs:9,68,131-136`); the var-arg vec
still `Vec::new()` + `push`, never `with_capacity(n)` (#1710,
`crates/pex/src/reader.rs:465-505`); `MAX_REBUILD_DEPTH = 1024` present in
**both** `control_flow.rs:39` and `boolean.rs:56`; `MAX_EXPR_DEPTH` /
`MAX_STMT_DEPTH = 256` in `crates/papyrus/src/parser/{expr.rs:19,stmt.rs:38}`.

`translate_pex`'s `catch_unwind` is not only still present, it is now
**test-reachable** for the first time (`decompile_catching_panics` +
`a_decompile_panic_is_a_silent_none`, `crates/scripting/src/translate/mod.rs:118-144`)
— #3287 closed a genuine "fix confirmed only by reading it" gap.

**Untrusted-input robustness verdict for `.pex` / `.psc` / `.hkx`: CLEAN.** No
panic, OOB index, or unbounded allocation reachable from hostile bytes was
found. `crates/hkx` gained two real malformed-input regression tests this range
(`decode_spline_animation_rejects_a_sample_count_bomb`,
`read_annotations_skips_an_out_of_range_time_and_keeps_the_rest`) and its
annotation walk remains bounded (`MAX_TRACKS = 4096`,
`MAX_ANNOTATIONS = 65_536`, `MAX_TRANSFORM_SAMPLES = 16_000_000`).

## Decline-Invariant Audit

| Decline point | Verdict |
|---|---|
| `classify_guard_atom` `?` in `classify_if_condition`'s per-atom loop | Conservative (unchanged) |
| `split_and` refusing to split `\|\|` | Conservative, deliberate (unchanged) |
| `lower_statements`'s `_ => return None` statement arm | Conservative; the two narrowed exceptions (`Stmt::While` via `lower_3d_loaded_wait`, `Stmt::If` via `Effect::Conditional`) are unchanged and still exactly as narrow as documented |
| `receiver_object`'s explicit `key == "self"` guard + local-receiver decline | Present and unchanged (`effects.rs:1212-1221`) |
| `RECOGNIZERS` order (per-script `two_state_activator`, `rumble` before generic `quest_stage_gate`) | Correct (`translate/mod.rs:49-55`) |
| **Per-primitive argument-shape guards** | **Over-conservative in three primitives to the point of total production dead-ness — SCR-D5-2026-08-27-01** |
| **Upper arg-count guard on `prim_set_stage` + the three objective primitives** | **Absent — SCR-D5-2026-08-27-03** (the only four of 31 primitives with no upper bound) |
| Missing `Enable` counterpart to `Effect::Disable` | **Gap — SCR-D5-2026-08-27-02** |
| `SceneActorBindings::resolve` on an unfilled alias | Returns `None`, never fabricates (unchanged) |
| `QuestRef::Property` on an alias-bound entry | Still declines (unchanged, correct) |

## Runtime Lifecycle Invariant Matrix

| Invariant | Verdict |
|---|---|
| Marker drain coverage | **Complete.** Cross-checked all 46 `impl Component for` types in `crates/scripting` against the Pattern-A drain list (16), the Pattern-B table (10), and the persistent-state catch-all (20). No marker is emitted-but-undrained. `RemoteSceneActorStub` (the newest) is correctly persistent, not transient |
| `cleanup.rs`'s own contract test | Present and sound, but note it only checks doc-vs-drain-list consistency; it cannot detect a marker in *neither* list. The manual sweep above is what covers that |
| Two-phase lock-drop (`timer_tick_system`, `trigger_detection_system`, `recurring_update_tick_system`) | Unchanged, still correct |
| `scene_trigger_actor_approach_system` guard discipline | **Improved** by #3312: `ScenePlayer` and `SceneAliasCandidate` are now snapshotted into `Vec`s before the `Transform` read guard, and `drop(transforms)` precedes the `query_mut::<Transform>()` write phase (`byroredux/src/systems/cinematic.rs:553-601`) |
| `apply_effect` nested-lock residual list | Unchanged in substance (`PlayerControlState` ×3, `Globals` ×1, 12 component acquisitions under the two quest-resource guards); every caller is `add_exclusive`. **But the docstring recording it is now attached to the wrong function — SCR-D6-2026-08-27-01** |
| Scheduler ordering `trigger_detection` → `scene_trigger_actor_approach` → `quest_advance` → `quest_alias_readiness` → `scene_playback` → `scene_fragment_dispatch` → … → `quest_fragment_dispatch` → `fragment_continuation`; `event_cleanup` last in `Stage::Late` | Confirmed unchanged (`byroredux/src/boot.rs:877-970,1520`), all `add_exclusive` |
| Cascade queue FIFO + `is_cascade`-gated `MAX_CASCADE` | Unchanged, correct |
| `QuestStageAdvancedBatch` five-writer consistency | **Still violated at one site** — #3277, re-verified below, not re-filed |
| `cinematic_retained_entities` / `CellRoot` strip | Re-examined the open question 08-24 left: victims come from `CellRootIndex::map.remove(&cell_root)`, so a retained entity is removed from the inverted index **and** loses its `CellRoot` in the same transaction — no orphaned index entry. Consistent; not filed |
| `populate_quest_fragments` idempotency | Now latched per **session** (not per cell load) via `QuestStageFragments::populated`. The latch is set before the `have_archive` fast-out, but `ScriptProvider` is installed at `byroredux/src/scene.rs:814`, ahead of both `populate_scene_runtime` call sites (`world_setup.rs:919`, `cell_loader/load.rs:524`), so the "latched empty before the archive existed" hazard is not reachable on the live path. Not filed |

## Findings

### MEDIUM

#### SCR-D5-2026-08-27-01: Three effect primitives guard on the hand-authored `.psc` call arity, but the only production frontend is `.pex`, where the compiler materializes every default argument — `MoveTo` therefore declines 100% of 3,334 real calls

- **Severity**: MEDIUM
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/effects.rs:790-801` (`prim_move_to`);
  `crates/scripting/src/translate/effects.rs:1072-1080` (`prim_evaluate_package`);
  `crates/scripting/src/translate/effects.rs:889-893` (`prim_player_controls`)
- **Status**: NEW
- **Description**: The Papyrus compiler emits **every** parameter of a call
  into the compiled `.pex`, including ones the author omitted and left at
  their declared default. Quest fragments only ever reach the effect table
  through the `.pex` frontend (`populate_quest_fragments_from_pex` →
  `decompile_script` → `lower_fragment_with_quest_properties`) — the `.psc`
  route is a test-only path. Several primitives, however, were written against
  the *authored* call shape, and reject the default-materialized one:

  | Primitive | Accepted arity | Arities actually observed in the corpus | Declined |
  |---|---|---|---|
  | `prim_move_to` | exactly 1 | 5 (Skyrim, ×1742) and 6 (FO4/Starfield, ×1592) | **3,334 / 3,334 = 100%** |
  | `prim_evaluate_package` | exactly 0 | 0 (×853) and 1 (FO4/Starfield `abResetAI`, ×2628) | **2,628 / 3,481 = 75%** |
  | `prim_player_controls` | `<= 9` | 9 (×118) and 11 (FO4/Starfield, ×61) | 61 / 179 = 34% |

  `MoveTo` is the severe case: it has an `Effect::MoveTo` variant, a dispatch
  arm in `fragment.rs` that resolves both receiver and destination through the
  alias-aware `resolve_object`, and its own regression tests — and it can
  **never** fire on production input. `prim_move_to`'s comment justifies the
  narrowness as refusing to "silently drop [an offset] and misplace the
  object", which is sound reasoning applied to the wrong input shape: the
  offsets it is refusing are, overwhelmingly, the compiler's own zeros.
- **Evidence**: `prim_move_to`, verbatim:
  ```rust
  // effects.rs:790-801
  fn prim_move_to(e: &Expr, scope: &Scope) -> Option<Effect> {
      let (object, args) = method_call(e, "MoveTo")?;
      // The conservative 2-arg shape only (receiver + destination) — a 3rd+
      // argument (offsets / match-rotation) declines rather than silently
      // dropping it and misplacing the object.
      if args.len() != 1 {
          return None;
      }
      ...
  }
  ```
  Corpus probe over every `Fragment_*` body in `Skyrim - Misc.bsa` +
  `Fallout4 - Misc.ba2` + `Starfield - Misc.ba2` (26,641 `.pex`, 43,818
  behavioral fragments), tallying the literal shape of `MoveTo`'s trailing
  arguments:
  ```
  moveto  args=5  count=1742
  moveto  args=6  count=1592
  moveto-tail[f0,f0,f0,btrue]         args=5  count=1668
  moveto-tail[f0,f0,f0,btrue,bfalse]  args=6  count=1585
  ...(43 further distinct tails, 81 calls total, carrying real offsets)
  evalpkg-arg[bfalse]  args=1  count=2621
  evalpkg-arg[btrue]   args=1  count=7
  ```
  **3,253 of 3,334 `MoveTo` calls (97.6%) carry exactly `(0.0, 0.0, 0.0,
  matchRotation)` — precisely the "receiver + destination" semantics
  `Effect::MoveTo { moved, destination }` already models.** Only ~81 calls
  (2.4%) carry a real offset where the current decline is genuinely
  protective. Likewise 2,621 of 2,628 one-arg `EvaluatePackage` calls pass the
  literal `false` default.
- **Impact**: A shipped, tested, dispatch-wired, alias-aware effect
  (`MoveTo`) contributes nothing on any real game's content and cannot be
  observed to be broken by any existing gate — `fragment_coverage` reports a
  zero for it that reads identically to "authors don't use this". Every
  fragment containing a `MoveTo` call is guaranteed to decline in full, so
  this is also a hard ceiling on the whole-fragment claim rate (42.6% Skyrim /
  34.9% FO4+SF), not just on one effect. `EvaluatePackage` is
  game-asymmetric: it works on Skyrim and silently declines on FO4/Starfield,
  which is exactly the kind of per-game divergence the domain's abstraction
  rules exist to prevent. This is a *decline*, so nothing is mis-lowered —
  hence MEDIUM, not HIGH.
- **Related**: `docs/engine/m47-3-quest-alias-design.md` Phase 2's unchecked
  "re-measure `AddItem`/`MoveTo` yield" item — this finding is that
  measurement's result. Same family as #3159 (`Lock`/`Unlock` absent): the
  effect table is growing faster than anything checks it against real input.
- **Suggested Fix**: Accept the default-materialized tail where its literal
  value is the documented Papyrus default, and keep declining otherwise —
  i.e. for `MoveTo`, accept 5/6 args when args 1–3 are numeric-literal `0`
  and the rotation flags are literals, decline on a non-zero or non-literal
  offset; for `EvaluatePackage`, accept a literal `abResetAI`; for
  `prim_player_controls`, widen the bound to the FO4/Starfield parameter
  count. Then add a corpus-arity assertion to `fragment_coverage` (or a
  sibling instrument) so the next primitive written against a `.psc` arity
  fails a gate instead of silently measuring zero.

#### SCR-D5-2026-08-27-02: `Effect::Disable` shipped without an `Enable` counterpart, over a save-persisted resource — a latent one-way door, and 3,005 real `Enable()` calls decline

- **Severity**: MEDIUM
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/effects.rs:476-513`
  (`EFFECT_PRIMITIVES` — no `prim_enable`); `:803-812` (`prim_disable`);
  `crates/scripting/src/fragment.rs:63-84` (`ReferenceEnableState`, whose
  `set_enabled(form_id, bool)` API already supports both directions)
- **Status**: NEW
- **Description**: `5f38402e` added `Effect::Disable` and the save-serialized
  `ReferenceEnableState` resource it writes into, but no `Enable` primitive.
  The resource's own API is symmetric (`set_enabled` takes a `bool` and
  removes from the `disabled` set when `true`), and it is registered for save
  persistence (`byroredux/src/save_io.rs:438`,
  `.register_resource::<ReferenceEnableState>("ReferenceEnableState")`), so
  the *state model* is complete — only the lowering half is one-directional.
  In the real corpus, `Enable()` is the **more common** of the pair:
  ```
  disable  args=1  count=2587
  enable   args=1  count=3005
  ```
  Every fragment containing an `Enable()` call therefore declines in full
  today (the whole-fragment lowering contract), and once
  `ReferenceEnableState` gains the runtime consumer #3278 asks for, a
  reference a script disables can never be re-enabled by script — the disable
  survives save/load by design.
- **Evidence**: `EFFECT_PRIMITIVES` contains `prim_disable` at
  `effects.rs:493`; a grep for an `Enable` sibling finds only
  `prim_enable_player_controls` (`:500`, an unrelated
  `Game.EnablePlayerControls` primitive) — there is no
  `ObjectReference.Enable` lowering. `ReferenceEnableState::set_enabled`
  (`fragment.rs:76-82`) has the `enabled == true` branch that no caller ever
  reaches:
  ```rust
  pub fn set_enabled(&mut self, form_id: u32, enabled: bool) {
      if enabled {
          self.disabled.remove(&form_id);   // no production caller
      } else {
          self.disabled.insert(form_id);
      }
  }
  ```
- **Impact**: Today, inert — nothing consumes `ReferenceEnableState` (#3278),
  so neither half does anything observable. **This finding's severity is
  conditional on #3278 being fixed**: the moment a consumer lands, disabling
  becomes permanent and unrecoverable across saves, and a `Disable`/`Enable`
  pair authored to hide a reference for one quest stage will hide it forever.
  Fixing #3278 without fixing this would ship a strictly worse state than
  either fix alone. Also caps fragment coverage: 3,005 guaranteed declines.
- **Related**: #3278 (`Effect::Disable` has no production consumer, and its
  receiver resolution is narrower than its siblings) — same commit, same
  effect, must be fixed together. Structurally identical to #3159 (a `Lock`
  with no `Unlock`), which the 08-20 pass already named as a one-way door.
- **Suggested Fix**: Add `prim_enable` mirroring `prim_disable` (same
  `receiver_object` treatment, same optional literal `abFadeIn` argument) and
  an `Effect::Enable` variant dispatching to
  `deferred.reference_enable_changes.push((form_id, true))`. Land it in the
  same change as #3278's consumer, not after.

### LOW

#### SCR-D3-2026-08-27-01: the #3019 fix replaced a stale decompiler pass order with a wrong one — `decompile/mod.rs` now lists the boolean pass last, contradicting `decompile_body` and the sibling doc corrected under #2542

- **Severity**: LOW
- **Dimension**: Decompiler — Control-Flow / Boolean / Lower (Dimension 3)
- **Untrusted-Input**: No
- **Location**: `crates/pex/src/decompile/mod.rs:7-18`
- **Status**: Regression of #3019 (CLOSED 2026-08-26 by `149e9c03`)
- **Description**: #3019 filed `decompile/mod.rs`'s module docstring as
  first-commit-era ("phase 1 — this commit", phases 2–4 "(next)"). The fix
  rewrote it to name five phases — but ordered them
  `cfg → lift → control_flow → lower → boolean`, putting the short-circuit
  boolean pass **last**. The actual pipeline runs the boolean pass **third**,
  before control-flow reconstruction, and this ordering is load-bearing: the
  boolean pre-pass collapses `&&`/`||` chains into one conditional so the
  control-flow pass sees a clean diamond, and `control_flow.rs`'s
  conditional-predecessor branch fails closed (#1732) precisely because
  well-formed input should never reach it *after* the boolean pass has run.
  The commit message states the wrong order as fact and records that
  `docs/engine/scripting.md` and `m47-2-design.md` were checked — but
  `docs/feature-matrix.md:174`, corrected in the *same* commit under #2542,
  now carries the **right** order (`CFG→lift→short-circuit→control-flow→lower`),
  so the two docs contradict each other.
- **Evidence**:
  ```rust
  // crates/pex/src/decompile/lower.rs:230-236 — the real pipeline
  let mut cfg = build_cfg(func)?;
  let mut scopes = lift_function(object, func, &cfg)?;
  // Collapse `&&`/`||` short-circuits before control-flow reconstruction
  rebuild_boolean_operators(&mut cfg, &mut scopes, &func.name)?;
  let nodes = reconstruct(cfg, scopes, &func.name)?;
  Ok(lower_body(&nodes))
  ```
  ```rust
  // crates/pex/src/decompile/mod.rs:13-18 — the docstring that just landed
  //! 3. [`control_flow`] — control-flow reconstruction (if/else, loops) over
  //!    the CFG.
  //! 4. [`lower`] — lowers the node tree → `byroredux_papyrus::ast::Script`,
  //!    with a fidelity gate.
  //! 5. [`boolean`] — short-circuit boolean-operator reconstruction
  //!    (`rebuild_boolean_operators`).
  ```
  ```
  docs/feature-matrix.md:174: ... (CFG→lift→short-circuit→control-flow→lower) ...
  ```
- **Impact**: Doc-rot only, but on the one ordering fact the domain's own
  skill file calls load-bearing, in the module docstring a reader reaches
  first. #2542 and #3019 were filed as the *same* defect in two files; one was
  fixed correctly and one was not, and both are now closed.
- **Related**: #3019 (CLOSED), #2542 (CLOSED, fixed correctly in
  `docs/feature-matrix.md`).
- **Suggested Fix**: Swap phases 3–5 in `crates/pex/src/decompile/mod.rs` to
  `boolean → control_flow → lower`, and add a one-line pointer to
  `lower.rs::decompile_body` as the authority so the next rewrite has a source
  to check against.

#### SCR-D6-2026-08-27-01: the #3250 fix orphaned `apply_effect`'s nested-lock-safety docstring onto a three-line helper; `apply_effect` now has no doc comment at all

- **Severity**: LOW
- **Dimension**: Scripting Runtime Systems (Dimension 6)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment.rs:603-641`
- **Status**: NEW
- **Description**: `1d9a5041` inserted the new `copied_transform` helper
  *between* `apply_effect`'s multi-paragraph doc comment and `fn apply_effect`.
  Rust attaches a `///` block to the next item, so the entire nested-lock
  contract — the residual list (`PlayerControlState` ×3, `Globals` ×1, "12
  component-storage acquisitions"), the "only safe because every caller is
  `add_exclusive`" argument, the #2660 `SceneActorBindings` snapshot rationale,
  and the instruction to re-derive the analysis before adding a lock — is now
  the documentation for a three-line `Transform` copy helper, and
  `apply_effect` (line 641) is undocumented. This is the exact doc the
  `/audit-scripting` skill directs auditors to treat as authoritative ("re-read
  that doc comment rather than this bullet's own count, since it is the thing
  this bullet is transcribing and will drift again").
- **Evidence**:
  ```rust
  // fragment.rs:634-641
  /// Every production caller constructs the batch before taking quest
  /// resource guards and applies it only after those guards have dropped
  /// (#2269, #2539, #2660).
  fn copied_transform(world: &World, entity: EntityId) -> Option<Transform> {
      // #3250 — `World::get` returns an owning read guard. ...
      world.get::<Transform>(entity).map(|transform| *transform)
  }

  fn apply_effect(          // <- no doc comment
  ```
- **Impact**: `cargo doc` renders a lock-ordering contract on the wrong item,
  and a future editor of `apply_effect` no longer sees the "adding a new
  nested lock here needs the analysis re-derived" instruction adjacent to the
  code it governs — the same drift the ABBA argument depends on not happening.
- **Related**: #3250 (CLOSED — the fix itself is correct; only its placement
  is wrong).
- **Suggested Fix**: Move `copied_transform` above the doc block (or below
  `apply_effect`), and give it its own one-line doc. Purely mechanical.

#### SCR-D5-2026-08-27-03: `prim_set_stage` and the three objective primitives are the only four of 31 effect primitives with no upper argument-count guard — an over-arity call silently lowers

- **Severity**: LOW
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Untrusted-Input**: Yes (a modded `.pex` reaches this code)
- **Location**: `crates/scripting/src/translate/effects.rs:588-595`
  (`prim_set_stage`); `:699-709` (`prim_set_objective_displayed`);
  `:711-720` (`prim_set_objective_completed`); `:722-731`
  (`prim_set_objective_failed`)
- **Status**: NEW
- **Description**: Every other primitive in `EFFECT_PRIMITIVES` bounds its
  argument count — `prim_add_item` (`args.len() > 3 → None`), `prim_activate`
  (`> 2`), `prim_disable` (`> 1`), `prim_set_open`, `prim_start_scene`
  (`!args.is_empty()`), and so on — and #2289 added a decline-path test for
  each. These four read only positional args 0 and 1 and ignore any further
  argument silently. `prim_set_stage` is the highest-traffic effect in the
  domain (20,322 real calls, all one-argument) and the one whose false-positive
  lowering has the largest blast radius: a fragment shaped
  `SomeQuest.SetStage(10, <unmodeled term>)` lowers to a plain
  `SetStage { stage: 10 }` rather than declining.
- **Evidence**:
  ```rust
  // effects.rs:588-595 — no args.len() bound anywhere in the body
  fn prim_set_stage(e: &Expr, scope: &Scope) -> Option<Effect> {
      let (object, args) = method_call(e, "SetStage")?;
      let stage = u16::try_from(int_arg(args, 0)?).ok()?;
      Some(Effect::SetStage {
          quest: receiver_quest(object, scope)?,
          stage,
      })
  }
  ```
  Mechanical sweep of all 31 `fn prim_*` bodies for an `args.len()` /
  `args.is_empty()` guard: 27 guarded (directly or via a guarded delegate),
  4 unguarded — the four above.
- **Impact**: Not reachable from vanilla content (the compiler emits exactly
  the declared arity, and all four functions' Papyrus signatures are within
  the read range), so no shipped game is affected — hence LOW. It is a real
  hole in the decline discipline for modded/hand-authored input, and an
  inconsistency a reader of the other 27 primitives would not expect.
- **Related**: #2289 (CLOSED — added decline tests for 14 primitives, but
  arg-count declines for these four were not among them); #2540 (CLOSED —
  added negative-index and i32-overflow declines for the three objective
  primitives, but not an over-arity decline).
- **Suggested Fix**: Add `if args.len() > N { return None; }` to each (N = 1
  for `SetStage`, 3 for `SetObjectiveDisplayed`, 2 for the other two, matching
  their real Papyrus signatures), plus one decline test each in the block
  #2289 already established.

#### SCR-D5-2026-08-27-04: `fragment_coverage`'s module doc promises a decline-reason tally that the implementation does not produce — the instrument meant to make "the next primitives to add obvious" cannot answer that question

- **Severity**: LOW
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/examples/fragment_coverage.rs:1-22`
  (the claim) vs `:155-165` (the tally loop)
- **Status**: NEW
- **Description**: The harness's module doc states it will *"tally claimed vs
  declined (with the decline reasons, so the next primitives to add are
  obvious)"*. The implementation tallies `behavioral`, `claimed`, `empty`,
  and a per-`Effect`-kind histogram of the fragments that **did** lower — and
  nothing at all about the ones that declined. Since a fragment declines
  wholesale on its first unmodeled statement, the 8,986 (Skyrim) + 18,325
  (FO4+SF) declined fragments are reported as a single number with no
  attribution. This audit could not use the harness to answer "why is `MoveTo`
  zero" and had to build a separate AST-walking probe to get there; the two
  headline findings above came from that probe, not from the checked-in
  instrument.
- **Evidence**:
  ```rust
  // fragment_coverage.rs:155-165 — the entire tally
  if let Some(effects) = lower_fragment_with_quest_properties(&func.body, &quest_properties) {
      claimed += 1;
      claimed_effects += effects.len();
      for e in &effects {
          *effect_hist.entry(effect_kind(e)).or_default() += 1;
      }
  }
  ```
  There is no `else` arm, and no `decline_hist` anywhere in the file.
- **Impact**: The domain's one empirical coverage instrument reports *what
  works* and is blind to *what doesn't* — which is the half that drives
  roadmap decisions about which primitive to write next. Directly explains why
  SCR-D5-2026-08-27-01 survived four prior audit passes: the harness would
  report a `MoveTo` structural-zero identically to "authors don't use MoveTo".
- **Related**: SCR-D5-2026-08-27-01 (the finding this gap concealed). A second,
  smaller instance of the same class in the same range:
  `byroredux/src/asset_provider/script.rs:74-85` still says the quest-fragment
  walk *"Runs once per cell load"*, three lines above the #3161 latch that
  made it run once per session.
- **Suggested Fix**: Record, per declined fragment, the first statement shape
  that failed to classify (method name + arity is enough — that is exactly
  what would have surfaced `moveto/5` and `enable/1` at the top of the list),
  and print the top ~30. Small change to a non-shipping example; high leverage
  for every future primitive decision.

## Existing / correctly-tracked — NOT re-filed

Re-verified against current code this pass:

- **#3277** (SCR-D6-2026-08-24-01 — `quest_fragment_dispatch_system`'s tail
  `QuestStageAdvancedBatch` write is the one non-defensive producer of five)
  — **still open, unchanged**. Now at `crates/scripting/src/fragment.rs:1963-1965`
  (was `:1928-1931`); still a bare `q.insert(player_entity,
  QuestStageAdvancedBatch(chained))` with no `get_mut`-then-`extend`, while
  the other four writers (`quest_advance.rs:467-473`, `quest_stages.rs:948-954`,
  `quest_stages.rs:1130-1136`, `fragment.rs:1475-1479`) all check first.
- **#3278** (SCR-D5-2026-08-24-01 — `Effect::Disable` has no production
  consumer, and its receiver resolution is narrower than its siblings) —
  **still open, unchanged**, and **strengthened by this pass's corpus run**:
  `Disable` is now measured at **488 real emissions** across the three games
  (89 Skyrim, 399 FO4+Starfield), all of them inert. `ReferenceEnableState::is_enabled`
  still has no caller outside `fragment/tests.rs`. See also
  SCR-D5-2026-08-27-02 above, which must be fixed alongside it.
- **#3279** (SCR-D5-2026-08-24-02 — `Effect::Conditional`'s `lower_statements`
  recursion has no explicit depth cap) — **still open**; `grep -n depth
  crates/scripting/src/translate/effects.rs` returns nothing.
- **#3159** (SCR-D5-2026-08-20-01 — no `Lock`/`Unlock`/`SetLockLevel`
  primitive; `1e9723ab`'s `Locked` marker has no clearing path) — still open;
  no `prim_lock` exists. SCR-D5-2026-08-27-02 is the same defect shape for
  `Disable`/`Enable`.
- **#3160** (SCR-D7-2026-08-20-01 — `m47-triggers.sh`'s counts are SOFT-only,
  so a script-attach regression cannot fail the gate) — still open;
  `docs/smoke-tests/m47-triggers.sh` unchanged.
- **#2668** (SCR-D4-NEW11-02 — `OffsetMap::to_original` is an unindexed linear
  scan over an already-sorted vec) — still open; `crates/papyrus` unchanged
  since 2026-08-19.

Closed and verified fixed since the last pass (do not carry forward):
**#2289**, **#2290**, **#2540**, **#2541**, **#2542**, **#3014**, **#3018**,
**#3019** (fix landed but is itself wrong — see SCR-D3-2026-08-27-01),
**#3161**, **#3250**, **#3287**, **#3312**, and the 08-24 workspace build
break (**SAFE-BUILD-2026-08-24-01**).

**SCR-D6-2026-08-20-01** (`HasPerk` reads a `Perks` component the player never
gets) is now **closed by code**: `byroredux/src/scene.rs:1372-1393` (#3158)
gives the player entity a `Perks::default()` at construction, with a comment
naming this exact condition arm. The residual FO3/FNV/Skyrim gap is a
source-data gap (`PRKR` is FO4+ only), not this defect.

## Considered and disproved / dropped

- **"The #3161 latch can strand `QuestStageFragments` empty for a whole
  session if the first `populate_scene_runtime` runs before `--scripts-bsa` is
  installed."** `mark_populated()` is indeed called *before* the
  `have_archive` fast-out, so the hazard is real in the abstract — but
  `world.insert_resource(build_script_provider(&args))` runs at
  `byroredux/src/scene.rs:814`, inside the `if let Some(ref path) = esm_path`
  block that precedes both the interior and exterior dispatch, and both
  `populate_scene_runtime` call sites (`scene/world_setup.rs:919`,
  `cell_loader/load.rs:524`) are downstream of that dispatch. Not reachable.
  Not filed. (The stale "Runs once per cell load" docstring left behind is
  noted under SCR-D5-2026-08-27-04.)
- **"`prim_add_item` requires an explicit count, so the common
  `AddItem(item)` one-argument form declines."** Disproved by the same
  compiler-fills-defaults fact that drives SCR-D5-2026-08-27-01, in the
  opposite direction: `additem args=3 count=2578` — the corpus contains no
  under-arity `AddItem` at all. `prim_add_item`'s shape is correct.
- **"`cinematic_retained_entities`'s `CellRoot` strip orphans the entity in
  `CellRootIndex`."** The 08-24 pass left this open for lack of budget.
  Chased to conclusion: `unload_cell_inner` builds `victims` from
  `CellRootIndex::map.remove(&cell_root)` — the whole entry is removed from
  the index before `retained` is filtered out of the victim list, so a
  retained entity is dropped from the index and loses its `CellRoot` in the
  same transaction. No stale index entry is possible. Not filed.
- **"`read_annotations`'s new `continue` (#3018) allows a log-spam storm from
  a malformed clip."** Bounded: `MAX_ANNOTATIONS = 65_536` caps the warn
  count per clip, and clip decode is a load-time operation, not per-frame.
  Not filed.
- **The marker-drain sweep.** All 46 `impl Component for` types in
  `crates/scripting` were enumerated and matched against the Pattern-A drain
  list, the Pattern-B table, and the persistent-state catch-all. No gap.
  Reported as a matrix row, not a finding.

## Future-Phase Readiness

- **The effect table needs an input-shaped gate, not just a decline test.**
  SCR-D5-2026-08-27-01 is the third finding in three passes (with #3159 and
  #3278) saying the same thing from a different angle: primitives are being
  written and tested against hand-constructed `.psc` shapes, then shipped
  against a `.pex` frontend nobody measures them on. #2289's decline tests
  raised confidence that a primitive *rejects* what it should reject; nothing
  yet checks that it *accepts* what real content actually contains. The
  cheapest structural fix is the decline-reason tally SCR-D5-2026-08-27-04
  asks for, promoted from an example into something a smoke gate can read.
- **`Disable`/`Enable`/`ReferenceEnableState` is now a three-part fix, not
  two.** #3278 (add a consumer), SCR-D5-2026-08-27-02 (add `Enable`), and
  #3278's receiver-resolution half must land together; any two without the
  third leaves the runtime in a worse state than none of them.
- **`QuestStageAdvancedBatch` still has five hand-rolled writers.** The 08-24
  pass proposed a shared `push_quest_stage_advances` helper; nothing landed,
  and #3277 remains the one writer that gets it wrong. This range added no
  new writer, so the count is stable — but the structural fix is still the
  right one.
- **Dims 1–4 are stable.** Two full ranges with no functional change to
  `crates/pex` or `crates/papyrus`, all standing invariants re-verified, and
  the last unguarded safety mechanism in the domain (`translate_pex`'s
  `catch_unwind`) gained a test this range. Future passes can reasonably
  spot-check these unless a commit touches them.

## Findings Count

**6 new: 0 CRITICAL / 0 HIGH / 2 MEDIUM / 4 LOW.**

By dimension — **Dim 1** (`.pex` reader & opcode decode): 0 (unchanged,
verified clean). **Dim 2** (decompiler CFG & lift): 0 (unchanged). **Dim 3**
(control-flow / boolean / lower): 1 LOW. **Dim 4** (`.psc` lexer & Pratt
parser): 0 new (#2668 carried). **Dim 5** (recognizer-chain soundness): 2
MEDIUM + 2 LOW. **Dim 6** (scripting runtime systems): 1 LOW (3 existing
carried). **Dim 7** (engine attach & trigger wiring): 0 new (1 existing
carried). **Dim 8** (Havok idle / cinematic slice): 0 new — `crates/hkx`'s two
open findings (#3014, #3018) both closed correctly this range, with real
malformed-input fixtures.

**Untrusted-input robustness verdict**: CLEAN for `.pex`, `.psc` and `.hkx`.

**99.996% decompile-rate claim**: harness re-run this pass over all three
corpora; `decompile_script` sits inside the tally and a panic is caught and
**not** counted as success (`fragment_coverage.rs:127-131` mirrors
`pex_corpus_smoke`'s structure). Claim stands.

**`.psc`-vs-`.pex` fidelity gate**: `recognizes_da10_and_reproduces_hand_builder`
and `da10_pex_reproduces_hand_builder_byte_for_byte` (#1740, `#[ignore]`-gated
on Skyrim data) both still assert byte-equality against `da10_main_door(..)`.
Unchanged.

TALLY: CRITICAL=0 HIGH=0 MEDIUM=2 LOW=4
