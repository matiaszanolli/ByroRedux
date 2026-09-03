# #3738: `recreate_screen_passes` has regrown to 700 LOC (renderer)

- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/renderer/src/vulkan/context/resize.rs` — `recreate_screen_passes`
- **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-30.md` (`TD1-2026-08-30-03`), HEAD `64f64480`

## Description

#1671 (CLOSED) split `recreate_swapchain` at 761 LOC into `recreate_swapchain_core` (now
332 LOC) plus siblings. `recreate_screen_passes` has since grown to **700 LOC** in the
same file — 4 LOC short of what triggered the original split, and the same shape: one
linear rebuild of every screen-sized attachment and its dependent descriptor writes.

## Suggested Fix

Split per pass group, mirroring the attachment families the function rebuilds —
G-buffer attachments / SVGF + TAA history / composite + bloom chain / upscaler inputs.
Each group is an independent `create → transition → write descriptors` triple.

**Caution — render-pass adjacent.** Do not change layout-transition order or barrier
placement while moving code; validate under `BYRO_VALIDATION=1` rather than on
`cargo test` alone. Use the `sed`-extract method and diff-check (`cargo fmt` reformats
the whole crate).

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: Vulkan object lifecycle still reverse-order correct
- [ ] TESTS: regression test pins this fix

---

# #3739: `build_scheduler` is 818 LOC (ecs)

- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `byroredux/src/boot.rs` — `build_scheduler` (`pub(crate) fn build_scheduler() -> Scheduler`)
- **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-30.md` (`TD1-2026-08-30-04`), HEAD `64f64480`

## Description

One 818-LOC function listing every `add_to_with_access` / `add_exclusive` call in stage
order, plus three release-level `assert_eq!` access-report guards at its tail, inside a
1797-production-LOC `boot.rs`. Largest non-renderer function in the workspace.

## Suggested Fix

One `register_<stage>_systems(&mut scheduler)` per `Stage`
(`Early` / `Update` / `PostUpdate` / `Physics` / `Late`), with `build_scheduler` reduced
to five calls plus the guard block.

Effort: small (mechanical, `sed`-extract per stage; diff-check because `cargo fmt`
reformats the whole crate).

## Completeness Checks
- [ ] LOCK_ORDER: if a RwLock scope changes, TypeId-sorted acquisition preserved
- [ ] TESTS: regression test pins this fix — three release `assert_eq!` guards must still fire
