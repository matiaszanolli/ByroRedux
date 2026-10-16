# Parser Discipline Audit: 2026-10-15

**HEAD**: 5f7ff5920 · **Baseline**: `AUDIT_PARSERS_2026-10-12.md` (HEAD c1a99c25e) · **Audited**: Dims 1–6. Each dimension's Paths had commits since c1a99c25e:
- **hkx (25 commits)**: the Havok 2014 / FO4 clip and skeleton reader (`a7bbba273`), the full behavior generator tree, modifiers, sync and timers (`49529db7c`, `6f48ffbcd`), FO4 reference-frame root motion (`e8026e737`), archetype idles, PlayAnimation.
- **menuxml (7)**: fixes for the baseline's findings (#5658, #5668, #5669, #5670, #5693, #5329).
- **sfmaterial (4)**: #5465, #5672, #5673.
- **bgsm and facegen**: #5010, #5011.
- **bsa (3)**: #5660/#5073, `.fuz` test (#5464).
- **game-detect (1)**: #5739.
- **asset_provider (61, mostly scripting/graph wiring)**.

**Unchanged since baseline (skimmed)**: none.

## Method and evidence

Three dimension agents covered: hkx; bsa, sfmaterial, bgsm and facegen with the asset_provider consumers; menuxml and game-detect plus Dim 4 for all seven crates. Each finding of MEDIUM or above was re-checked at HEAD by the orchestrator: D1-01, D2-01, D2-03, D5-01, and D5-03's cycle check. Probes were standalone crates outside the repo with a shared `CARGO_TARGET_DIR`, reading vanilla archives only. Nothing in the repo was touched apart from this report.

**Default lane** (`cargo test -p byroredux-{bsa,bgsm,sfmaterial,hkx,facegen,menuxml,game-detect}`): all green.

| Crate | Unit | Other |
|---|---|---|
| bsa | 114 | + 1 ba2_real |
| bgsm | 34 | + 1 parse_all |
| sfmaterial | 42 | + 6 header_smoke |
| hkx | 36 | |
| facegen | 31 | |
| menuxml | 58 | |
| game-detect | 59 | |
| doc-tests | 4 | |

**Real-data suites, strict lane** (`BYROREDUX_REQUIRE_GAME_DATA=1 cargo test --release -p <crate> -- --ignored`, one crate at a time, all data present): all green, nothing skipped.

| Crate | Passed | Breakdown |
|---|---|---|
| bsa | 31 | 11 unit, 11 `ba2_real`, 8 `bsa_real`, 1 `csg_real` |
| bgsm | 3 | |
| sfmaterial | 4 | `real_cdb` |
| hkx | 7 | includes the Skyrim LE parity test (the WINE-prefix fallback resolves) |
| facegen | 4 | |
| menuxml | 9 | 4 Oblivion, 3 FO3, 2 FNV |

**HKX vanilla census (probe; not a committed gate)**:
- **FO4**:
  - 13,990 clip-bearing files: 13,133 decode and 857 fail. 856 carry `hkaLosslessCompressedAnimation` (D5-01); 1 is a bare `hkaAnimationContainer`.
  - Skeletons 121/121, behaviors 328/328, characters 222/222.
  - Every clip that decodes and carries `hkaDefaultAnimatedReferenceFrame` yields one (13,162).
- **Skyrim SE behaviors**: 420/423; the 3 `*_lod.hkx` files lack `hkbBehaviorGraphStringData`.
- **Robustness and cost**: no panic on any vanilla file. The slowest decode is 35 ms.
- **Census tool discrepancy**: the gates agent's run of the in-repo `clip_census_ba2` example reported "0 rejects, 1,806 non-clip" for the base FO4 archive. That tool classes a file with no spline-compressed class as non-clip, so the lossless clips land in the skip bucket. The direct decode above is the evidence, and the gap is part of D4-01.
- **Sample-cap headroom**: the FO4 vanilla maximum is 584,155 spline samples (`npcfmagnolia`) against `MAX_TRANSFORM_SAMPLES` 1 M. That is 1.7× headroom, not the documented 8× (D4-01).

**Hostile-input probing**: the HKX agent did not run its crafted and mutated packfile fuzz. D1-01, D1-02, D2-01, D2-02, D2-03 and D3-01 are static findings at HEAD, confirmed by reading the code:
- **D1-01**: a blender iterates up to `MAX_STATES` children per level to depth 32, with no visited set.
- **D2-01**: `pop_due` re-selects an event whose `at + period == at`. Both a file-authored speed around 1e30 and an ordinary short loop under a large `now` reach this, because `f32` has an ULP of 0.125 at 2^20 s.
- **D2-03**: `i32::MIN - 1` under the dev profile's default overflow checks.

Each fix needs a fixture test.

## Executive Summary

| Severity | NEW | Existing (open) re-confirmed |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 2 | 0 |
| MEDIUM | 6 | 1 (#5667) |
| LOW | 13 | 5 (#5663, #5664, #5665, #5666, #5446) |

- **HIGH**: both are main-thread hangs reachable from a mod's HKX file, in the new FO4 behavior-graph surface:
  - D1-01: exponential generator, state-machine and modifier expansion over shared or cyclic pointers.
  - D2-01: the graph runtime's trigger re-arm never advances time once `at + period == at`.
- **MEDIUM**:
  - Inline-reference nesting depth is unbounded (D1-02).
  - Reference-frame root motion is silently dropped, and NaN can reach the root keys (D2-02).
  - A dev-build overflow panic (D2-03).
  - There is no FO4 HKX corpus gate or v11 default-lane fixture (D4-01).
  - 856 FO4 lossless clips are unread (D5-01).
  - One bad clip discards and caches away a whole object graph (D5-02).
- **LOW**: diagnostics and memo residuals of #5672/#5010, a v11 × 32-bit offset gap, sweep-matrix holes, the never-run CI lane, doc rot, and three `--profiles` edges.

**Baseline status in these crates**:
- **Fixed, and the fixes hold**: #5658, #5659, #5660/#5073, #5661, #5668, #5669, #5670, #5671, #5672 (residuals D2-04/D2-05), #5673, #5691, #5693, #5010 (residual D5-03) and #5011.
- **Still open, code unchanged**:
  - #5663: the `open_shared` stamp.
  - #5664: the shadow count on reuse.
  - #5665: a panicking CDB build is not memoised.
  - #5666: `PREFETCH_GATE` ordering.
  - #5667: BC1/2/3 allocate before checking the payload. Additional evidence for #5667: undecodable candidates are not memoised by resolved path, so each spelling re-fetches and re-allocates.

**ID map** (agent scratch IDs → report IDs; cross-references inside finding bodies may use either):

| Report ID | Scratch ID |
|---|---|
| D1-01, D1-02, D2-01, D2-02, D3-01, D4-01, D5-01, D5-02 | unchanged (hkx) |
| D2-03 | hkx D1-03 |
| D2-04 | archives D2-21 |
| D2-05 | archives D2-22 |
| D5-03 | archives D5-20 |
| D5-04 | archives D5-23 |
| D4-01 | also absorbs gates D4-40 |
| D4-02 | gates D4-41 |
| D4-03 | gates D4-42 |
| D4-04 | gates D4-43 |
| D4-05 | gates D4-48 |
| D4-06 | gates D4-44 |
| D6-01 | gates D6-45 |
| D6-02 | gates D6-46 |
| D6-03 | gates D6-47 |

## Findings

### HIGH

#### PAR-D1-2026-10-15-01: The behavior-graph reader follows shared object pointers with no visited set, so the generator, state-machine and modifier walks are exponential in depth on a DAG or a cycle. A small behavior file can hang or exhaust memory on the main thread.
- **Severity**: HIGH (a hang or OOM reachable from a mod file)
- **Dimension**: Size Discipline
- **Location**:
  - `crates/hkx/src/behavior.rs:1089-1217` (`read_generator`)
    - the blender children loop at `:1188-1203` re-enters `read_generator` per child pointer
    - the selector loop is at `:1164-1170`
    - the state-machine arm `:1119-1122` calls `read_machine_at` → `read_state` per state pointer (`:1363-1368`)
  - `:1226-1333` (`read_modifier`): the `hkbModifierList` loop at `:1321-1327` and the `child(...)` arms
- **Status**: NEW. There is no open or closed issue about a visited set or DAG blowup in behavior decoding.
- **Description**:
  - The caps bound each array (`MAX_STATES` 1024 entries) and the recursion depth (`MAX_GENERATOR_DEPTH` 32, `MAX_MODIFIER_DEPTH` 16). Nothing bounds the total number of nodes visited.
  - Every pointer is followed again each time it is reached, and each visit builds a fresh owned subtree (`Generator`, `BlendChild`, `Arc<ModifierNode>`).
  - Example: a blender whose two child entries both point back at itself expands into roughly 2^32 nodes before the depth cap returns `None`.
  - Example: a modifier list with K entries pointing at itself costs K^17 calls. At K = 3 that is about 1.3e8 allocations; at K = 1024 it never completes.
  - Such a file is a few kilobytes. All three decode call sites are on the main thread with no time or size budget: `object_graph.rs:63`, `actor_graph.rs:99` and `archetype_idle.rs:173`, reached from `app_step.rs:93/1032/1044`, `cell_loader/load.rs:731-733` and `scene/world_setup.rs:1135-1137`.
  - `Packfile::class_at` is a linear scan over all objects (`packfile.rs:282-287`) on every visit, which multiplies the cost further.
- **Evidence**:
  - Static: there is no `HashSet` or memo in `read_generator`, `read_machine_at` or `read_modifier`. `depth` is the only bound.
  - Vanilla is unaffected: all 751 vanilla graphs decode, the slowest in 35 ms (probe).
- **Trigger Input**: an `hkbBlenderGenerator` / `hkbManualSelectorGenerator` / `hkbStateMachine` / `hkbModifierList` whose child pointers (global fixups) target an ancestor or a shared descendant, with ≥ 2 such pointers per level.
- **Impact**: one mod behavior file named by a placed object's NIF or by an FO4 RACE subgraph freezes the engine at cell load, or aborts it on OOM.
- **Suggested Fix**:
  - Memoise on object offset per decode. A `HashMap<usize, Generator>` / `HashMap<usize, Arc<ModifierNode>>` turns the walk into O(objects); `Arc` already lets modifier nodes be shared.
  - Alternatively, carry an on-stack set that rejects a cycle and a global visit budget (e.g. 64 K nodes per file) that returns `InvalidData`.
  - Add a fixture: a self-referencing blender and a self-referencing modifier list must decode or fail in bounded time.

#### PAR-D2-2026-10-15-01: The graph runtime's trigger re-arm adds `period = duration / playback_speed` without checking that time advances. A huge or tiny file-authored `playbackSpeed` loops `step()` forever on the main thread.
- **Severity**: HIGH (main-thread hang reachable from a mod file). Owner overlap: `/audit-scripting` Dim 5 owns HKX playback. Filed here because the decoder passes the field through unvalidated and this audit's brief asked about runtime loops.
- **Dimension**: Error Semantics
- **Location**:
  - `crates/scripting/src/animation_graph.rs:795-797` (`speed` is accepted when finite and > 0; `period = registered.duration / speed`)
  - `:948-951` (`rearm`: `at: fired.at + period`)
  - `:985-1011` (`step`: `while let Some(due) = self.pop_due(limit)`)
  - Called every frame from `object_graph_step_system` (`:1417-1427`)
  - Decode side: `crates/hkx/src/behavior.rs:1051` (`playback_speed: file.f32(...)`, unchecked)
- **Status**: NEW
- **Description**:
  - A looping clip's trigger is re-armed at `fired.at + period`.
  - If `period` is 0 (underflow, e.g. `duration / 3e38`) or below half an ULP of `fired.at`, the new entry has the same `at`. It is then due again within the same `limit`.
  - When the trigger's event changes no machine state, `send_event` returns `None` without touching `scheduled` (`:884-890`). Two cases do this: an event no transition answers, or an event id `-1`, which `transition_of` refuses at `:846`. The loop then never exits.
  - Each pass also pushes onto the raised-event list (`note_raised`), so memory grows while the frame hangs.
  - Even when `at` does advance, a period of 1e-6 costs dt/period ≈ 16 K `send_event` + `terms()` evaluations per frame.
  - Absorption also arrives with time alone. With `speed = 1e3` on a 1 s clip, `at + 1e-3 == at` once graph time passes about 2^14 s (4.5 h).
- **Evidence**: static. `pop_due` selects `at <= limit` (`:955-965`). `rearm` is unconditional for looping clips. `send_event`'s early `return None` leaves the rearmed entry in place.
- **Trigger Input**: an `hkbClipGenerator` with mode Looping, `playbackSpeed` ≥ ~1e30 (or a duration/speed ratio below the f32 ULP of the graph clock), and a trigger whose event no transition answers.
- **Impact**: the engine freezes on the first frame the trigger is due, whether in a placed object or an FO4 actor graph.
- **Suggested Fix**:
  - Clamp the playback speed at decode or at the runtime boundary to a sourced range, and reject non-positive or absurd speeds as "not looping".
  - In `rearm`, require `fired.at + period > fired.at`. Otherwise drop the re-arm and warn once.
  - Cap the iterations of the `step()` loop per frame, as `run_events` already does with `MAX_CHAIN` at `:924`.


### MEDIUM

#### PAR-D1-2026-10-15-02: `inline_references` bounds the *count* of inlines (64) but not the resulting nesting depth. A self-referencing behavior nests about 64 × 33 machines, which every recursive runtime walker then descends.
- **Severity**: MEDIUM (a possible stack overflow from a mod file, not measured)
- **Dimension**: Size Discipline
- **Location**:
  - `crates/hkx/src/behavior.rs:845-898`: `inline_machine` recurses into each newly inlined machine (`:887-889`), and `inline_references_with` recurses through `resolve`.
  - Recursive consumers of the result:
    - `remap_machine_events` / `remap_machine_variables` / `offset_machine_modifier_ids` / `machine_modifier_id_bound` (`:700-807`)
    - derived `Clone` / `Drop` / `PartialEq`
    - the runtime `MachineRun::enter` (`animation_graph.rs:254-269`), `dispatch` (`:861-882`), `schedule_from::walk` (`:779-815`) and `eval_machine`
- **Status**: NEW
- **Description**:
  - One decoded file nests at most 33 machines (`MAX_GENERATOR_DEPTH`).
  - Inlining pushes each referenced graph's root machine under the referencing state, then recurses into it. A graph whose deepest state references its own path is served from `actor_graph.rs`'s `decode_behavior` cache 64 times, giving a chain of about 2,100 nested machines.
  - Nothing caps that depth. The recursive walkers above run on the main thread (8 MB stack) every frame.
  - Separately, each inline interns its events and variables through linear `event_id` / `variable_id` scans (`:812-837`). That makes inlining O(E² · inlines): up to 64 graphs × 4096 events is about 3e10 string comparisons.
- **Evidence**: static. `budget` (`MAX_INLINED_REFERENCES`) is decremented per resolve and is the only bound. `MAX_REFERENCE_HOPS` applies only to `start_clip`. Stack use at about 2,100 levels was not measured.
- **Trigger Input**: a v11 behavior whose nested start-state chain ends in an `hkbBehaviorReferenceGenerator` naming its own file, or a 64-file chain.
- **Suggested Fix**: carry a total nesting-depth budget through `inline_machine`, so an inlined machine counts against the same 32-deep limit as decoded nesting. Intern events and variables through a `HashMap`.

#### PAR-D2-2026-10-15-02: FO4 reference-frame decode silently drops the root motion on any malformed field, and finite samples whose difference overflows f32 put NaN into the root translation keys. The #5691 overflowing-span policy is not applied.
- **Severity**: MEDIUM (silently wrong output from a mod file)
- **Dimension**: Error Semantics
- **Location**:
  - `crates/hkx/src/animation.rs:639-668` (`read_reference_frame`)
    - every failure path is `.ok()?` / `return None` with no warning (`:646-664`)
    - samples are checked per value with `is_finite` (`:662`)
  - Consumer: `byroredux/src/asset_provider/animation.rs:401-409` (`reference_frame_position`: `a + (b - a) * t`), added to the root track at `:494-497`
- **Status**: NEW. This is the reference-frame twin of the closed #5691 spline-scalar case.
- **Description**:
  - Two separate things go wrong here.
  - **Silent drop.** These all decode as `reference_frame: None`, exactly like a clip that has no reference frame at all: a truncated sample table, an unbound array, `count > 65,536`, a non-finite duration, or one non-finite sample. The clip then plays in place, and no log names the file. Contract (c) wants a warning or an error, never a silent partial result.
  - **Overflowing span.** Each sample only has to be finite. Adjacent samples such as `x = 3e38` and `x = -3e38` give `b - a = -inf`. At `t = 0`, which is every frame landing exactly on a sample, `inf * 0` is NaN. A NaN translation key then reaches the root bone and root-motion accumulation.
- **Evidence**:
  - Static for the decoder paths.
  - The vanilla census is clean: 13,162 FO4 clips carry the class, and every one that decodes yields a reference frame. Vanilla is therefore not affected.
- **Trigger Input**: an `hkaDefaultAnimatedReferenceFrame` with sample count > 65,536, a truncated or unbound sample array, or two adjacent finite samples of opposite sign above about 1.7e38.
- **Suggested Fix**:
  - Return a `Result`-shaped outcome, or a flag the consumer warns on with the clip path (as `dropped_bones` does).
  - Reject sample tables whose adjacent differences are non-finite, following the #5691 policy.

#### PAR-D2-2026-10-15-03: `read_bone_weights` computes `offset - 1` on a file-controlled `i32`. A binding's `offsetInArrayPlusOne = 0x80000000` panics with overflow in dev builds, on the main thread, with no `catch_unwind`.
- **Severity**: MEDIUM (a panic reachable from a mod file in the engine's default dev profile, which has overflow checks on; release wraps and drops the binding)
- **Dimension**: Error Semantics
- **Location**: `crates/hkx/src/behavior.rs:1074-1075`
  ```rust
  let offset = file.u32(element + BINDING_OFFSET_IN_ARRAY, "binding array offset")? as i32;
  if let (Ok(variable), Some(bone)) = (usize::try_from(variable), usize::try_from(offset - 1).ok()) {
  ```
- **Status**: NEW
- **Description**:
  - When the raw u32 is 0x80000000, the cast gives `offset = i32::MIN`, and `offset - 1` overflows.
  - The decode callers listed in D1-01 have no `catch_unwind`. The only HKX-adjacent one is in `streaming/pre_parse.rs` and covers NIF only. A panic here kills the dev engine during cell load.
  - This is the same class as #5008 (a file-controlled panic that only debug builds hit).
- **Trigger Input**: an `hkbBoneWeightArray` binding whose member path is `boneWeights` and whose `offsetInArrayPlusOne` field is 0x80000000.
- **Suggested Fix**: `offset.checked_sub(1).and_then(|o| usize::try_from(o).ok())`. Better, read the field as `u32` and use `checked_sub(1)`.

#### PAR-D4-2026-10-15-01: The Havok 2014 (v11) surface has no real-data census and no default-lane fixture. Clips, skeletons, behaviors, THREECOMP48, the predicate area, `LAYOUT_2014`, the modifiers and the reference frame are each gated by at most one hand-picked file.
- **Severity**: MEDIUM (a missing gate; it already hid the 856 rejects in D5-01)
- **Dimension**: Corpus Gates
- **Location**:
  - `crates/hkx/src/animation.rs:2329-2369` (`fallout_4_skeleton_and_clip_decode`: one skeleton, one clip)
  - `crates/hkx/src/behavior.rs:1606-1652` (`fallout_4_havok_2014_graphs_decode`: 3 graphs)
  - The FO4 census lives only in examples (`examples/clip_census_ba2.rs`, `behavior_census_ba2.rs`), which no lane runs.
  - `packfile.rs` `fixtures::PackfileBuilder` writes v8 only (`0x0C = 8`, 0x30 section headers). The only v11 mention in the default lane is the contents-mismatch rejection at `packfile.rs:665`.
- **Status**: NEW. #5446 (dimension-gate fixtures) is separate and not re-filed.
- **Description**:
  - The SE clip census (`skyrim_se_spline_dimensions_census_stays_under_the_gate_ceilings`) pins 6,126 clips with an empty reject set. Nothing comparable exists for:
    - FO4: 13,990 clips, 121 skeletons, 328 behaviors, 222 characters
    - SE behaviors: 423
  - In CI, no test exercises:
    - the 0x40 section header
    - a non-zero predicate area
    - `animation_type == 3`
    - `decode_three_component_48`
    - `read_reference_frame`
    - any `LAYOUT_2014` offset
    - `read_modifier`
  - Every one of those paths could regress with the default lane still green.
  - The strict nightly `parsers` job would run the ignored FO4 tests, but it has no runner (count 0, per the skill). The ignored tests do fail under `BYROREDUX_REQUIRE_GAME_DATA`, so they are not vacuous there.
- **Evidence**: census numbers in `/tmp/audit/parsers/probe-hkx/census.txt`. Of the 3 SE `*_lod.hkx` behaviors that fail (`frostbitespiderbehavior_lod.hkx` and two more), none is pinned anywhere.
- **Suggested Fix**:
  - Add `fallout_4_hkx_census_is_exact`: exact clip, skeleton, behavior and character counts; the lossless set as a named, counted bucket; the reject set otherwise empty; a reference-frame count.
  - Add an SE behavior census.
  - Extend `PackfileBuilder` with a version-11 mode (0x40 headers, predicate size) plus unit tests for THREECOMP48 decode, type-3 gating and the reference-frame offsets.
- **Merged from the gates agent (its PAR-D4-2026-10-15-40, same gap, independent census)**:
  - **Severity**: MEDIUM
  - **Dimension**: Corpus Gates
  - **Location**:
    - `crates/hkx/src/animation.rs:2331-2373` (`fallout_4_skeleton_and_clip_decode`): one skeleton and one clip, `1hm\walkforward.hkx`.
    - `crates/hkx/src/behavior.rs:1607-1650` (`fallout_4_havok_2014_graphs_decode`): three named graphs.
    - `crates/hkx/src/animation.rs:72-81`: the `MAX_TRANSFORM_SAMPLES` rationale, which cites the SE census only.
    - `crates/hkx/src/animation.rs:83-89`: the #5847 ceilings, documented as SE-census-derived.
    - `crates/hkx/examples/clip_census_ba2.rs` and `behavior_census_ba2.rs`: whole-archive FO4 censuses with **0 assertions**, which no lane runs.
    - `.github/workflows/real-data-gates.yml:208`: the `parsers` loop, which does run the two FO4 tests.
  - **Status**: NEW. The SE census (`skyrim_se_spline_dimensions_census_stays_under_the_gate_ceilings`) is exact. FO4 support (a7bbba273, 60082cc55, 49529db7c, all 2026-10-14) added a second packfile generation with no equivalent. Distinct from #5446, which covers the unit fixtures.
  - **Trigger Input**: n/a (gate gap). Regression class: any change to `Packfile::parse` v11 (the predicate area or the 0x40-byte section headers), spline type 3, THREECOMP48, or `hkaDefaultAnimatedReferenceFrame` that breaks clips other than `1hm\walkforward.hkx` or graphs other than the three named ones.
  - **Description**:
    - FO4 humanoids now run graph-driven locomotion and idles from this reader, but the strict lane checks 2 clips and 3 graphs out of 15,320 files.
    - The census code already exists, but only as print-only examples. That is the "census exists but cannot fail" pattern #5326 / #5671 removed for SE.
    - The dimension ceilings were never re-measured against FO4. The doc comment calls `MAX_TRANSFORM_SAMPLES` = 1,000,000 "8× the measured maximum" (SE: 124,821). FO4 vanilla reaches 584,155, so the real headroom is 1.71×.
    - A modded voice/lip clip about 1.7× longer than Magnolia's line would be rejected, and no gate would show how close vanilla sits.
    - The two FO4 test helpers (and `behavior.rs:1502-1516`'s SE helper) also do not make a set `BYROREDUX_FO4_DATA` / `_SKYRIMSE_DATA` binding (#3850). A set-but-wrong override silently skips outside the strict lane, whereas `animation.rs:1425-1431` asserts `is_dir()`.
  - **Evidence**: probe `fo4hkx`, real `decode_spline_animation` / `decode_behavior_graph`, release, every BA2 in the FO4 Data dir.

    | Scope | `.hkx` files | Clips decoded | Non-clip (`MissingClass`) | Rejects | Dropped-track clips | `behaviors\` graphs decoded |
    |---|---|---|---|---|---|---|
    | `Fallout4 - Animations.ba2` | 15,320 | 13,514 | 1,806 | 0 | 0 | 328 |
    | All BA2s (including DLC, CC, SS2, UFO4P) | 18,863 | 16,187 | 2,676 | 0 | 0 | 461 (0 errors) |

    - Maximum frames: 6,149. Maximum tracks: 120.
    - Maximum samples, vanilla: **584,155** (`sound\voice\fallout4.esm\npcfmagnolia\0019ddf3_1.hkx`, 95 × 6,149). Mods: 559,360 (SS2).
    - No non-finite sample appeared in any decoded clip.
    - SE side (probe `sebeh`): 420 SE `behaviors\` graphs decode. 3 `*_lod.hkx` graphs (frostbitespider, goat ×2) fail `MissingClass("hkbBehaviorGraphStringData")`. 343 of 345 character files decode. None of this is gated either; only the swinging blade and the Nordic lever are pinned.
  - **Impact**: today's vanilla state is fully green, but nothing holds it there. A v11 regression would surface only as FO4 actors sliding or idling wrong, and the cap documentation overstates its margin by about 4.7×.
  - **Related**: #5326, #5671, #5316, #5847, #5446, #3850.
  - **Suggested Fix**:
    - Promote `clip_census_ba2` to an `#[ignore]` test over `Fallout4 - Animations.ba2`, pinned exactly at 15,320 / 13,514 / 1,806, rejects as the empty set, 0 partial, max frames 6,149, and `max_samples <= MAX_TRANSFORM_SAMPLES`.
    - Add a graph census pinned at 328 for FO4 and 420 plus the named 3-file reject set for SE.
    - Correct the `MAX_TRANSFORM_SAMPLES` doc to cite the FO4 maximum, or re-derive the cap from it.
    - Make the three helpers' env override binding.

#### PAR-D5-2026-10-15-01: 856 vanilla Fallout 4 clips are `hkaLosslessCompressedAnimation`, which the decoder does not read. They fail as "class missing", and the only report is a debug log.
- **Severity**: MEDIUM. The 841 first-person clips have no consumer yet. The 15 third-person clips are reachable now through FO4 actor graphs' lazy clip resolution: 12 `animations\paired`, 2 `furniture\coupleholding`, 1 `deathclaw`, plus 1 `pipboy`. Under the skill rule "vanilla content failing = HIGH", this becomes HIGH once first-person or paired playback is consumed.
- **Dimension**: Decode-Consumer Wiring
- **Location**:
  - `crates/hkx/src/animation.rs:400` (`pack.object("hkaSplineCompressedAnimation")?` is the only clip class read)
  - Consumers: `actor_graph.rs:272-274` (`log::debug!`, then `job.lazy.fail`), `object_graph.rs:99-103`, `archetype_idle.rs:152-154`
- **Status**: NEW. There is no issue for lossless or non-spline FO4 clips.
- **Description**:
  - The FO4 census: 13,990 clips under `\animations\`.
    - 13,133 decode.
    - 744 are `hkaLosslessCompressedAnimation` + `hkaDefaultAnimatedReferenceFrame`.
    - 112 are `hkaLosslessCompressedAnimation` alone.
    - 1 is a bare `hkaAnimationContainer`.
  - All 857 fail with `HKX object class 'hkaSplineCompressedAnimation' is missing`, which reads as file corruption rather than "unsupported compression".
  - In `actor_graph.rs` the failure logs at debug level. The follow-up log at `:299` then says the clip was "not found", which misdiagnoses it.
- **Evidence**: `/tmp/audit/parsers/probe-hkx/census.txt` and `missing.txt`
  - first-person 841, e.g. `meshes\actors\character\_1stperson\animations\laserpistol\wpnafterjigglefireautoadd.hkx`
  - third-person e.g. under `meshes\actors\character\animations\paired\`
- **Trigger Input**: a vanilla FO4 clip whose animation object is `hkaLosslessCompressedAnimation`.
- **Suggested Fix**:
  - Return `UnsupportedLayout("hkaLosslessCompressedAnimation")` when the container's animation is a known non-spline class, and warn in the consumers.
  - Decode the lossless format (Havok 2014 quantised key arrays), or record it as a parked format in the census (see D4-01).

#### PAR-D5-2026-10-15-02: Object-graph loading discards the whole graph, and caches the failure, when any one clip is missing or does not decode. Actor graphs drop a skeleton decode error with no log at all and then report every clip as "not found".
- **Severity**: MEDIUM
- **Dimension**: Decode-Consumer Wiring
- **Location**:
  - `byroredux/src/asset_provider/object_graph.rs:99-103`: the `?` on `fetch(...).and_then(decode)` returns `None` from `load_graph`. `:178` caches `None` per project, so the prop never animates.
  - `:8-10`: the module doc still says "Skyrim Special Edition's 64-bit Havok 2010 packfiles only". v11 has decoded since 60082cc55.
  - `byroredux/src/asset_provider/actor_graph.rs:121`: `decode_skeleton(&bytes).ok()` with no log. `:271` (`job.skeleton.as_ref()?`) then yields `None` for every clip, and `:299` logs it as "not found".
  - Behavior and clip decode errors throughout are `log::debug!` only (`actor_graph.rs:100`, `:273`; `object_graph.rs:58-67`, `:101`; `archetype_idle.rs:148/153/174`).
- **Status**: NEW. The `resolve_wanted` restart bug at `actor_graph.rs:307-318` is #5861 and is not re-filed.
- **Description**:
  - Contract: a bad asset costs that asset, and a partial result is signalled.
  - In the object path, one missing or unsupported clip costs every state of the graph, including clips that decoded. Given D5-01, that includes any FO4 placed object whose graph names one `hkaLosslessCompressedAnimation` clip.
  - In the actor path, a skeleton that fails to decode, such as a mod rig above `MAX_SKELETON_BONES` or one with a non-finite pose, leaves no trace at the default log level. The symptom presents as missing files.
- **Suggested Fix**:
  - Register the clips that decode and skip the rest with a `warn!` naming project, state and clip.
  - Warn on skeleton decode failure (as `archetype_idle.rs:148` does at debug, but at warn level).
  - Raise the decode-failure logs to `warn!` once per path.


### LOW

#### PAR-D2-2026-10-15-04: `MaterialIndex` drops an `Objects` row whose `PersistentID` columns drift off `{Dir: u32, Ext: u32}` silently. #5672's degradation counter covers `DBID` and `Parent` but not the key columns #5465 now reads by name
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/sfmaterial/src/index.rs:749-760`. The `match (u32_field(pid, "Dir"), u32_field(pid, "Ext"))` falls to `_ => None` with no count. The warning at `:762-768` counts only `dbid_misses` and `parent_misses`.
- **Status**: NEW. It is the residual of #5672 (fa6666053, closed) as reshaped by 30bdbcc3b (#5465, closed). The pre-#5465 `n == 3` arm was equally silent. Searches for "PersistentID" in open and closed issues found only #5465 and #5502.
- **Trigger Input**: a mod or DLC `materialsbeta.cdb` whose `BSResource::ID` class declares `Dir` or `Ext` under another name or type (e.g. `u64`), or whose `ObjectInfo.PersistentID` is not an inline object.
- **Description**:
  - #5672 reasoned that "an unkeyed object only loses its own lookup" and made `DBID` and `Parent` misses warn once per build.
  - A `PersistentID` shape drift unkeys *every* row. `objects_by_key` ends empty, `MaterialIndex::build` returns `Ok`, and every `.mat` lookup misses into `apply_cdb_pbr_fallback`.
  - The only signal is the info-level "material index built (0 keyed objects)" at `byroredux/src/asset_provider/material/cdb.rs:175-179`. That is the wholesale silent degradation #5672 was filed to remove, moved onto the field #5465 just made name-dependent.
  - No test covers it. `degraded_objects_rows_and_parent_walks_warn_instead_of_truncating_silently` re-types `Parent` only.
- **Evidence**: code reading at HEAD (`index.rs:735-769`). Vanilla is clean: the base CDB keys 500,403 objects, which equals the Objects row count in the baseline `cdbprobe`.
- **Impact**: a schema-drifted mod CDB renders every Starfield material it should own as PBR-fallback, with no warning naming the cause.
- **Related**: #5672, #5465, #5320, #5502.
- **Suggested Fix**: count `pid_misses` beside `dbid_misses` and `parent_misses` and include them in the same once-per-build warning. Return a typed error (in the style of `RowInstanceMismatch`) when rows exist but none keyed. Extend the #5672 fixture to re-type `PersistentID.Dir`.

#### PAR-D2-2026-10-15-05: the #5672 Parent-walk warnings fire once per `collect_settings` call, with no memo, so a cyclic or over-deep mod chain floods the log on every merge. A cycle back through the start object stays silent
- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**:
  - `crates/sfmaterial/src/index.rs:381-405` (`collect_settings`).
  - `:386`: `.filter(|&&p| p != obj)` ends the walk silently on a return to the start.
  - Called at `:302` (root) and `:338` (each layer material) in `lookup`.
  - Consumer: `byroredux/src/asset_provider/material/cdb.rs:230-242` (`lookup_cdb_material`, not memoised) ← `provider.rs:368` ← `merge.rs:134`, per mesh merge.
- **Status**: NEW. It is the residual of fa6666053 (#5672). The issue asked to "warn once per lookup", which landed, but a lookup is per mesh and there is one walk per layer.
- **Trigger Input**: a mod CDB where a widely shared ancestor (for example a layered template that many materials inherit) has a `Parent` cycle not through the looked-up object, or a chain deeper than 64.
- **Description**:
  - `MaterialIndex` is process-lifetime, but the warnings are emitted per walk.
  - Every merge of every material under the bad ancestor warns, once for the root and once per layer material. Starfield cells merge thousands of shapes.
  - The `Objects`-table half of #5672 warns once per build. The walk half has no equivalent memo.
  - A cycle through the start (`A → B → A`, or the self-loop `A → A`) is dropped by the `p != obj` filter with no warning, while the structurally identical `A → B → C → B` warns. Both describe the same corrupt graph.
  - No inheritance is lost in either cycle case: every node on the cycle has been visited by the time of the revisit. So the "inheritance truncated" wording is accurate only for the depth cap.
- **Evidence**: code reading. `lookup` is uncached (`cdb.rs:234-240` re-walks per call). The #5672 test asserts on message content because "the binary's other cycle test warns concurrently", which shows the repetition in miniature.
- **Impact**: log noise on corrupt mod CDBs only (vanilla has 0 cycles and a maximum depth of 24). It is diagnostic only.
- **Related**: #5672, #5502, PAR-D2-2026-10-12-04 (same once-per-path flood shape in menuxml).
- **Suggested Fix**: detect cycles and over-deep chains once at build time over `parents` (an O(n) colouring walk), warn once with a count, and keep the per-walk guards silent. Alternatively, memoise warned ancestors in a `OnceLock<Mutex<HashSet<u32>>>`. Treat the start-revisit like any other revisit.

#### PAR-D3-2026-10-15-01: `read_reference_frame` hard-codes 64-bit field offsets (`DURATION 0x40`, `SAMPLES 0x48`) while the rest of the v11 clip path derives offsets from the pointer size, so a 32-bit Havok 2014 clip reads the wrong fields silently.
- **Severity**: LOW (no vanilla 32-bit v11 file exists, but `Packfile::parse` accepts the combination)
- **Dimension**: Version Gating
- **Location**:
  - `crates/hkx/src/animation.rs:640-648`
  - Contrast `Layout::new(pack.pointer_size())` at `:399`, and `Packfile::parse` accepting pointer size 4 together with version 11 (`packfile.rs:55-90`)
- **Status**: NEW
- **Description**:
  - On a 32-bit packfile, `hkReferencedObject` is 8 bytes. That moves `m_up`, `m_forward`, `m_duration` and `m_referenceFrameSamples` to 0x10 / 0x20 / 0x30 / 0x34.
  - The reader instead uses the 64-bit 0x40 / 0x48. It reads bytes from inside the forward vector as the duration and the sample count, and may return `Some` with a garbage path.
  - The behavior reader refuses 32-bit (`require_64_bit`). The clip reader does not.
- **Trigger Input**: a 32-bit (`bytes[0x10] == 4`) `hk_2014` clip with a reference frame.
- **Suggested Fix**: derive both offsets from `Fields::referenced_object(pointer)` (frame-type byte, then two 16-aligned vectors, an int and an array), or return `None` with a warning when `pointer_size() != 8`.

#### PAR-D4-2026-10-15-02: the FO3 and FNV menu-XML corpora are never parsed by a gate, although the include budgets are calibrated on them
- **Severity**: LOW
- **Dimension**: Corpus Gates
- **Location**:
  - `crates/menuxml/tests/fo3_corpus.rs:74-90` and `crates/menuxml/tests/fnv_corpus.rs:83-101` list `*.xml` with only a `>= 90` / `>= 110` floor. FO3 parses one document, `hud_main_menu.xml`, through `fo3_hud_renders`. FNV parses none.
  - Budget calibrations that cite these corpora: `crates/menuxml/src/parse.rs:595-607` ("heaviest vanilla document splices 38,378 bytes", "largest vanilla fragment (12,582 B, FNV)") and `:632` ("largest vanilla document has 205 tiles").
- **Status**: NEW. The Oblivion corpus got exact parse pins in ba252c482 (#5329). The baseline gate matrix already noted "tile totals printed, not pinned", and that is now true only for FO3/FNV.
- **Trigger Input**: n/a (gate gap). Regression class: a scanner or include change that drops, or newly budget-truncates, FO3/FNV menu content. Truncation is a `log::warn!`, never an error, so only a pinned total would show it.
- **Description**:
  - The `MAX_INCLUDE_*` / `MAX_DOCUMENT_TILES` constants are justified by a 2026-10-05 three-corpus census. Two of those three corpora have no committed check.
  - A future change could push a vanilla FNV document into a budget, and the strict lane would stay green.
- **Evidence**: probe `menuparse`, real `parse_document` with an archive-backed source.

  | Corpus | XML | Tiles un-spliced | Tiles spliced | Largest document |
  |---|---|---|---|---|
  | FO3 | 99 | 736 | 1,445 | `inventory_menu.xml`, 150 tiles |
  | FNV | 121 | 1,060 | 2,206 | `inventory_menu.xml`, 195 tiles |

  - 0 budget, cycle or depth warnings on either corpus.
  - The XML-count floors are loose: FO3 is `>= 90` against 99 measured, FNV `>= 110` against 121.
- **Impact**: lost regression coverage for the FO3/FNV HUD driver's whole document set. There is no current failure.
- **Related**: #5329, #5314, #4665, PAR-D4-2026-10-12-01.
- **Suggested Fix**: mirror `vanilla_corpus_parses` in both files: parse every XML un-spliced and spliced, and pin the totals above plus the exact XML counts.

#### PAR-D4-2026-10-15-03: the (format × game) sweep matrix still has unswept archive generations; #5073's FO3 placeholder sat outside every committed gate
- **Severity**: LOW
- **Dimension**: Corpus Gates
- **Location**:
  - `crates/bsa/tests/ba2_real.rs:743-775` (`fo3_bsa_corpus_opens_and_lists`, which never extracts).
  - `ba2_real.rs:687-740` (FO76: list everything plus one DX10 extract).
  - `bsa_real.rs:358-416` (SE, `Meshes0` only).
  - `bsa_real.rs:419-462` / `ba2_real.rs:776-815` (the `declared_size` tests, which fully extract FNV Meshes, SE Meshes0, FO4 Meshes and Textures1 only).
  - `crates/facegen/tests/parse_real_facegen.rs` (one file per game).
- **Status**: NEW as a matrix. #5011 closed with "the other (format × game) gaps the issue listed are not addressed", and #4665 added list-only FO76/FO3 coverage.
- **Trigger Input**: n/a.
- **Description**: committed coverage per (format × game):

  | Format | Oblivion | FO3 | FNV | Skyrim LE | Skyrim SE | FO4 | FO76 | Starfield |
  |---|---|---|---|---|---|---|---|---|
  | BSA / BA2 full extract | ✓ 17/17 archives | ✗ list only | Meshes only (via `declared_size`) | ✗ none | Meshes0 only | Meshes + Textures1 (DX10) | ✗ list + 1 extract | ✓ full sweep |
  | BGSM / BGEM | — | — | — | — | — | ✓ rate 1.0 (count unpinned, D4-43) | ✓ rate 1.0 (count unpinned) | — |
  | CDB | — | — | — | — | — | — | — | ✓ exact 97 / 1,438,780; index 500,403; DLC 500,385 |
  | HKX clips | — | — | — | 16-clip LE/SE parity | ✓ exact census | ✗ 1 clip (D4-40) | — | — |
  | HKX graphs | — | — | — | — | 2 graphs | 3 graphs | — | — |
  | FaceGen | 1 EGM | 1 each EGM/EGT/TRI | 1 each | — | — | — | — | — |
  | MenuXml XML | ✓ exact | ✗ (D4-41) | ✗ (D4-41) | — | — | — | — | — |
  | Menu DDS | ✓ exact | ✓ exact | ✓ exact | — | — | — | — | — |
  | CSG | — | — | — | — | — | ✓ exact | — | — |

  The FO3 and FNV empty-placeholder entries behind #5073 were found only by the orchestrator's uncommitted 2026-10-12 sweep. The committed Oblivion sweep caught Oblivion's 35, while FO3's archives are never extracted by any test.
- **Evidence**: probe `fgsweep` shows the FaceGen sweep is cheap and green today, though uncommitted.

  | Game | EGM | EGT | TRI |
  |---|---|---|---|
  | Oblivion | 154 | 20 | 86 |
  | FO3 | 250 | 36 | 49 |
  | FNV | 275 | 36 | 49 |

  0 parse failures in all of them (679 EGM in total), under seconds of runtime.
- **Impact**: reader regressions in the v104 zlib path (FO3/FNV non-mesh and LE), in v105 outside Meshes0, in FO4 DX10 beyond Textures1, in FO76 extract, and on any EGM other than `headhuman` would land green.
- **Related**: #5011, #4665, #5073, #5660.
- **Suggested Fix**:
  - Generalise the Oblivion brute-force test into a per-title loop over every vanilla archive (FO3, FNV, SE, FO4, FO76; LE behind its variable), with exact file counts.
  - Add a FaceGen sweep pinned at the counts above.

#### PAR-D4-2026-10-15-04: the BGSM/BGEM sweeps pin no corpus size, so a listing regression that drops files stays at "100%"
- **Severity**: LOW
- **Dimension**: Corpus Gates
- **Location**: `crates/bgsm/tests/parse_all.rs:364-368` (FO4, `assert!(stats.total > 0)`) and `:466-469` (FO76, the same). The FO76 JSON bucket is counted but not pinned (`:437-440`), although the comment at `:470-472` says "exactly 2 known JSON files".
- **Status**: NEW. `MIN_SUCCESS_RATE` 1.0 is confirmed at `parse_all.rs:52`, and drift is asserted empty. The size half of contract (d) is missing.
- **Trigger Input**: n/a. Regression class: a `Ba2Archive` name-table or `list_files` regression that loses entries (for example, one GNRL page), or a reader change that starts `{`-prefixing binary payloads. The rate is computed over whatever is listed, and JSON-classified files are not failures.
- **Description**:
  - Every other parser census now pins exact counts: HKX, CDB, menu DDS, CSG and SE Meshes0. The two BGSM sweeps alone can pass on one file.
  - Measured figures already sit in the comments: FO4 "0/36,888 files" (`:357`); FO76 "29,989/29,991 OK with exactly 2 known JSON files" (`:470`).
- **Evidence**: code reading. No probe was needed: the assertions are one-sided by inspection.
- **Impact**: a weaker gate than its siblings. No current failure.
- **Related**: #5011, #4665, #5671 (the same one-sided-to-exact fix on HKX).
- **Suggested Fix**: assert the exact per-variant FO4 BGSM/BGEM totals, and assert FO76 `total == 29,991` and `json_text == 2`.

#### PAR-D4-2026-10-15-05: every parser real-data suite's only automated lane has never produced a result, and nothing tracks it
- **Severity**: LOW
- **Dimension**: Corpus Gates
- **Location**: `.github/workflows/real-data-gates.yml`:
  - the `corpus`, `parsers` and `closed-high-pins` jobs all have `runs-on: [self-hosted, linux, x64, byroredux-game-data]`;
  - the `parsers` loop is at `:208`.
- **Status**: NEW as an issue. The fact itself is recorded in the audit-parsers skill text ("runner count 0, 2026-10-15") with the advice to run locally. #4619 (closed) fixed the YAML that produced 0-job runs but not the missing runner. No open issue tracks the runner (search "self-hosted runner real-data-gates").
- **Trigger Input**: n/a.
- **Description**: the lane exists and is correctly shaped, but it is not a gate.
  - The workflow is well formed: `--no-fail-fast`, green-no-op refusal, strict env, and the LE `--skip` is named.
  - Every `--ignored` suite of the six parser crates is scheduled in it. game-detect has none, and its unit tests run in `ci.yml`.
  - The two FO4 HKX tests run there, since `BYROREDUX_FO4_DATA` is exported.
- **Evidence**:
  - `gh api repos/matiaszanolli/ByroRedux/actions/runners --jq .total_count` returns `0`.
  - The last 200 `real-data-gates.yml` runs: 184 `failure` (2026-09-15 → 2026-09-29, the pre-#4619 0-job era), 15 `cancelled` (2026-09-30 → 2026-10-14, queued and never picked up), and 1 `queued` (38711237512, 2026-10-15, at adbd468e2, which predates ceca6eaa8, so its job list has no `closed-HIGH real-data pins` job). **Zero successful runs, ever.**
- **Impact**: the "nightly" column of every parser gate is empty. All Dim 4 evidence comes from local runs, so the exact pins above guard nothing between audits.
- **Related**: #4619, #4659, #5799, #3919.
- **Suggested Fix**: either register a `byroredux-game-data` runner, or open a tracking issue and mark the lane "manual-only" in the workflow header and `docs/contributing.md`, so a cancelled run is not read as a gate.

#### PAR-D4-2026-10-15-06: `docs/feature-matrix.md` and ROADMAP's crate table still say FO4 `.hkx` is unread and that there is no behavior-graph loading
- **Severity**: LOW
- **Dimension**: Corpus Gates (doc claims)
- **Location**:
  - `docs/feature-matrix.md:82-87`: "awaits a Havok `.hkx` loader for FO4's packfile layout … covers Skyrim only".
  - `docs/feature-matrix.md:132-142`: row `~` "Skyrim LE + SE", "no behavior-graph loading or execution … FO4 and Starfield `.hkx` remain unread".
  - `docs/feature-matrix.md:327`, `:341`: the gaps table lists the "Havok `.hkx` loader (FO4 / Starfield layouts)".
  - `docs/feature-matrix.md:98`: "FO4+ still slides".
  - `ROADMAP.md:789`: `byroredux-hkx` described as "(Skyrim LE/SE rigs and clips)".
- **Status**: NEW. The file was last touched at ce130a7cf (2026-10-15), after a7bbba273 / 60082cc55 / 49529db7c (2026-10-14) shipped the FO4 Havok 2014 clip, skeleton and behavior-graph readers. `ROADMAP.md:102` is already correct ("FO4's run through its Havok 2014 behavior graphs").
- **Trigger Input**: n/a.
- **Description**:
  - No BGSM/BA2/CDB/HKX *rate* claim without a date was found. The only numeric rates in `ROADMAP.md:218-220` are NIF-level and dated.
  - The capability claims for HKX, however, contradict the code and `ROADMAP.md:102`.
- **Evidence**: `git log -1 -- docs/feature-matrix.md` returns ce130a7cf, dated 2026-10-15. The `.hkx` rows still predate the FO4 commits.
- **Impact**: doc accuracy; readers of the feature matrix believe FO4 actors cannot animate from `.hkx`.
- **Related**: PAR-D4-2026-10-15-40.
- **Suggested Fix**: update the four feature-matrix spots and the ROADMAP crate row. Cite the FO4 census (D4-40) once it lands.

#### PAR-D5-2026-10-15-03: 26.5% of vanilla FO4 BGSM chains run through the self-referential `defaulttemplate_wet.bgsm` and are never cached, so every resolve re-extracts and re-parses about 3.6 files. Since #5010, every one of those re-parses also re-fires the per-file diagnostics hook
- **Severity**: LOW
- **Dimension**: Decode-Consumer Wiring
- **Location**:
  - `crates/bgsm/src/template.rs:248`: `let key = path.to_ascii_lowercase()`. This is the raw spelling, not normalised.
  - `:273`: `resolver.parsed(&key, &diagnostics)`.
  - `:280`: `let parent_key = pp.to_ascii_lowercase()`, compared against `visited`.
  - `:307`: `if !truncated { self.insert(..) }`. The truncation propagates to every ancestor.
  - Consumer: `byroredux/src/asset_provider/material/provider.rs:395-500` (`resolve_bgsm`). Callers are `merge.rs:1214` (per mesh merge) and `cell_loader/refr.rs:294` (per REFR swap).
- **Status**: NEW. The never-cache rule is #2701 (closed). The re-report is a side effect of 746e0858e (#5010). A search for "defaulttemplate_wet" and "truncated chain cached BGSM" across all issues found nothing.
- **Trigger Input**: vanilla FO4 `materials\template\defaulttemplate_wet.bgsm`, whose `root_material_path` is `template/defaulttemplate_wet.bgsm` (itself). Any mod template chain that ends in a self-reference behaves the same way.
- **Description**:
  - #2701 stopped caching cycle-truncated chains, because where a *multi-node* cycle breaks depends on the entry node.
  - A self-loop breaks at the same place from every entry, but it is treated the same way. The only cycle in vanilla FO4 is that self-loop.
  - So every leaf whose chain reaches it is never cached, and neither is any template above it, because `truncated` propagates upward. Every `resolve_bgsm` repeats the archive extract, inflate and parse of the whole chain.
  - The cycle check compares the raw `root_material_path` spelling against the normalised leaf key the provider passes in, so the self-loop is detected one level late. `defaulttemplate_wet.bgsm` appears twice in a leaf's chain, and three times when it is resolved directly. That is harmless to the merge (`walk()` is first-Some-wins on identical fields) but costs an extra extract and parse each time.
  - #5010's `parsed` hook documents "Cache hits are not re-reported". On these chains nothing is ever a cache hit, so a mod template with drift or lossy strings on such a chain warns again on every mesh merge that touches it, rather than once per file.
- **Evidence**: probe `probe-archives`, real `TemplateCache` plus a resolver that normalises like `ArchiveReader`, run over all 6,616 vanilla FO4 BGSMs:
  ```
  bgsm files 6616 lossy 0 unconsumed 0
  resolve errs 3; leaves re-parsed on a repeat resolve 1753; parsed() calls on repeats 6291
  self-loop anchors 1 (chains 1753), multi-node cycles 0
    1753 materials\template\defaulttemplate_wet.bgsm
  materials\setdressing\greebs\greebs02alpha.bgsm: chain len 4 parsed [leaf, "template/vehiclebustemplate_wet.bgsm",
    "template/defaulttemplate_wet.bgsm", "template/defaulttemplate_wet.bgsm"]   repeat resolve parsed 4 more
  materials\template\defaulttemplate_wet.bgsm: chain len 3 …  repeat resolve parsed 3 more
  ```
  Vanilla has zero diagnostics, so there is no warning flood on vanilla data today. The repeated extract, inflate and parse does happen on vanilla.
- **Impact**:
  - **Performance (vanilla):** 1,753 of 6,616 vanilla materials pay about 3.6 archive extracts plus BGSM parses per `resolve_bgsm`, which runs per mesh merge and per REFR swap on the cell-load path, instead of zero after the first resolve.
  - **Diagnostics (mod content):** the #5010 diagnostic contract degrades into a warning per merge.
  - No wrong bytes result.
- **Related**: #2701, #5010, #1148, #3899, `/audit-performance`, `/audit-fo4`.
- **Suggested Fix**:
  - Run `visited` and the cache key through one canonical form, either a key-normalising hook on `TemplateResolver` or normalisation inside the provider before `resolve`. The self-loop would then be caught at its first occurrence.
  - Treat a self-loop (parent key equals own key) as not truncating, since its cut is entry-independent. Only a multi-node cycle needs #2701's no-cache rule.
  - Pin it with a cache-hit test on a self-referential fixture: the second resolve gives zero `parsed` calls.

#### PAR-D5-2026-10-15-04: #5466's replacement text says the CDB index is "built lazily on the first lookup" and is "not built at discovery", but #5291 prefetches it at discovery
- **Severity**: LOW
- **Dimension**: Decode-Consumer Wiring
- **Location**:
  - `byroredux/src/asset_provider/material/cdb.rs:324`. This is the INFO log printed for every discovered CDB: "(the material index is built lazily on the first lookup)".
  - `cdb.rs:9`: module doc "the lazily built `MaterialIndex` (see `provider.rs`)". The index is built in `cdb.rs`.
  - `provider.rs:213-216`: "built lazily on the first `.mat` lookup … not at discovery — a session that loads no Starfield content never pays the ~2 s / ~470 MB build".
  - Contradicted by `cdb.rs:288-291`, where `discover_starfield_cdbs` calls `prefetch_cdb_material_index` for every valid CDB, and by `cdb.rs:86-90` ("~247 MB … the ~470 MB 224a19372 recorded is stale").
- **Status**: NEW. 4b289dce1 (#5466, closed) rewrote this text after dd759b762 (#5291) had already made the build eager. The new log line is stale on arrival, which is the same "INFO log printed every session" defect #5466 was filed for.
- **Trigger Input**: any session that discovers a valid `materialsbeta.cdb`.
- **Description**: the operator-facing log and two doc sites describe the pre-#5291 lazy model. A Starfield session pays the build at discovery on a worker thread whether or not any `.mat` merge happens. The peak figure quoted in `provider.rs` is the stale ~470 MB rather than the re-measured ~247 MB.
- **Evidence**: `cdb.rs:203-224` spawns `sf-cdb-index` from discovery, and `cdb.rs:131-136` itself says the build is paid "by the background prefetch … when it gets there first".
- **Impact**: doc and log accuracy only. Someone tracing a memory or latency question is told the build cannot happen in a session without `.mat` lookups.
- **Related**: #5466, #5291, #5666 (the prefetch ordering residual, still open).
- **Suggested Fix**: say "built in the background at discovery (#5291); a lookup that arrives first builds it (or parks on the build)". Point the module doc at `cdb.rs`, and replace ~470 MB with the ~247 MB peak / ~115 MB resident figures.

#### PAR-D6-2026-10-15-01: 0f07131a8 (#5739) spliced the new helper's doc into `resolve_profile_root`'s doc comment; `resolve_profile_root` is now undocumented and `absolutise_against_cwd` documents the wrong function
- **Severity**: LOW
- **Dimension**: I/O & Paths (doc integrity of the moved `--profiles` reader)
- **Location**: `crates/game-detect/src/profiles.rs:52-63`.
  - Lines 52-55 are `resolve_profile_root`'s original doc ("Compose a profile's data directory from `games_root` + … Phase 20.").
  - Lines 56-63 are the #5739 text.
  - Both precede `pub fn absolutise_against_cwd` (`:64`).
  - `pub fn resolve_profile_root` (`:91`) now has no doc at all.
- **Status**: NEW. It is the same defect class as #5029 (fixed for menuxml in 511f8f4b1) and open #5704.
- **Trigger Input**: n/a.
- **Description**: rustdoc for `absolutise_against_cwd` opens with "Compose a profile's data directory from `games_root` + the profile's `subdir` field…", which describes `resolve_profile_root`.
- **Evidence**: `git show 0f07131a8 -- crates/game-detect/src/profiles.rs`. The hunk at `@@ -53,6 +53,41 @@` inserts below line 55, inside the existing `///` block.
- **Impact**: doc rot on a public API used by two tools.
- **Related**: #5739, #5029, #5704.
- **Suggested Fix**: move lines 52-55 back above `resolve_profile_root` (`:91`).

#### PAR-D6-2026-10-15-02: `load_with_user_path(Some(X))` merges profiles from X but takes `[roots]` from `$BYRO_PROFILES` or the home file
- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Location**: `crates/game-detect/src/profiles.rs:336-341`. `if let Some(user_path) = selected_user_path()` ignores the injected `user_path` that `:324-328` honours. Callers: `tools/byro-launcher/src/state.rs:81`, `:97` and `tools/byro-detect/src/main.rs:57`.
- **Status**: NEW (pre-existing; predates the delta, but sits beside the code #5739 moved). #5476 (closed) fixed `[defaults]` and the profile merge, not this overlay.
- **Trigger Input**: `byro-launcher --profiles X` (or `byro-detect --profiles X`) on a machine whose `~/.byroredux/profiles.toml` has different `[roots]` or is malformed. In the malformed case the launcher prints `[roots] could not read ~/.byroredux/profiles.toml: …` for a file it was not pointed at.
- **Description**:
  - `load_with_user_path`'s contract (`:290-294`, #5476) is "the registry they validate against comes from the SAME file they were pointed at". The `[roots]` overlay breaks it.
  - The spawned engine, which gets `BYRO_PROFILES=X`, overlays X's `[roots]`, so the two registries disagree on `entry.root`.
- **Evidence**: code reading. I confirmed no tool currently reads `entry.root` (`grep '\.root\b' tools/byro-{launcher,detect}/src`: no hits). Candidates take `data_dir` from `detect_all(profiles_path)`, which reads X directly. **The defect is latent:** the only live symptom is the stray read and error line.
- **Impact**: latent divergence plus a misleading error line. It would become a wrong-root validation the moment a tool consumes `entry.root`.
- **Related**: #5476, #5294, #5739.
- **Suggested Fix**: overlay `[roots]` from `user_path` when it is `Some`, falling back to `selected_user_path()` only for `load_default`. Pin this with a test like `launch_defaults_follow_the_selected_user_file`.

#### PAR-D6-2026-10-15-03: a non-UTF-8 `--profiles` path panics both tools instead of giving a usage error; the shared helper hard-codes `String`
- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Location**:
  - `crates/game-detect/src/profiles.rs:81-89`: `parse_profiles_arg(args: &mut impl Iterator<Item = &'a String>)`.
  - `tools/byro-detect/src/main.rs:26` and `tools/byro-launcher/src/main.rs:27`: `std::env::args()`.
- **Status**: NEW. #5739 centralised the parse but kept the `String` interface.
- **Trigger Input**: `byro-detect --profiles $'/tmp/\xff/profiles.toml'`, or any argv element that is not valid UTF-8 on Linux.
- **Description**:
  - `std::env::args()` panics during iteration on non-Unicode arguments, before `parse_args` runs. A legal Linux path aborts the tool with a panic backtrace rather than "--profiles needs a path"-style usage handling.
  - Because the shared helper takes `&String`, a fix in either tool needs an API change in game-detect.
  - Minor related edge: `--profiles ""` passes the `starts_with('-')` filter and absolutises to the cwd *directory* (`:66-71`). The loader then warns "failed to read" on an `exists()` directory (`:325-326`, `:357-362`) instead of rejecting the value as a usage error.
- **Evidence**: code reading. The panic is documented `std::env::args` behaviour ("will panic during iteration if any argument to the process is not valid Unicode").
- **Impact**: robustness of a user-facing CLI. No data hazard, since nothing is written before the panic.
- **Related**: #5739, #5167, #5476.
- **Suggested Fix**: collect `std::env::args_os()`, make `parse_profiles_arg` take `&OsString` and return `PathBuf`, and reject an empty value as a usage error.


## Gate Matrix

| Crate / format | Size caps | Error policy | Version gate | Corpus sweep | CI lane |
|---|---|---|---|---|---|
| bsa BSA | ✓ single ceilings (`safety.rs`); file-relative `capacity_hint` | ✓ empty record decided before the decoder (#5660) | ✓ 103/104/105; v105 LZ4 frame | Oblivion brute force; FNV and SE Meshes. ✗ FO3, LE full extract (D4-03) | nightly `--no-fail-fast`; never ran (D4-05) |
| bsa BA2 | ✓; `declared_size` used only as a budget | ✓ | ✓ 1/2/3/7/8 | FO4 Meshes + Textures1; Starfield full. ✗ FO76 DX10 extract (D4-03) | same |
| bsa CSG / UVD | ✓ (`read_psg` guard live) | ✓ | magic | CSG exact (32,370 / 6,841) | same |
| asset_provider archive registry | 1024 cap | ✗ #5663, #5664 | n/a | n/a | bin unit |
| bgsm | ✓ depth 16 + visited | ✓ diag everywhere. ✗ re-reported on uncached self-loop chains (D5-03) | per-field, ceiling 22 | rate 1.0, drift-gated. ✗ sizes unpinned (D4-04) | nightly |
| sfmaterial | ✓ nesting 64; streaming index | ✗ PersistentID drift silent (D2-04); walk warnings unmemoised (D2-05); #5665/#5666 | header + pinned vocabularies | ✓ exact (97 / 1,438,780; 500,403 keys) | nightly |
| hkx clips (v8) | ✓ 1M samples, 256 frames/block | ✓ #5662/#5691 track-drop | ✓ fileVersion 8 + contents; padded `mask_size` | ✓ SE exact (6,126, 0 rejects); LE parity | nightly; #5446 open |
| hkx v11 / FO4 + behavior graphs | ✓ table caps all present. ✗ no visit budget (D1-01); no depth budget across inlines (D1-02) | ✗ D2-01 runtime loop; D2-02 ref-frame; D2-03 dev panic; D5-02 consumer logs at debug level | ✓ v11 + contents; predicate area bounded. ✗ v11 × 32-bit ref frame (D3-01) | ✗ none (D4-01); 856 lossless unread (D5-01) | ✗ only spot tests |
| facegen | ✓ morph-count cap; exact size | ✓ | ✓ magics | Oblivion EGM exact. ✗ one file per game (D4-03) | nightly |
| menuxml parse | ✓ depth 48/64, include +1, 256 splices, 256 KiB, 64 KiB fragment, 512 KiB fetched (1 KiB floor), 16,384 tiles, resolved-key cycle check | ✓ once-per-document budget warnings | n/a | Oblivion exact (1,868 / 2,773). ✗ FO3/FNV unparsed (D4-02) | `ci.yml` unit; nightly never ran |
| menuxml tex | ✓ ≤8192 dimensions; uncompressed sized first; 1 GiB budget; refusals memoised. ✗ BC allocates first (#5667) | ✓ undecodable vs missing; substitution warned | DXT1/3/5, masked 8/16/24/32 | ✓ exact for 3 games | same |
| game-detect | ✓ VDF depth 32 | ✓ malformed VDF rejected. ✗ argv panic (D6-03); `[roots]` from the wrong file (D6-02) | n/a | unit only, by design | ✓ `ci.yml` |

## Already tracked: re-verified, not re-filed

| Issue | State at HEAD |
|---|---|
| #5446 | OPEN: three of four HKX dimension-gate fixtures leave `mask_size` 0 |
| #5663 / #5664 | OPEN: `asset_provider/archive.rs` untouched since 5402b2530 |
| #5665 / #5666 | OPEN: `material/cdb.rs:137-141, :214-219` unchanged (no `catch_unwind`; gate taken before the slot) |
| #5667 | OPEN: `tex.rs:207/222/249` allocate before the payload check |
| #5009 | CLOSED 2026-10-15 (`44219e881`) after this audit gathered its data: the `debug_load.rs` raw extract loop is gone |
| #5861 | OPEN (gameplay): `resolve_wanted` restarts every graph sharing a loaded graph. Related to D5-02 |
| #5502 / #5509 / #5525 | OPEN: CDB Parent-inherited texture slots, additive blend modes, `parse_loose_mat`. No parked CDB field leaks into a live slot |

## Cross-audit routing

- **`/audit-scripting` Dim 5** (HKX playback): D2-01 lives in `crates/scripting/src/animation_graph.rs`. It is filed here because the hang is file-driven, but the fix is in the runtime.
- **`/audit-performance`**: D5-03 (about 6,291 redundant BGSM parses per full resolve pass) and the per-frame `Inventory` clone noted by the gameplay audit.
- **`/audit-tooling`**: D6-01..03 (`--profiles`).

## Summary

| Severity | NEW | Existing |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 2 | 0 |
| MEDIUM | 6 | 1 |
| LOW | 13 | 5 |

Suggested next step: `/audit-publish docs/audits/AUDIT_PARSERS_2026-10-15.md`. Suggested labels:

| Findings | Labels |
|---|---|
| HKX (D1-01, D1-02, D2-01, D2-02, D2-03, D3-01, D4-01, D5-01, D5-02) | `animation` + `game:fo4` |
| CDB (D2-04, D2-05, D5-04) | `nifal` + `game:starfield` |
| BGSM (D5-03) | `nifal` + `game:fo4` |
| D4-02 | `ui` + `test-gap` |
| D4-03 / D4-04 / D4-05 | `test-gap` + `import-pipeline` |
| D4-06 | `documentation` + `doc-rot` |
| D6-01..03 | `tech-debt` |
