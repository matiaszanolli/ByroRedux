# 3273: UI-D5-04: the #3147 archive-backed --menu route fix has no verification beyond a CLI-arg-parsing unit test

**Severity**: LOW · **Report**: `docs/audits/AUDIT_UI_2026-08-24.md` (UI-D5-04)

## Description

`archive_menu_args` — the pure parser deciding whether `--menu`/`--menu-archive` were supplied — has one unit test. The Vulkan-backed remainder of the route requires a real GPU device and is untestable by `cargo test`. Unlike `docs/smoke-tests/m41-equip.sh`, there is no smoke-test script for `--menu`.

## Location

`byroredux/src/scene.rs:1402-1460` (the route) · `byroredux/src/scene.rs:1509-1529` (`archive_menu_route_tests`, the only automated coverage) · `docs/smoke-tests/` (no entry)

## Impact

The #3147 fix is real and correctly wired by static reading, but "the shipped binary cannot open any vanilla Bethesda menu" converts to "unverified whether the shipped binary can open one" — a materially lower-severity claim, since #2968's redundant-decompress path and the `ScaleformNavigatorRuntime` preload loop are exercised for the first time by a real caller today.

## Related

#3147 (the fix this verifies is unverified), #2968 (promoted by the same fix).

## Suggested Fix

Add `docs/smoke-tests/m48-menu-load.sh` following the `--bench-hold` pattern: launch with `--menu interface\hudmenu.swf --menu-archive "Fallout4 - Interface.ba2" --bench-hold`, attach `byro-dbg`, assert the UI texture handle is populated / the menu reached frame 1.

## Completeness Checks
- [ ] **TESTS**: A smoke test (`docs/smoke-tests/m48-menu-load.sh`) exercising the real Vulkan-backed route
