# #7095 Oblivion menu surface coverage is unpinned: wave_one_menu_surface_coverage_is_the_published_one has only Skyrim and New Vegas rows

Labels: low, tech-debt, game:oblivion, ui · State: OPEN

## Problem
`byroredux/src/menu_drivers.rs::wave_one_menu_surface_coverage_is_the_published_one` pins `surface_coverage` only for `MenuGame::Skyrim` and `MenuGame::Fallout3NV`. The Oblivion row is unpinned.

`250a5ba67` (#7086) added the two Oblivion rest documents, sleep/wait (1012) and level-up (1027), through `byroredux/src/menuxml_rest.rs`, and `ui.coverage` now counts them. A regression that drops either document from the Oblivion registry, or double-counts it, would not fail any test.

## Suggested fix
Add an `Oblivion` row to the test, with the `SurfaceCoverage` and `covered()` values measured at HEAD. If Oblivion has a published figure, as the Skyrim and New Vegas rows do, cite where it comes from in the same commit.

Found by `/audit-sync` (meta lane, 2026-11-20).

