# #6991: TD9-2026-11-15-02: three source scans added in the window sit outside every `.claude/issues/<N>/CLASSIFICATION.md` record

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6991
- **Labels**: low,gameplay,audio,test-gap,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

**Partly deferred**: the two `byroredux/src/detection/` scans are M65 code still being edited (#6908 tracker), and they belong in the #6686 classification record. The `video` and `audio` rows are independent.

- **Severity**: LOW
- **Dimension**: 9 (source-scan tests)
- **Location**: `byroredux/src/detection/sight.rs:170-213` (`the_detection_module_is_the_only_caller_of_the_line_test_and_the_range`); `byroredux/src/detection/tick.rs:413-427` (`get_detection_level_toward_the_player_answers_the_real_level`, last line only); `crates/video/src/lib.rs:654-665` (`every_reader_thread_runs_under_the_decoder_guard`); `crates/audio/src/tests.rs:2256` (`the_movie_hold_skips_sounds_already_stopping`)
- **Age**: `995ed089b` and `49c92afbc` 2026-11-15 (detection); `0565c491d` 2026-11-15 (video); audio earlier in the window
- **Effort**: small
- **Labels**: `tech-debt`, `test-gap`
- **Status**: NEW; belongs in the #6686 record (binary crate, OPEN) for the first two. `video` and `audio` have no record.
- **Description**: `.claude/issues/6686/CLASSIFICATION.md` names neither `detection/sight.rs` nor `detection/tick.rs` (the detection module landed after the record's last pass on 11-10 and f5210e6e6 on 11-15). `sight.rs` is the strongest instance: it walks every `.rs` under `byroredux/src` and `crates/` with `std::fs::read_dir` and asserts `!source.contains(needle)` on whole-file text for `line_of_sight_blocked(`, `config.range(` and `.range(exterior)`. It does not use `byroredux_core::source_scan::production_text`, so a comment, doc line or test body in any crate that spells a needle fails it, and it reads the whole tree on every `cargo test`. This is a "single caller" property, the shape #6685 turned into a `syn` walk (`byroredux/src/acquisition_scan.rs`). `tick.rs` ends a behavioural test with an `include_str!("../boot/world.rs")` substring check that boot installs `detects`. `video` pins "two named reader threads each call `guard_decoder(` within 260 bytes" by splitting `lib.rs` on `thread::Builder::new()`; `audio` slices `world.rs` by `"\n    }\n"` and looks for a `.filter(|sound| !sound.stop_issued)` substring, with the doc stating the runtime form needs an audio device (a legitimate device/data class).
- **Evidence**: `sight.rs:196` `for needle in ["line_of_sight_blocked(", "config.range(", ".range(exterior)"] { if source.contains(needle) {`; `video/src/lib.rs:659-663` `let head = &body[..body.len().min(260)]; assert!(head.contains("guard_decoder("), …)`.
- **Impact**: no escaped defect found. The cost is false failures on a comment, silent blindness if `guard_decoder` is renamed in the reader but still mentioned in the window, and a test that reads about 1,500 source files per run.
- **Related**: #6686, #6685 (`acquisition_scan.rs` precedent), #6687/#6688/#6689 (closed siblings), `crates/core/src/source_scan.rs`.
- **Suggested Fix**: add the four tests to the #6686 record with their class. For `sight.rs`, restrict the walk with `production_text` or move the "single caller" property to a `syn` pass reusing `acquisition_scan`; for `tick.rs`, assert the installed query by running `boot` world construction or drop the substring line; for `video`, replace the 260-byte window with a function-level call (`fn run_reader(...)` that both threads must call).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
