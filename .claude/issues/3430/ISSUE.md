# #3430: UI-D5-2026-08-27-05: the new `--menu` smoke gate passes a menu that loaded with every `ImportAssets` dependency missing or with a stalled preload

- **Severity**: MEDIUM
- **Dimension**: Resource Navigator / Engine Wiring
- **Profile**: both
- **Location**: `docs/smoke-tests/m48-menu-load.sh:105-126` · `crates/ui/src/player.rs:251-265`, `:543-566` · `crates/ui/src/navigator.rs:205-218`
- **Source**: `docs/audits/AUDIT_UI_2026-08-27.md` (UI-D5-2026-08-27-05)

## Description

#3273's gate (added `fdba763d`) asserts exactly two things: that `ui.menu: loaded` appears, and that none of the five `--menu`-route failure arms in `scene.rs` logged. Every failure mode *inside* the UI crate is invisible to it. By design (#2720), a dependency that cannot be fetched is **not** fatal — it is answered with `placeholder_movie()` and recorded — and an unsettled preload is advanced anyway after a 60-frame grace. So a menu whose entire font and symbol graph is missing (wrong archive, cross-archive import — the navigator holds exactly one archive, and `--menu-archive` takes exactly one path) loads, prints `ui.menu: loaded`, draws an empty stage, and the gate reports PASS.

## Evidence

The gate's failure vocabulary (`docs/smoke-tests/m48-menu-load.sh:110-126`) is `"Failed to open UI archive"`, `"Failed to extract archive menu"`, `"Failed to detect Scaleform profile"`, `"Failed to load archive menu"`, `"Failed to register UI texture"` — all five from `scene.rs`. It never greps for `record_resource_errors`' `log::error!("{error}")` (`player.rs:563`), which is where `"Scaleform resource {archive_path:?} was not found in the configured archive"` (`navigator.rs:255-257`) surfaces, nor for `"Scaleform archive preload for {movie_path:?} did not settle"` (`player.rs:258-262`). Both are already on stderr at `error`/`warn`; only the greps are missing.

The ignored FO4 corpus test does assert `player.resource_error() == None` (`crates/ui/src/host/tests.rs:634`) — but it is `#[ignore]`d and FO4-only, which is precisely the gap #3273 existed to close.

## Impact

The gate that converts #3147's HIGH claim from "unverified" to "verified" verifies less than its own rationale header claims ("#2968's redundant-decompress path and the whole `ScaleformNavigatorRuntime` preload loop are also exercised for the first time by a real caller on this gate"). It exercises them; it does not check their outcome.

## Related

#3273, #3147, #2720, and sibling finding UI-D3-2026-08-27-02.

## Suggested Fix

Add two greps to the failure-arm loop — `"was not found in the configured archive"` and `"did not settle after"` — and, once UI-D3-2026-08-27-02 lands, assert the `state=` token on the `ui.menu: loaded` line is `AdapterInjected*` for the FO4 case.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other smoke-test gates' failure vocabularies)
- [ ] **TESTS**: A regression test pins this specific fix (the gate itself is the test — the added greps must be exercised)
