# Parser Discipline Audit: 2026-10-21

**HEAD**: 4a3e89d56 · **Baseline**: `AUDIT_PARSERS_2026-10-15.md` (HEAD 5f7ff5920) · **Audited**: Dims 1, 2, 5 and 6 in depth; Dims 3 and 4 on their changed files only · **Unchanged since baseline (skimmed)**: menuxml `parse.rs` / `font.rs`, game-detect `vdf.rs` / `steam.rs` / `catalog.rs`, facegen `src/`, bsa `src/`.

Commits since 5f7ff5920 on the Paths of this skill:

| Area | Commits | What landed |
|---|---|---|
| hkx | 8 | Fixes for every hkx finding of the baseline (#5891 step budget and ancestor set, #5893 inline nesting, #5894 reference-frame refusals, #5895, #5897 the new `hkaLosslessCompressedAnimation` decoder, #5896 FO4 census and v11 fixtures, #5901) |
| menuxml | 3 | BC payload check before allocation (#5667), FO3/FNV corpus pins (#5902), and BC4 / BC5 / DX10 decoding for terrain height maps (5017a64c2) |
| sfmaterial | 4 | #5899, #5900, #5525 (shared settings capture), #5502 Parent-chain texture-slot inheritance (1d7e2c6bd) |
| bgsm | 2 | #5907 canonicalised self-reference cache, #5904 corpus sizes |
| bsa / facegen | 3 | sweep tests (#5903), the `.fuz` census example (#5912) |
| game-detect | 4 | #5909, #5910, #5911, and a clippy cleanup |
| asset_provider | 27 | **new loose `Data\` tier** (48e422a71, #6200), MSWP / TXST-through-BGSM wiring, graph loading fixes (#5898, #6005, #6126) |
| new reader in scope | 1 | `crates/audio/src/fuz.rs` (FUZE / xWMA container, acd577a79, #5912). Owned jointly with `/audit-audio` per `_audit-owners.md`. |

All 21 findings of the baseline were published as #5891–#5911 and are closed. The fixes were re-read at HEAD and hold. Two of them have residuals, filed below as PAR-D1-01 and PAR-D1-02.

## Method and evidence

- This was a solo, sequential run. No engine was launched. Nothing in the tree was changed apart from this report.
- **Probe**: one throwaway crate at `/mnt/data/tmp/suite-1021-parsers/hkx`. It is a verbatim copy of `crates/hkx` plus a `src/probe.rs` test module, built with its own target dir under the same root. It builds packfiles with the crate's own `PackfileBuilder` and drives the real `read_generator` / `Walk` / `BehaviorGraph::inline_references`. Its numbers back PAR-D1-01 and PAR-D1-02.
- **Default lane** (`cargo test -p byroredux-{bsa,bgsm,sfmaterial,hkx,facegen,menuxml,game-detect}`): all green.

| Crate | Unit | Other |
|---|---|---|
| bsa | 114 | + 1 `ba2_real` |
| bgsm | 35 | + 1 `parse_all` |
| sfmaterial | 51 | + 6 `header_smoke` |
| hkx | 63 | |
| facegen | 31 | |
| menuxml | 62 | |
| game-detect | 60 | |
| doc-tests | 6 | |

**Real-data suites, strict lane** (`BYROREDUX_REQUIRE_GAME_DATA=1 cargo test --release -p <crate> -- --ignored`, one crate at a time): all green, nothing skipped, all data present.

| Crate | Passed | Breakdown |
|---|---|---|
| bsa | 34 | 11 unit, 12 `ba2_real` (463 s), 10 `bsa_real` (79 s), 1 `csg_real` |
| bgsm | 3 | `parse_all`, with pinned corpus sizes (#5904) |
| sfmaterial | 5 | `real_cdb` |
| hkx | 10 | Includes the FO4 clip and behavior censuses (#5896) and the Skyrim LE parity test (WINE-prefix fallback). The FO4 census asserts every one of 14,370 clips decodes, 856 of them lossless. |
| facegen | 5 | |
| menuxml | 11 | 4 Oblivion, 4 FO3, 3 FNV (#5902) |

## Executive Summary

| Severity | NEW | Existing (open) re-confirmed |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 2 | 0 |
| MEDIUM | 2 | 0 |
| LOW | 3 | 4 (#5446, #5663, #5664, #5665) |

- **HIGH**: both findings are main-thread hangs or out-of-memory failures that a mod's Fallout 4 behavior graph can cause. They are residuals of #5891 and #5893.
  - **PAR-D1-01**: `MAX_GRAPH_STEPS` counts objects and child-array entries, but not the leaf tables read inside each object (clip triggers, variable bindings, bone weights, transitions). The work per step is therefore not bounded.
    - Measured with a 146 KB file: 984,001 steps, which is under the cap, produce 491,519 clip copies.
    - 128 triggers on each clip took 1.76 GiB; the cap allows 1,024, which extrapolates to about 12 GiB.
    - 256 bindings on each clip took 8.4 s; the cap allows 4,096, which extrapolates to about 135 s.
  - **PAR-D1-02**: `inline_references` resolves up to 64 sub-behaviors, and each is a full clone of a decoded graph. One self-referencing graph therefore costs up to 65 times the per-decode budget.
    - Measured: a graph at 10% of the step budget (30 MiB) reached a 1.99 GiB peak in 9.7 s, and 32 nested copies stay in the cache.
    - At the budget this extrapolates to about 20 GB.
- **MEDIUM**:
  - **PAR-D5-01**: the new lossless clip decoder passes raw quaternions to the pose with no normalisation and no zero-length check. The spline path normalises them and refuses a zero-length one.
  - **PAR-D6-01**: the new loose-`Data\` indexer recurses through symlinked directories with no depth cap and no visited set. A symlink back to an ancestor directory makes the startup walk exponential.
- **LOW**:
  - **PAR-D1-03**: the lossless frame count is not tied to the file's size, and a zero duration is accepted.
  - **PAR-D6-02**: the loose tier folds keys with an ASCII-only lowercase while the archives use a Unicode fold, and it overwrites case-variant duplicates silently.
  - **PAR-D2-01**: `.fuz` decoding runs `ffmpeg` synchronously, with no timeout and no output cap, while holding the voice-cache write lock, and writes through a non-exclusive temp file.
- **Clean this window**:
  - The BC4 / BC5 / DX10 additions to the menuxml DDS decoder all pass `bc_output`'s payload check before allocating. `decoded_len` stays correct for every new format.
  - The CDB Parent-chain slot inheritance uses the bounded, cycle-safe `ancestry` iterator.
  - BGSM canonicalisation is consistent between `ArchiveReader::canonicalize`, `resolve_bgsm` and `peek_magic`.
  - The 32-bit Havok 2014 reference-frame offsets are fixed (#5901).
  - The `--profiles` fixes hold.

## Findings

### HIGH

#### PAR-D1-01: `MAX_GRAPH_STEPS` does not charge the leaf tables read inside each object, so the work and memory per step are unbounded. A ~150–300 KB mod behavior graph inside the step budget can take about 12 GiB or minutes of main-thread time.
- **Severity**: HIGH (an out-of-memory failure or hang reachable from a mod file)
- **Dimension**: Size Discipline
- **Location**:
  - `crates/hkx/src/behavior.rs`:
    - 282 (`MAX_GRAPH_STEPS`)
    - 311–344 (`Walk`)
    - 1121–1152 (`read_clip`)
    - 1066–1079 (`bound_variable`)
    - 1163–1184 (`read_bone_weights`)
    - 1094 (transitions)
    - 1370 / 1395 / 1415 (modifier tables)
  - Caller: `byroredux/src/asset_provider/actor_graph.rs:92-104`.
  - Main thread: `byroredux/src/app_step.rs:1096/1108`.
- **Status**: NEW. This is a residual of #5891 (closed).
- **Trigger Input**:
  - A behavior graph whose blender fan-out re-enters one shared `hkbClipGenerator`. This is the shape #5891 bounded by steps.
  - The shared clip carries `MAX_TRIGGERS` (1,024) triggers and a binding set of `MAX_VARIABLES` (4,096) entries whose `memberPath` is not `playbackSpeed`.
  - Both tables live once in the file (32 KB and 160 KB) and are reached through one pointer each.
- **Description**:
  - e730f9a77 charges one step for each object `Walk::enter`s and one for each child-array entry (`walk.charge(len)` at 1279 / 1304 / 1451 / 1496).
  - Every leaf table read inside an entered object is free:
    - `read_clip` builds a `Vec<ClipTrigger>` of up to 1,024 entries, and `read_generator_node` clones it a second time into `content.blended_clips` (1222).
    - `bound_variable` walks up to 4,096 bindings and allocates an owned `String` (up to 256 B) for each one's member path. It is called for every clip, blender, blend child and transition effect.
    - `read_bone_weights` reads 1,024 weights and 4,096 bindings.
    - The tables of transitions (4,096, each with a condition parse), event ranges, expressions and assign-variable bindings are not charged either.
  - The doc on `MAX_GRAPH_STEPS` ("about 230 ns a step … a graph at the cap costs about a quarter of a second") holds for vanilla files only. A step's real cost scales with these tables by up to about 4,096×.
- **Evidence**: probe `probe_uncharged_leaf_tables`. Blender of 480 children → blender of 1,024 children → one shared clip; release build; the `read_generator` the decoder uses.

  | Triggers / bindings per clip | File | Steps | Clip copies | Peak RSS Δ | Time |
  |---|---|---|---|---|---|
  | 0 / 0 | 146 KB | 984,001 (< 1 M) | 491,519 | 305 MiB | 0.28 s |
  | 64 / 0 | 148 KB | 984,001 | 491,519 | 1,040 MiB | 0.94 s |
  | 128 / 0 | 150 KB | 984,001 | 491,519 | 1,760 MiB | 1.49 s |
  | 0 / 64 | 149 KB | 984,001 | 491,519 | 305 MiB | 2.23 s |
  | 0 / 256 | 158 KB | 984,001 | 491,519 | 305 MiB | 8.45 s |

  Both columns are linear. At the caps this extrapolates to about 12 GiB with 1,024 triggers, and about 135 s with 4,096 bindings, from a file of about 190 KB and about 300 KB respectively. Both are inside `MAX_GRAPH_STEPS`, so the #5891 guard never fires. The probe uses the Havok 2010 offsets; FO4 files go through the same reader functions with the 2014 table.
- **Impact**:
  - An FO4 mod actor (or placed object) whose behavior graph has this shape freezes the main thread or exhausts memory when the actor's graph is built (`populate_actor_graphs`, every tick that has pending actors).
  - `ActorGraphCache` then keeps the result and clones it (PAR-D1-02).
  - Vanilla is unaffected: the census maximum is 115,605 steps on `gunbehavior.hkx`, with small tables.
- **Related**:
  - #5891 / e730f9a77: the step budget.
  - #5893: inline nesting.
  - PAR-D1-02.
  - The census `fallout_4_behavior_graph_census_is_exact` pins steps, not table volume.
- **Suggested Fix**:
  - Thread `walk` into `read_clip`, `bound_variable`, `read_bone_weights`, the transition reader and the modifier table readers, and `walk.charge(len)` for every table they read, or charge bytes rather than entries.
  - Re-measure the shipped maximum with the census and keep the 8× headroom assertion.
  - Add a fixture: a shared clip with 1,024 triggers under a fan-out must fail with the budget error.

#### PAR-D1-02: `inline_references` multiplies the per-decode budget by up to 65: each of its 64 resolves is a full clone of a cached decoded graph, all of them live at once on the recursion. A 10%-of-budget self-referencing graph peaks at 2 GiB and 9.7 s, and 32 copies stay cached.
- **Severity**: HIGH (an out-of-memory failure or hang reachable from a mod file)
- **Dimension**: Size Discipline
- **Location**:
  - `crates/hkx/src/behavior.rs:933-993` (`inline_references`, `inline_machine`, `inline_references_with`; the height check is at 966).
  - `byroredux/src/asset_provider/actor_graph.rs:92-115` (`decode_behavior` returns `.clone()` of the cached graph on every resolve; `build` inlines on the main thread).
- **Status**: NEW. This is a residual of #5893 and #5891 (both closed).
- **Trigger Input**: an FO4 behavior graph with a state whose generator is an `hkbBehaviorReferenceGenerator` naming its own path (or two graphs naming each other), plus a body of N steps that is still under `MAX_GRAPH_STEPS`.
- **Description**:
  - `MAX_GRAPH_STEPS` bounds one decode. `decode_behavior` memoises that decode and hands out a deep clone on every call. The generator tree is `Box`/`Vec`-owned, and `machine_modifier_id_bound` clones each state's generator again.
  - `inline_machine` resolves a reference, then recurses into the clone's own references (`inner.inline_references_with(resolve, budget)`, 961). It applies the `MAX_MACHINE_NESTING` height check only after that recursion returns (966).
  - A self-reference therefore stacks up to `MAX_INLINED_REFERENCES` (64) live clones before any is refused. The ones that fit the height limit (32 here) are kept in the final graph, which `ActorGraphCache` retains for the process.
  - Nothing bounds the total size of the inlined result. 3ce34ed28 bounded the nesting depth and hashed the name merge, but not the volume.
- **Evidence**: probe `probe_inline_clone_amplification`, built from `blender 48 → 1,024 → clip`, one state that refers to itself, and a resolver that clones, as `decode_behavior` does:

  ```
  PROBE2 file=104683B steps=98401 one_copy=30MiB resolves=64 machines_retained=32 peak=1990MiB elapsed=9.66s
  ```

  Scaled to a graph near the step cap (305 MiB for each copy, from the PAR-D1-01 baseline row), this is about 20 GB at peak and about 100 s. PAR-D1-01's table amplification multiplies it further.
- **Impact**:
  - A mod graph that names itself freezes or kills the process the first time an actor using it spawns.
  - Separately, `*ids += inner_bound` (977) and `*id += offset` (823) add file-offset-derived modifier ids across up to 64 inlines in `u32`. A graph file of 64 MiB or more overflows them: a panic in dev builds, and silently colliding modifier runtime state in release.
- **Related**: #5893 (closed), #5891 (closed), PAR-D1-01. #5861 / #6005 graph restart scope (gameplay).
- **Suggested Fix**:
  - Give `inline_references` one shared work budget: charge each resolved graph's step count, or a node count, against a total such as `MAX_GRAPH_STEPS`.
  - Run the height check before recursing into a resolved graph. Skip a reference whose path is already on the inline stack, since a self-reference can never fit.
  - Use `checked_add` on the modifier-id offsets.

### MEDIUM

#### PAR-D5-01: The lossless clip decoder hands raw quaternions to the pose: a zero-length or non-unit rotation reaches `Quat::from_array` and the sampler's un-normalised `slerp`, where the spline path rejects or normalises it
- **Severity**: MEDIUM (silently wrong output from a mod file)
- **Dimension**: Decode-Consumer Wiring
- **Location**:
  - `crates/hkx/src/animation.rs:903-916` (`decode_lossless_in`, `rotation_at`).
  - Spline path: `normalize_quaternion` at 1637–1661.
  - Consumer: `byroredux/src/asset_provider/animation.rs:585-602`.
  - Sampler: `crates/core/src/animation/interpolation.rs:260` (`q0.slerp(q1, t)`, no normalise).
- **Status**: NEW (in code added by 2649febc7, #5897)
- **Trigger Input**: an `hkaLosslessCompressedAnimation` whose static or dynamic rotation array stores `(0,0,0,0)`, or any finite non-unit quaternion such as `(0,0,0,2)`.
- **Description**:
  - `decode_lossless_in` reads four `f32`s per rotation and only flags non-finite values (912). The spline decoder runs every quaternion through `normalize_quaternion`, which:
    - errors on zero length and on length-squared overflow (#4396);
    - normalises everything else.
  - The two decoders of the same `HkxAnimation` therefore apply different policies. A lossless clip's zero quaternion becomes a zero rotation matrix (the bone and its skinned vertices collapse to a point). A length-2 quaternion scales the bone by 4.
  - Nothing warns about either.
- **Evidence**: 912 checks `is_finite` only. `convert_hkx_clip` does `Quat::from_array(rotation)` with no normalise, and the linear rotation sampler `slerp`s the raw keys.
- **Impact**: modded FO4 lossless clips (first-person poses, furniture) render with collapsed or scaled bones and no diagnostic. Vanilla is unaffected: the #5897 census found all 827,268 stored quaternions unit length.
- **Related**: #5897, #4396, #5662 (the non-finite track-drop policy this decoder does share).
- **Suggested Fix**: run each lossless rotation through `normalize_quaternion`. Treat its `Err` like a non-finite value, so the track is dropped into `dropped_bones`. Add a fixture with a zero quaternion.

#### PAR-D6-01: The loose `Data\` indexer follows symlinked directories with no depth cap or visited set. A symlink back to an ancestor directory makes the startup walk exponential and hangs boot.
- **Severity**: MEDIUM. The input is an unbounded loop from the Data directory layout rather than from a file's bytes; it needs a symlink, which mod deployment tools can create.
- **Dimension**: I/O & Paths
- **Location**: `byroredux/src/asset_provider/archive.rs:67-97` (`LooseDir::index`, `LooseDir::walk`). Reached from `build_texture_provider` / `build_material_provider` via `Archive::open_loose` (245) at boot.
- **Status**: NEW (in code added by 48e422a71, #6200)
- **Trigger Input**: `Data\textures\a -> ..` and `Data\textures\b -> ..`, two directory symlinks to an ancestor (or one per level of a deeper tree).
- **Description**:
  - `walk` decides "directory" with `path.is_dir()` and "file" with `path.is_file()`, and both follow symlinks.
  - It recurses with no depth limit and no set of visited `(dev, inode)`.
  - On Linux a symlinked-cycle path stops resolving only at `ELOOP`, after 40 symlinks in one path, or at `ENAMETOOLONG`. One looping link therefore re-indexes the subtree about 40 times under ever-longer keys. Two links make that about 2^40 `read_dir` calls.
  - The walk runs once per process, synchronously, before the first cell loads. The registry's caching does not help with the first walk.
  - Windows junctions behave the same way.
- **Evidence**: the code at 81–97 has no guard. None of `asset_provider/tests/loose_override.rs`'s four tests uses a symlink.
- **Impact**:
  - A user whose mod manager deploys by symlink and leaves a looped link hangs at boot.
  - A single loop bloats the index with duplicate keys, all of which then also go through `count_shadowed_entries`.
- **Related**: #6200. The archive readers' bounded table walks (Dim 1).
- **Suggested Fix**:
  - Use `entry.file_type()`, which does not follow symlinks. Either skip symlinked directories or keep a visited set of canonical directory paths.
  - Cap the depth: archive paths in vanilla data are shallow, so 32 levels is generous.
  - Add a symlink-loop fixture under `#[cfg(unix)]`.

### LOW

#### PAR-D1-03: The lossless decoder does not tie `frame_count` to the file's bytes when every channel is absent or static, and it accepts a zero duration over many frames. The spline path refuses both.
- **Severity**: LOW. The absolute `MAX_TRANSFORM_SAMPLES` cap bounds it.
- **Dimension**: Size Discipline
- **Location**: `crates/hkx/src/animation.rs:822-865`, `891`, `942`
- **Status**: NEW (in code added by 2649febc7)
- **Trigger Input**: a lossless clip with `transform_count = 4,096`, every descriptor 0 (absent), and `m_numFrames = 244`. The file is about 41 KB: the descriptor tables only.
- **Description**:
  - Dynamic series must fill `series × frame_count` entries, which ties output to the file. Absent and static channels cost nothing per frame.
  - A file of a few tens of KB can therefore claim up to `MAX_TRANSFORM_SAMPLES` (1 M) samples. That is about 40 MB of `HkxTransform`, and about 120 MB of key structs retained by `convert_hkx_clip`.
  - The spline path closed exactly this gap with #4655 and #5006 (frames ≤ blocks × (frames per block − 1) + 1).
  - Separately, `duration = 0` with `frame_count > 1` is accepted (`frame_duration` 0, every key at t = 0). The spline path rejects `frame_duration <= 0`.
- **Suggested Fix**:
  - Bound `frame_count` by the dynamic data present. For example, with no dynamic series, require `frame_count` ≤ a small constant, or charge the absolute cap against `transform_count`.
  - Reject `duration <= 0` when `frame_count > 1`.

#### PAR-D6-02: The loose tier folds keys ASCII-only while the archive readers fold with Unicode, and case-variant duplicates on a case-sensitive filesystem silently last-win in `read_dir` order
- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Location**:
  - `byroredux/src/asset_provider/archive.rs:81-103` (`walk`, `key`: `to_ascii_lowercase`).
  - The archive folds: `crates/bsa/src/archive/mod.rs:124` and `crates/bsa/src/archive/open.rs:279` (`to_lowercase`).
- **Status**: NEW (48e422a71)
- **Trigger Input**: a loose override named with a non-ASCII uppercase letter (for example `textures\Ölfass.dds`) referenced in lowercase, or `Data/textures/x.dds` beside `Data/Textures/x.dds` on Linux.
- **Description**:
  - Dim 6's contract is that open-time and lookup keys agree across tiers. The archives fold Unicode, so `Ö` becomes `ö`. The loose index and its lookup fold ASCII only, so a non-ASCII loose override is never found where the archive copy is.
  - Two loose files that normalise to the same key overwrite each other in `files.insert` in nondeterministic `read_dir` order, and nothing is counted or warned. The BSA and BA2 readers count and warn on in-archive duplicates (#4671).
- **Suggested Fix**:
  - Fold with the same `to_lowercase().replace('/', "\\")` the archive readers use, ideally through one shared helper.
  - Count and warn once for duplicate loose keys.

#### PAR-D2-01: `.fuz` voice decoding shells out to `ffmpeg` synchronously, with no timeout and no output cap, under the `VoiceSoundCache` write guard. Its temp file is non-exclusive, and a failed temp write is reported as "ffmpeg missing".
- **Severity**: LOW. `/audit-audio` co-owns this; the decode needs a host `ffmpeg`.
- **Dimension**: Error Semantics
- **Location**:
  - `crates/audio/src/fuz.rs:207-260` (`fuz_to_wav_bytes`).
  - Caller: `byroredux/src/systems/dialogue_voice.rs:264-296` (inside `cache.get_or_load`).
- **Status**: NEW (acd577a79, #5912)
- **Trigger Input**: any `.fuz` that makes `ffmpeg` slow or produce very large PCM (a long or crafted `data` chunk), or a temp-dir write failure.
- **Description**:
  - `Command::output()` blocks until `ffmpeg` exits and buffers all of stdout. That buffer is copied again into the WAV (`wav_header`), and kira then decodes it to `f32` frames.
  - `VoiceSoundCache`'s 64 MiB budget only evicts other entries after the insert, so one line has no ceiling.
  - All of this runs inside the loader closure, which holds the cache write guard on purpose for #5383's lock order.
  - The temp path is `temp_dir()/byro-fuz-<nanos>.xwma`, written with `fs::write`, which follows symlinks and truncates; it is not `create_new`.
  - A write failure maps to `FfmpegMissing`, which is logged at debug level as "no host ffmpeg", so a full or read-only temp dir is misreported.
  - The parsers themselves are sound: `parse_fuz` uses checked `lip_end`, and the chunk walk ends on an overflowing size.
- **Suggested Fix**:
  - Cap the stdout read, for example at 64 MiB of PCM through a bounded reader on a spawned child, and add a wall-clock timeout that kills the child.
  - Create the temp file with `create_new` and a random suffix, or pipe the stream on stdin.
  - Give the temp write its own error variant.

## Gate Matrix

| Crate / format | Size caps | Error policy | Version gate | Corpus sweep | CI lane |
|---|---|---|---|---|---|
| bsa BSA | ✓ single ceilings (`safety.rs`); file-relative `capacity_hint` | ✓ empty record decided before the decoder | ✓ 103/104/105; v105 LZ4 frame | ✓ FO3/FNV/SE census (#5903); Oblivion brute force; LE optional | manual only (`workflow_dispatch`, runner unregistered) |
| bsa BA2 | ✓; `declared_size` used only as a budget | ✓ | ✓ 1/2/3/7/8 | ✓ FO4 full extract, FO76, Starfield | same |
| bsa CSG / UVD | ✓ | ✓ | magic | CSG exact | same |
| asset_provider archive registry | 1024 cap | ✗ #5663, #5664 open | n/a | n/a | bin unit |
| asset_provider loose tier (new) | ✓ 1 GiB cap per file. ✗ unbounded walk (D6-01) | ✓ NotFound falls through | n/a | n/a (unit only) | bin unit |
| bgsm | ✓ depth 16 + visited; canonicalised self-loop cache | ✓ diagnostics on every parse site | per field, ceiling 22 | ✓ rate 1.0 + pinned sizes (#5904) | manual |
| sfmaterial | ✓ nesting 64; streaming index; ancestry ≤ 64, cycle-safe | ✓ degradations warned once per build. ✗ #5665 open | header + pinned vocabularies | ✓ exact | manual |
| hkx v8 clips | ✓ 1 M samples, 256 frames/block | ✓ | ✓ | ✓ SE exact; LE parity | manual; #5446 open |
| hkx v11 spline + lossless | ✓ spline. ✗ lossless frames not file-tied (D1-03) | ✓ track drop, reference-frame refusal. ✗ no lossless quaternion normalisation (D5-01) | ✓ v11 + contents; lossless 64-bit only | ✓ FO4 exact (14,370 clips, 0 dropped) | manual + v11 default-lane fixtures |
| hkx behavior graphs | ✓ table caps, ancestor set, 1 M-step budget. ✗ leaf tables uncharged (D1-01); inlining ×65 (D1-02) | ✓ | ✓ 64-bit only | ✓ FO4 / SE census (steps only) | manual |
| facegen | ✓ | ✓ | ✓ magics | ✓ sidecar census (#5903) | manual |
| menuxml parse | ✓ all include / tile / fetch budgets | ✓ | n/a | ✓ Oblivion + FO3/FNV pinned (#5902) | `ci.yml` unit; manual |
| menuxml tex | ✓ payload checked before allocation for all BC (#5667) and new BC4/BC5/DX10; 1 GiB budget | ✓ | DXT1/3/5, ATI1/2, BC4U/5U, DX10 BC1–5 / RGBA8 / BGRA8 / R8 | ✓ menu census; no BC4/BC5/DX10 real-data sweep (terrain maps unit-tested only) | same |
| audio `.fuz` (new, shared with audio) | ✓ container offsets checked. ✗ ffmpeg output uncapped (D2-01) | ~ silent-on-missing ffmpeg by design; temp write misreported | magic + RIFF/XWMA + fmt tag | ✓ `bsa_real` `.fuz` census | manual |
| game-detect | ✓ VDF depth 32 | ✓; argv and `[roots]` fixed | n/a | unit only | ✓ `ci.yml` |

## Already tracked: re-verified, not re-filed

| Issue | State at HEAD |
|---|---|
| #5446 | OPEN: three of four HKX dimension-gate fixtures leave `mask_size` 0 |
| #5663 | OPEN: the `open_shared` stamp is still `(len, mtime)` (`asset_provider/archive.rs:286-300`) |
| #5664 | OPEN: reused `open_shared` handles skip the shadow count. The new loose tier is pushed through `log_opened_archive(.., reused: false)`, so it does count. |
| #5665 | OPEN: no `catch_unwind` on the CDB index worker |
| #5502 / #5509 / #3398 | OPEN: CDB glass coverage, additive blend modes, phase 2. No parked CDB field leaks into a live slot (`apply_cdb_material` unchanged in this respect) |
| #6156 | OPEN (tech-debt): the Skyrim `.fuz` voice-path builder duplication. Not re-filed |
| #5666 / #5667 | CLOSED; their fixes are in place (`e16a624e9` reserves the slot before queueing; `bc_output` is checked before allocation) |
| #5891–#5911 | CLOSED; the fixes hold. #5891 and #5893 have the residuals PAR-D1-01 and PAR-D1-02 |
| PERF-D3-01 (this suite) | Terrain height maps decode the full DDS on a disk-cache hit: performance, not re-filed here |
| NIFAL-D7-2026-10-21-01 (this suite) | `convert_hkx_clip` drops the reference-frame yaw: not re-filed |
| NIF-D5-2026-10-21-01 (this suite) | FO76/SF ragdoll TAG0 blobs: not re-filed |

## Cross-audit routing

- **`/audit-audio`**: PAR-D2-01 (`fuz.rs` is co-owned).
- **`/audit-scripting` Dim 5**: PAR-D1-02's retained nested copies feed `ObjectAnimationGraph` runtime walks.
- **`/audit-performance`**: the `vec![Vec::with_capacity(frame_count); n]` in `decode_lossless_in` (891). Only one of the `n` clones keeps its capacity. Trivial, not filed.

## Summary

| Severity | NEW | Existing |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 2 | 0 |
| MEDIUM | 2 | 0 |
| LOW | 3 | 4 |

Suggested next step: `/audit-publish docs/audits/AUDIT_PARSERS_2026-10-21.md`. Suggested labels:

| Findings | Labels |
|---|---|
| PAR-D1-01, PAR-D1-02 | `animation`, `safety`, `game:fo4` |
| PAR-D5-01, PAR-D1-03 | `animation`, `game:fo4` |
| PAR-D6-01, PAR-D6-02 | `import-pipeline` |
| PAR-D2-01 | `audio`, `game:skyrim` |
