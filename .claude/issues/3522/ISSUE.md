# Issue #3522 — AUD-2026-08-27-D7-02

Source: `docs/audits/AUDIT_AUDIO_2026-08-27.md` · https://github.com/matiaszanolli/ByroRedux/issues/3522

Filed from `docs/audits/AUDIT_AUDIO_2026-08-27.md` (finding `AUD-2026-08-27-D7-02`). This is the **unfixed half of CLOSED #3087** — the closing commit `159307e8` touched `byroredux/src/boot.rs` only, leaving the second clause of that issue's two-clause title live at HEAD `969d81c8`.

- **Severity**: LOW
- **Dimension**: Gameplay Audio Wiring (documentation)
- **Location**: `byroredux/src/systems/audio.rs:55-57`
- **Related**: #3087 (closed 2026-08-26, this is its unfixed half); #1858 / #2731 (the `main.rs` → `boot.rs` / `app_*.rs` splits this comment predates)

## Description

#3087 ("stale audio scheduler-wiring comments — `audio_system` described as a 'Phase 1 stub', `reverb_zone_system` registration attributed to main.rs") was closed by `159307e8` on 2026-08-26. That commit touched **`byroredux/src/boot.rs` only** (23 lines, `git show --stat 159307e8`). The first half of the finding is genuinely fixed — `grep -n "Phase 1 body is a stub" byroredux/src/boot.rs` returns nothing. The second half is untouched:

```rust
/// Runs in `Stage::Late` alongside `audio_system` (registered first
/// in main.rs so the level is in place before any new spatial track
/// gets constructed this frame).
```

Two errors in one sentence.

(a) The registration is in `byroredux/src/boot.rs:1411-1417`, not `main.rs` — `main.rs` has been a thin App-construction module since #1858/#2731 and contains no scheduler registration at all. `boot.rs`'s own companion comment (`byroredux/src/boot.rs:1406-1408`) even asserts *"This `build_scheduler` block is the registration authority"*, which the `systems/audio.rs` comment directly contradicts.

(b) The stated mechanism — "registered first" — is not what guarantees the ordering. `reverb_zone_system` is registered **parallel** (`add_to_with_access`) and `audio_system` **exclusive** (`add_exclusive`, `byroredux/src/boot.rs:1481`); the guarantee comes from `Scheduler::run` executing a stage's entire parallel batch before its exclusive list (`crates/core/src/ecs/scheduler.rs:9`, `475-520`), not from registration order. Under the comment's stated mechanism, a maintainer converting `reverb_zone_system` to an exclusive registered *after* `audio_system` (a plausible "make the ordering structural" refactor, exactly what #2731-era work did to `audio_system` itself) would silently invert the dependency and give every spatial track built this frame *last* frame's send level.

## Evidence

`git show --stat 159307e8` → `byroredux/src/boot.rs | 23 +++---`, one file. `grep -n "main.rs" byroredux/src/systems/audio.rs` → line 56, the only hit and still present at HEAD `969d81c8`.

## Impact

Documentation only, no runtime behaviour — but it is the live in-file comment a maintainer reads while editing `reverb_zone_system`, and it misdirects both to a file that no longer registers anything and to an ordering mechanism that does not hold. Also a process signal: a closed issue whose two-clause title named two sites, fixed at one.

## Suggested Fix

Rewrite the parenthetical as "(registered in `boot.rs::build_scheduler` as a `Stage::Late` **parallel** system, while `audio_system` is a `Stage::Late` **exclusive**; the scheduler runs a stage's parallel batch before its exclusive list, so the send level is in place before any spatial track is constructed this frame)".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other in-file comments attributing scheduler registration to `main.rs`)
- [ ] **TESTS**: A regression test pins this specific fix
