# Parser Discipline Audit: 2026-10-27

**HEAD**: fa6be4a6b · **Baseline**: `AUDIT_PARSERS_2026-10-21.md` (HEAD 4a3e89d56) · **Audited**: Dims 1, 2, 4 and 6 in depth; Dim 5 on its changed consumer files · **Unchanged since baseline (skimmed)**: Dim 3 (version gating: no commits on `archive/open.rs`, the `ba2.rs` gates, bgsm `base`/`bgsm`/`bgem`, sfmaterial `reader.rs`, hkx `packfile.rs` or facegen), menuxml `parse.rs` / `tex.rs` / `font.rs`, game-detect `steam.rs` / `catalog.rs`, facegen `src/`, sfmaterial `src/` (one test edit).

This run was part of `/audit-suite --preset comprehensive`. It ran alone and in order, with no sub-agents. No engine was launched. Nothing in the tree changed apart from this report.

## Commits since 4a3e89d56 on this skill's paths

| Area | Commits | What landed |
|---|---|---|
| hkx | 6 | Fixes for the baseline's hkx findings: #6211 (the step budget now charges leaf tables; `MAX_GRAPH_STEPS` raised to 2 M), #6212 (`InlineBudget`: a shared weight budget and an ancestor-stack cycle cut), #6242 (lossless quaternion policy), #6306 (static lossless clip cap), #5446 (fixtures isolate their own clauses); #6401 / #6184 move real-data tests onto `test_paths` |
| asset_provider | 14 | #6243 (the loose walk gets a depth cap and a canonical visited set), #6308 (one Unicode `normalize_path` and deterministic loose collisions), #6330 (`geometries` root, loose CDBs), #5663 (`ArchiveStamp` adds `(dev, ino, ctime)`), #5664 (shadow-count memo per pool), #5665 (a panicked CDB build is memoised), #6290 / #6314 (terrain-height cache), #6241 (reference-frame yaw reaches `root_yaw`) |
| audio `fuz.rs` | 2 | #6307 (bounded `ffmpeg` run, exclusive temp file, typed errors). The file is shared with `/audit-audio` (see `_audit-owners.md`). |
| bsa / game-detect / sfmaterial | 4 | The `normalize_path` export, a clippy cleanup, `string_map` made test-only (#6192) |

All seven findings of the baseline were published as #6211, #6212, #6242, #6243, #6306, #6307 and #6308, and all seven are closed. Their fixes were re-read at HEAD and hold as written. **#6212 has a residual**, filed below as PAR-D1-01.

## Method and evidence

- **Probe.** One throwaway crate in the session scratchpad, which depends on `crates/hkx` and `crates/bsa` by path. It was built into the repo `target/` (`CARGO_TARGET_DIR=<repo>/target`), so no private target directory was used. It produced two binaries, `target/release/hkxprobe` and `target/release/fo4inline`. The first drives the real `BehaviorGraph::inline_references` on in-memory graphs. The second decodes and inlines every behavior graph in `Fallout4 - Animations.ba2`. Their numbers back PAR-D1-01 and PAR-D4-01.
- **Default lane** (`cargo test -p byroredux-{bsa,bgsm,sfmaterial,hkx,facegen,menuxml,game-detect}`): all green.

| Crate | Unit | Other |
|---|---|---|
| bsa | 114 | + 1 `ba2_real` |
| bgsm | 35 | + 1 `parse_all` |
| sfmaterial | 51 | + 6 `header_smoke` |
| hkx | 70 | |
| facegen | 31 | |
| menuxml | 62 | |
| game-detect | 60 | |
| doc-tests | 6 | |

- `cargo test -p byroredux --bin byroredux asset_provider`: 273 passed, 14 ignored.
- **Real-data suites, strict lane** (`BYROREDUX_REQUIRE_GAME_DATA=1 cargo test --release -p <crate> -- --ignored`, one crate at a time):

| Crate | Result | Notes |
|---|---|---|
| hkx | 10 / 10 | Includes both behavior-graph censuses (the FO4 maximum is 245,831 steps, under 2 M / 8), the FO4 clip census, the SE spline census and the LE parity test |
| bgsm | **2 / 3: one RED** | `parse_rate_fo76_materials_corpus` fails (PAR-D4-02). The parse itself is clean: 30,076 files plus 2 allowed JSON files, 0 failures. |
| sfmaterial | 5 / 5 | `real_cdb` |
| facegen | 5 / 5 | |
| menuxml | 11 / 11 | |
| bsa | FO76 tests only: **1 / 2, one RED** | `fo76_ba2_corpus_opens_and_lists` fails its archive count, 43 against the pinned 40 (PAR-D4-02). `fo76_dx10_texture_extract_round_trips` passes. The rest of the bsa suite was skipped: the only bsa source change was moving `normalize_path`, whose body is unchanged. |

## Executive Summary

| Severity | NEW | Existing (open) |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 1 | 0 |
| MEDIUM | 1 | 0 |
| LOW | 3 | 0 |

- **HIGH, PAR-D1-01.** #6212's weight budget does not weigh what each inline actually copies.
  - `machine_weight` counts a clip, or a whole `hkbModifierList`, as 1. That one unit can carry up to 1,024 triggers, or 1,024 x 1,024 expression strings.
  - Every inline deep-copies the cached graph through `Arc::make_mut`.
  - The result: a Fallout 4 mod graph that refers 64 times to a ~50 KB sub-behavior still makes 64 full copies.
  - Measured: +4.9 GiB and 4.8 s on the main thread from a 19 MB sub-graph. A sub-graph at the decode cap would need about 19–37 GB.
- **MEDIUM, PAR-D2-01.** The `.fuz` WAV header comes from the *last* xWMA `fmt ` chunk, and parsing continues past `data`. `ffmpeg` reads the first chunk.
  - The header is built with unchecked `u16`/`u32` multiplies.
  - A trailing `fmt ` chunk therefore mislabels the PCM. In the dev profile it panics the main thread.
- **LOW.**
  - PAR-D4-01: no real-data guard pins the #6212 inlining result. The worst vanilla graph takes 49 of the 64 allowed resolves.
  - PAR-D4-02: the FO76 pins in bgsm and BA2 are both red after FO76's 2026-10-24 patch.
  - PAR-D6-01: in the loose walker, which of two links to one directory wins depends on `read_dir` order.

There is no overlap with the suite's finished audits (§ 6 of the suite rules was re-read before this report was written).

## Findings

### HIGH

#### PAR-D1-01: `inline_references` weighs a sub-behavior by node count, not by what it holds. 64 inlines of one leaf-heavy graph are 64 deep copies, and a ~50 KB Fallout 4 mod graph can take tens of GiB on the main thread.
- **Severity**: HIGH (an OOM reachable from a mod file)
- **Dimension**: Size Discipline
- **Location**: `crates/hkx/src/behavior.rs`. The weight is `machine_weight` (:855-877) and `MAX_INLINED_WEIGHT` (:732). The charge is at :1011-1016. The deep copy happens in `visit_modifier` → `Arc::make_mut` (:757-758), reached from `remap_machine_events` / `offset_machine_modifier_ids` (:800-850) on every inline. The modifier reader is `read_modifier_node` (:1502-1517 expressions, :1540-1549 modifier list).
- **Status**: NEW. This is a residual of #6212 (closed), which fixed baseline PAR-D1-02.
- **Description**: #6212 states its invariant as "every unit weighed here cost at least one step to decode, so the inlined volume stays within one maximal graph's worth however the references repeat". The first half is true. The conclusion does not follow, because one unit can stand for about a million steps of decoded data:
  - **Clips.** A clip weighs 1, but its decode charges 1 + triggers + bindings, and it keeps up to 1,024 `ClipTrigger`s.
  - **Modifiers.** `for_each_modifier_mut` visits only the top-level modifier of each `hkbModifierGenerator`. It does not descend into `hkbModifierList` / `BSModifyOnceModifier` / `hkbEventDrivenModifier` children. So a list of 1,024 `hkbEvaluateExpressionModifier`s weighs 1. Each of those children holds up to 1,024 owned `ExpressionData` strings of at most 256 B.
  - **Copies.** Each resolve clones the cached graph (`actor_graph.rs` `decode_behavior` → `.clone()`). The event remap and the modifier-id offset then call `Arc::make_mut` on every modifier, which still has a second owner in the cache. Every inline is therefore a full deep copy, and all 64 are kept in the parent graph.
- **Trigger Input**: a Fallout 4 (packfile v11) sub-behavior shaped as one state → `hkbModifierGenerator` → `hkbModifierList` of 1,024 pointers to one `hkbEvaluateExpressionModifier`, which carries 1,024 expressions that all point at one 255-character string.
  - That is about 1.05 M steps, inside `MAX_GRAPH_STEPS` 2 M. It weighs 3. It decodes to about 1 M owned strings (~300 MB) from a file of roughly 50 KB.
  - A root graph that names it from 64 states, siblings rather than a cycle, passes the stack cut. The 64 resolves fit `MAX_INLINED_REFERENCES`, and the 192 total weight is far below `MAX_INLINED_WEIGHT`.
- **Evidence** (probe `hkxprobe`, which calls the real `inline_references` on in-memory graphs of N `EvaluateExpression` modifiers x 1,024 x 255-char expressions):

| Cached sub-graph | References | Resolved / inlined | Inlined expressions | RSS growth | Time |
|---|---|---|---|---|---|
| 64 modifiers (~19 MB) | 64 | 64 / 64 | 4,194,304 (64x) | +1,216 MiB | 1.2 s |
| 256 modifiers (~76 MB) | 64 | 64 / 64 | 16,777,216 (64x) | +4,866 MiB | 4.8 s |

  - Growth is linear in the size of the sub-graph, at about 300 B per expression per copy.
  - Extrapolated: the 1,024 x 1,024 trigger above (~300 MB per copy) gives about 19 GB, and a sub-graph at the full 2 M-step budget about 37 GB.
- **Impact**: `populate_actor_graphs` → `build` runs on the main thread (`byroredux/src/asset_provider/actor_graph.rs:106-115`, :170). The inlined graph is kept in `ActorGraphCache`. One FO4 mod behavior can OOM-kill the engine or stall it for minutes when an actor using it spawns. The trigger-table variant (clips weigh 1 but keep 1,024 triggers each) reaches the same multiplier with smaller numbers.
- **Related**: #6212, #6211 (closed). PAR-D4-01 (no real-data inline pin).
- **Suggested Fix**: charge each inline what its decode cost.
  - Option 1: record the decode's `Walk::steps` on the `BehaviorGraph` and charge that against `MAX_INLINED_WEIGHT`.
  - Option 2: make `machine_weight` count leaf-table lengths and recurse through `visit_modifier` children.
  - Either way, consider charging the copies as well, since `make_mut` makes each inline a full copy. Pin the fix with the modifier-list fixture from the Trigger Input.

### MEDIUM

#### PAR-D2-01: `.fuz` decoding builds its WAV header from the *last* xWMA `fmt ` chunk, read even past `data`, while `ffmpeg` decodes the *first*. The header arithmetic on the file's channel count and sample rate is unchecked.
- **Severity**: MEDIUM (silently wrong bytes; a panic in the dev profile)
- **Dimension**: Error Semantics
- **Location**: `crates/audio/src/fuz.rs`. The chunk walk is in `parse_xwma`: `fmt = Some(..)` is overwritten on each `fmt ` chunk, and the loop does not stop at `data`. The header is built in `wav_header`: `let block_align = channels * 2;` (`u16`) and `sample_rate * u32::from(block_align)` (`u32`). Consumer: `byroredux/src/systems/dialogue_voice.rs:282`.
- **Status**: NEW. 78be43036 (#6307) bounded the `ffmpeg` run but not this path.
- **Description**: `fuz_to_wav_bytes` passes the raw xWMA to `ffmpeg` and labels the PCM it gets back with `stream.channels` / `stream.sample_rate` from our own `parse_xwma`. The two readers disagree on which chunk they use:
  - ffmpeg's `libavformat/xwma.c` requires `fmt ` as the first chunk after `XWMA`, skips unknown chunks, and stops at `data`. This is from that source and was not reproduced here.
  - Our walker takes the last `fmt ` anywhere in the RIFF.
  - The format-tag check (`WAVE_FORMAT_WMAUDIO2`) also reads our chunk, not ffmpeg's.
- **Trigger Input**: a mod or replaced `.fuz` whose xWMA has a valid leading `fmt ` / `dpds` / `data`, plus a trailing `fmt ` with tag 0x0161:
  - With `channels` ≥ 0x8000, `channels * 2` overflows. That panics in the dev profile, which keeps overflow checks on (`[profile.release]` does not set them). The panic happens on the main thread while the `VoiceSoundCache` write guard is held. In release the value wraps.
  - With any other `channels` / `sample_rate`, the WAV header mislabels ffmpeg's real output. The line plays at the wrong speed or channel layout and comes back as `Ok`.
- **Impact**: a crash in a dev build when voiced dialogue starts, and garbled voice in release. Vanilla `.fuz` files have one `fmt ` chunk and are not affected.
- **Suggested Fix**:
  - Stop the chunk walk at `data` and keep the first `fmt `, which matches ffmpeg. Better still, take channels and rate from ffmpeg itself, for example by having it emit a WAV container.
  - Use `checked_mul` in `wav_header` and turn an overflow into `BadXwma`.

### LOW

#### PAR-D4-01: No real-data guard pins that vanilla graphs still inline every behavior reference under #6212's budgets. The worst vanilla graph uses 49 of the 64 resolves allowed.
- **Severity**: LOW (test-gap)
- **Dimension**: Corpus Gates
- **Location**: `crates/hkx/src/corpus_tests.rs`, which has step and decode censuses but no inline census. Code: `crates/hkx/src/behavior.rs` `inline_references` / `MAX_INLINED_REFERENCES`.
- **Status**: NEW
- **Description**: #6212 changed what inlining loads: an ancestor-stack cut, a shared weight budget, and a height check before the recursion. Every test of `inline_references` is synthetic.
  - Probe `fo4inline` decoded all 328 behaviors in `Fallout4 - Animations.ba2` and inlined each one through an archive-backed resolver, using the same path join as `actor_graph.rs`.
  - Today every reference inlines (0 graphs are left with an un-inlined reference).
  - The worst graph, `meshes\actors\character\behaviors\meleebehavior.hkx`, takes 49 resolves against the cap of 64, which is 1.3x headroom.
  - A tighter budget, which PAR-D1-01's fix may introduce, or a reference-heavy DLC or Creation graph would silently leave states without their sub-behavior.
- **Suggested Fix**: add the inline census to `fallout_4_behavior_graph_census_is_exact`. It should assert that every reference resolves and inlines, and pin the worst resolve count, with a ratio assert against `MAX_INLINED_REFERENCES` and the weight cap.

#### PAR-D4-02: Two FO76 strict-lane gates are red after FO76's 2026-10-24 patch. The bgsm material count (29,991 pinned, 30,078 shipped) and the BA2 archive census (40 pinned, 43 shipped) are exact pins on a live-service corpus.
- **Severity**: LOW (test-gap; both gates are red and each masks its own later assertions)
- **Dimension**: Corpus Gates
- **Location**:
  - `crates/bgsm/tests/parse_all.rs:476` (`parse_rate_fo76_materials_corpus`; pinned in c5a0b98c6, #5904, 2026-10-17)
  - `crates/bsa/tests/ba2_real.rs:724` (`fo76_ba2_corpus_opens_and_lists`; pinned in ce008af1c, #5903, 2026-10-17)
- **Status**: NEW
- **Description**: 28 FO76 archives on this install were rewritten on 2026-10-24.
  - **bgsm.** The parse is fully clean: 30,076 clean, 2 allowed JSON text files, 0 failures. But the exact-count assert runs first and fails (`left: 30078, right: 29991`). Every later assertion in the test, such as the diagnostic and over-ceiling checks, can no longer report.
  - **BA2.** `fo76_ba2_corpus_opens_and_lists` opens 43 archives against the pinned 40 (`left: 43, right: 40`, after 420 s). The entry-count assert and the "every entry extracted" assert come after it and never run, so this run does not show whether the patched FO76 archives extract cleanly.
  - `fo76_dx10_texture_extract_round_trips` passes.
  - FO76 is a live-service title, so an exact pin on its corpus breaks on every patch.
- **Suggested Fix**:
  - Re-measure and re-pin both.
  - Better: run the clean-rate and extract-all checks *before* the count asserts. Then make the FO76 counts floors, or key them on the archives' sizes or hashes, so that a patch reads as "corpus changed, re-measure" rather than hiding whether the parse still holds.

#### PAR-D6-01: The loose walker's global canonical visited set reaches a directory linked from two places under only one of the two keys, chosen by `read_dir` order, with no warning.
- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Location**: `byroredux/src/asset_provider/archive.rs` `LooseDir::walk` (`visited.insert(canonical)`, :119-124)
- **Status**: NEW (in code added by 120b7df86, #6243)
- **Description**: The visited set is global to the index rather than an ancestor stack, so a cycle and a second legitimate link look the same. Example: `Data/textures/a → /mods/x` and `Data/textures/b → /mods/x`. The files of `/mods/x` are indexed under `textures\a\…` or `textures\b\…`, whichever `read_dir` yields first. The other key falls back to the archive copy without a message. The game itself resolves both paths. #6308 made same-key *file* collisions deterministic and warned once; this directory-level drop is neither.
- **Suggested Fix**: cut cycles with an ancestor stack of canonical paths, and bound the total work with an entry budget, which is what #6243 needed the global set for. Alternatively, keep the set but record the second link's key as an alias and warn once.

## Gate Matrix

| Crate | Size caps | Error policy | Version gate | Corpus sweep (strict lane, this run) | CI lane |
|---|---|---|---|---|---|
| bsa (BSA/BA2/CSG) | `safety.rs` ceilings, `capacity_hint` (unchanged) | short-final-chunk `Ok`, otherwise `Err` (unchanged) | allowlists (unchanged) | **RED**: FO76 archive census (PAR-D4-02); the other lanes were not run this time | `real-data-gates.yml` `parsers` job, manual only (no runner registered) |
| bgsm | template depth 16 + visited set | `parse_*_diag` everywhere | per-field `version` gates, ceiling 22 | **RED**: FO76 count pin (PAR-D4-02); FO4 green | same |
| sfmaterial | `MAX_NESTING_DEPTH` 64, streamed index | typed join errors; #5665 panic memoised | header errors pinned | 5 / 5 | same |
| hkx | step budget charged per leaf table (#6211); **inline weight undercounts (PAR-D1-01)** | partial results surfaced (`dropped_bones`, `reference_frame_dropped`, `additive`) | v8 / v11 gates | 10 / 10; **no inline census (PAR-D4-01)** | same (LE parity is `--skip`ped unless the LE data env var is set) |
| facegen | morph cap, exact size | `Err` | magics | 5 / 5 | same |
| menuxml | include, tile and decoded-texture budgets | warn-once budgets | n/a | 11 / 11 | same |
| game-detect | `MAX_VDF_DEPTH` 32 | malformed input → `Err` | n/a | n/a | default lane only |
| audio `fuz.rs` (shared) | ffmpeg stdout capped at 64 MiB, 10 s timeout (#6307) | **header follows the wrong `fmt ` chunk (PAR-D2-01)** | tag 0x0161 check (on our chunk) | none | default lane only |

Suggested next step: `/audit-publish docs/audits/AUDIT_PARSERS_2026-10-27.md`. Labels: `animation` + `game:fo4` for PAR-D1-01 and PAR-D4-01 (with `test-gap` on the latter); `audio` + `import-pipeline` for PAR-D2-01; `test-gap` + `game:fo76` + `import-pipeline` for PAR-D4-02; `import-pipeline` for PAR-D6-01.
