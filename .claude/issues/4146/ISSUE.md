### AUD-2026-09-11-D6-01: `reverb_zone_system`'s ordering-guarantee comment has drifted stale for the second time — wrong file (again) and a mechanism that was never actually correct

- **Severity**: LOW
- **Dimension**: Reverb Send & Routing / Manager Lifecycle & ECS/Cell Streaming / Gameplay Audio Wiring (independently re-derived by three dimension agents in the source audit; reported once here)
- **Location**: `byroredux/src/systems/audio.rs:55-58`
- **Status**: Regression of #3522 (closed 2026-08-31 by `4dabfbaf`, which fixed the file-attribution half of the original finding but restated, rather than corrected, the mechanism half — and has since drifted stale on the path too, following the unrelated `8c5e02aa` `boot.rs` → `boot/` split on 2026-09-09). Confirmed still present at HEAD during publish.
- **Source**: `docs/audits/AUDIT_AUDIO_2026-09-11.md`

**Description**: The live comment on `reverb_zone_system` reads:
```rust
/// Runs in `Stage::Late` alongside `audio_system` — registered earlier
/// in `boot.rs::build_scheduler` (systems within a stage run in
/// registration order) so the send level is in place before any new
/// spatial track gets constructed this frame.
```
Two independent problems:

1. **Stale file reference.** `boot.rs` no longer exists — confirmed absent during publish (`ls byroredux/src/boot.rs` → not found); it was split into `byroredux/src/boot/` under commit `8c5e02aa` ("refactor(boot): split boot.rs into boot/ — one file per concern", 2026-09-09). `reverb_zone_system`'s actual registration is at `byroredux/src/boot/schedule/late.rs:165-171`, inside `register_late_systems`.
2. **The stated mechanism was never correct for this pairing, even before the file split.** "Systems within a stage run in registration order" is true for *exclusive-vs-exclusive* ordering (a plain sequential `Vec` iteration — `crates/core/src/ecs/scheduler.rs:511-514`) but false for the *parallel-vs-exclusive* relationship this comment is actually describing. `reverb_zone_system` is registered via `add_to_with_access` (`late.rs:165`), landing in `Stage::Late`'s **parallel** vector — confirmed directly during publish; `audio_system` is a bare `add_exclusive` (`late.rs:234`, confirmed), landing in the **exclusive** vector. `Scheduler::run` (`scheduler.rs:497-514`) always drains a stage's entire parallel batch (via a blocking `rayon::par_iter_mut`) before running a single entry from its exclusive list — this is what actually guarantees the ordering, and it is completely independent of which call textually appears first in `late.rs`. In fact `reverb_zone_system` is registered *after* three other `Stage::Late` **exclusives** (`make_billboard_system`, `footstep_system`, `ragdoll_writeback_system` — `late.rs:71-121`) and still runs before every one of them, precisely because it isn't one.

#3522's original filing already correctly identified mechanism (2) as the deeper problem, but the closing commit `4dabfbaf` only fixed clause (1) — it swapped `main.rs` for `boot.rs::build_scheduler` — while *restating* clause (2) in slightly different words rather than adopting the language #3522 itself already proposed. The mechanism claim was never actually corrected; it was reworded, and has now drifted stale on the path too as an unrelated refactor moved the registration site nine days later.

**Evidence**: `late.rs:165-171` (`add_to_with_access`, parallel) vs. `late.rs:234` (`add_exclusive`, exclusive) — both confirmed directly during publish; `scheduler.rs:93-99` (`StageData`'s two-vector layout) and `scheduler.rs:497-514` (`Scheduler::run`'s two-phase per-stage loop); contrast with the *correct* statement of this exact mechanism already in-tree, `late.rs:225-234` ("registered as **exclusive** so it sequences after the Late parallel batch... exclusive sequencing makes the dependency structural"), and in `byroredux/src/boot/schedule/mod.rs:27-34`'s own top-of-file explanation.

**Impact**: Documentation-only — the runtime behavior is correct and the ordering is structurally guaranteed, not incidental. The blast radius is a future refactor: this is exactly the comment a maintainer reads while touching `reverb_zone_system`, and it teaches a false mental model ("registration order" as a blanket rule) that a plausible future change — converting `reverb_zone_system` to an exclusive registered after `audio_system`, the same kind of parallel→exclusive conversion `make_billboard_system` and `footstep_system` already underwent (#3652) — would silently violate without any warning that the real invariant (staying in the `.parallel` bucket while `audio_system` stays `.exclusive`) had broken.

**Related**: #3522 (closed; this is the unfixed remainder of its own original two-part finding, now further stale on path); #3855/`8c5e02aa` (the boot split that broke the file reference); #3652 (the parallel→exclusive conversions that make the mechanism risk concrete rather than hypothetical).

**Suggested Fix**: Replace the parenthetical with the mechanism, not a file path that can drift again: state that `reverb_zone_system` is a `Stage::Late` **parallel** registration and `audio_system` a `Stage::Late` **exclusive**, and that `Scheduler::run` completes a stage's entire parallel batch before starting its exclusive list (`crates/core/src/ecs/scheduler.rs`) — independent of where either call appears in `boot/schedule/late.rs`. Consider a lightweight test that greps `late.rs` for both registrations and asserts `reverb_zone_system` uses `add_to*` while `audio_system` uses `add_exclusive*`, so a future accidental swap trips a test instead of relying on a comment surviving the next refactor.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix — grep `late.rs` for both registration calls and assert the parallel/exclusive split, so a third recurrence of this drift is a test failure instead of a documentation nit
