# #7417

## M39: budget reads in flight and make texture eviction follow TXS-BUD-003 / -004, then re-measure A8 and A10

Part of #6630 (Phase 3), under #6626. Requirements: [Texture Streaming PRD](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/texture-streaming.md#55-budget-and-eviction-txs-bud), revision 9.

There are two defects in the same budget code. Fix the reads in flight first, so the eviction-order change has a clean baseline.

## Problem 1: a read can land without budget
A promotion is checked against the budget when it is requested. Its read finishes frames later, and `queue_promotion_bytes` (`crates/renderer/src/texture_registry/upload.rs`) queues it after checking only that the texture is still held and the change would still raise its detail. The plan does not count reads in flight, and a finished read lands after that frame's budget step. A promotion the latest plan no longer covers is therefore still applied, and the next frame evicts to catch up. This conflicts with TXS-BUD-002 and TXS-BUD-004.

Measured at `22fbbad43` with temporary instrumentation, on the FO4 moving route (below) at pinned budgets of 340 and 260 MiB, three runs each:

- In one set of six unchanged runs, the budget's own byte count (payload for streamed textures, allocation for whole ones) was over budget in 1,157 logged frames. Every one had a read the latest plan did not cover land in it. The plan itself was never over budget in a logged frame. These runs did not separate declined promotions from detail that was no longer wanted.
- 12 of those frames were over 1.03 × budget (worst 1.045). Allocation rounding was a steady 4.3–4.7 MiB (1.4 % at 340 MiB, 1.8 % at 260 MiB).

### Fix: reserve budget while reading
From the hand-off in `take_promotion_requests`, count the read exactly like a queued change: its bytes at the target level, with the texture locked (not a new candidate, not evicted, not demoted) until the read lands, fails or the texture is released.

A prototype did this, and was compared with dropping uncovered reads on arrival. Medians of three runs in a separate set from the figures above:

| | 340 unchanged | 340 drop | 340 reserve | 260 unchanged | 260 drop | 260 reserve |
|---|---:|---:|---:|---:|---:|---:|
| Frames over the budget's byte count | 85 | 0 | 0 | 287 | 0 | 0 |
| Frames over 1.03 × budget | 2 | 0 | 0 | 1 | 0 | 0 |
| Highest resident ÷ budget, any run | 1.045 | 1.014 | 1.014 | 1.035 | 1.018 | 1.018 |
| Textures short of detail, mean per frame | 47.8 | 49.9 | 36.1 | 121.5 | 134.6 | 120.3 |
| Detail levels short, mean per frame | 54.3 | 57.1 | 40.9 | 150.7 | 165.7 | 153.1 |
| Promotions applied | 2,981 | 2,606 | 4,164 | 1,778 | 883 | 1,383 |
| Evictions | 3,748 | 3,393 | 4,508 | 2,089 | 1,073 | 1,515 |
| Declined promotions | 4,371 | 4,313 | 4,023 | 7,928 | 8,221 | 7,976 |
| Churn | 4,701 | 4,184 | 5,872 | 2,302 | 1,103 | 1,682 |
| Reads discarded | 0 | 345 | 0 | 0 | 324 | 0 |
| Frame p99, ms | 47.8 | 46.0 | 48.0 | 48.0 | 47.3 | 47.1 |

- Prefer reserving: it kept the budget without discarding reads. It left less detail short than dropping at both budgets, and less than the unchanged code at 340 MiB; at 260 MiB it was about level with the unchanged code.
- Its cost: at 340 MiB, evictions were about 20 % and churn about 25 % above the unchanged code. At 260 MiB both were about 27 % below it. Why they differ was not investigated.
- Frame p99 showed no clear difference. No more than 4 reads were reserved at once (`MAX_READS_IN_FLIGHT`), and none remained in the last frame.
- "Short of detail" counts textures resident coarser than wanted. Image quality was not compared.

For the real change:
- End the reservation on both release paths, `release_ref` and `release_refs_batch`. The prototype cleared only the first and relied on ignoring reservations whose stream source was gone.
- Only a pinned budget was tested. Check what happens when a derived budget shrinks while reads are in flight.
- The prototype was not run with the Problem 2 changes.

## Problem 2: the eviction order
`spend_budget` (`crates/renderer/src/texture_registry/upload.rs`) does not follow the approved eviction rules. At `25d12ae76`:

- **Over budget**, it takes levels finer than wanted from textures of every age before it takes any wanted level. A texture promoted inside the hysteresis window can lose surplus while a texture outside the window keeps all its wanted levels.
- It takes all it can from one texture before moving to the next. Of two tied textures that need to give up two levels between them, one can lose both.
- **To fund a promotion**, it takes from any texture resident finer than wanted, whatever its age, including one inside the window.

## Required behaviour
**Over budget**, plan one level at a time until the plan fits, never below a floor:
1. Textures outside the hysteresis window go before textures inside it.
2. Within each group, the oldest last-wanted frame goes first.
3. Textures with the same last-wanted frame first give up levels finer than wanted, then wanted levels. In each stage, each gives up one level before any gives up a second.

The levels planned for one texture may still be applied as one image replacement. The PRD does not order textures that are equal on all three; keep a deterministic tie-break, as the handle order is now.

**To fund a promotion**, use the same order, but only on textures that were last wanted strictly before the promoted one and are outside the window. If they cannot make room, decline the promotion, evict nothing and count the decline.

## Implementation notes
From reading the code; none of this has been run.

- Every promotion candidate is wanted in the current frame, because a texture nothing references targets its floor. "Strictly older" therefore means "not wanted this frame", and for those textures every level above the floor is surplus.
- Surplus on textures in view can no longer fund a promotion. The normal hysteresis demotion should still free it, and candidates are rebuilt every frame, so a declined promotion can succeed later. Expect more declines in A8.
- A new stream source starts with `last_wanted_frame` set to the frame it was installed, even if nothing has referenced it. PRD §2 defines the value by the latest reference and does not cover a never-referenced texture. State which behaviour the change keeps; if it contradicts §2, revise the PRD.
- Design §12.4 recorded 0.52 ms per frame for the wanted-mip pass on FO4 MedTek. Taking one level per texture per round must not make the pass noticeably slower.

## Tests
In `crates/renderer/src/texture_registry_tests.rs`. Add tests that fail on the current code:
- A recently promoted texture with surplus is spared while a texture outside the window loses a wanted level.
- Two tied textures each give up one level. With `budgeted_registry(&[0, 0], 2 * BC1_FROM_0 - 2049)` and both wanted at level 0, expect `[(0, 1), (1, 1)]`; the current code should give `[(0, 2)]`.
- A promotion cannot take surplus from a texture wanted this frame.
- A promotion cannot take from an older texture that is inside the window.

`a_promotion_that_does_not_fit_evicts_the_unwanted_first` will need its setup changed: it runs at frame 0 and the test sources start at `last_wanted_frame: 0`, so its victim is not strictly older.

For reads in flight:
- Hand a promotion to the host, then let another texture compete for the same room before the read lands. After `apply_wanted` and the landing, the planned bytes are still within the budget. This should fail on the current code.
- A failed read and each release path free the reservation.

## Moving route
This is the route `docs/smoke-tests/m-exteriors.sh fo4 soak` uses: FO4 with `--grid 0,0 --radius 1 --wrld Commonwealth --bench-frames 1800 --bench-mode renderer-stepped --bench-camera grid-soak --fly --upscaler taa`. Pin the budget with a `BYROREDUX_SETTINGS_PATH` TOML (see ROADMAP's texture streaming repro row). The `tex-stream:` line prints `peak_bytes`, `evictions`, `declines` and `churn`, so these checks need no extra instrumentation. The soak script itself still fails on #6632.

## Acceptance Criteria
- [ ] TXS-BUD-002 / -004: a read in flight is counted at its target level until it lands, fails or its texture is released, and landing it never takes the planned bytes past the budget
- [ ] TXS-BUD-002: on the moving route at 340 and 260 MiB, `peak_bytes` is at most 1.03 × budget
- [ ] The design records `churn`, evictions and declines on the moving route at both budgets, once with only the reservation fix and again with the eviction-order change
- [ ] TXS-BUD-003: over-budget demotion follows the order above, one level at a time, never below a floor
- [ ] TXS-WANT-005: the over-budget pass takes from a texture inside its window only when no texture outside the window is above its floor
- [ ] TXS-BUD-004: a promotion evicts only strictly older textures outside the window; otherwise it is declined, evicts nothing and is counted
- [ ] The new tests fail before the change and pass after it; the renderer tests pass
- [ ] Design §7.2 is rewritten to match, and §3.1's description of `last_wanted_frame` is corrected, in the same change
- [ ] TXS-BUD-002 / A10: the `p5-soak.sh` probe checks that peak resident bytes (`resident … (peak N)` in `tex.residency`) are at most 1.03 × budget, replacing its check of bytes above floors and whole textures
- [ ] A8 passes on FO4 and Starfield at two budgets each (Phase 3)
- [ ] A8 is re-run on FNV and Skyrim SE and run for the first time on Oblivion and FO3 (these four count toward #6631). Each budget is below the streaming-on resident bytes and above floors plus whole textures; where no such value exists, use a denser cell and record it in the design
- [ ] A10 passes under the new probe
- [ ] A7 is re-run with zero churn, as a regression check
- [ ] ROADMAP's Texture streaming table has the new A8 results, with budget adherence as peak resident ÷ budget; the note that A8 predates this change is removed; ROADMAP stays within 800 lines

For the Starfield runs, close other applications first: design §12.5 recorded about 16 GiB of engine memory use.

## Out of scope
A9 and #6632, A11, turning streaming on by default, the budget setting's in-game description, and the 128 MiB minimum derived budget.

