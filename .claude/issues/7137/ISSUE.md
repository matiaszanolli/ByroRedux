# #7137: PAR-D4-2026-11-20-01: The Bink movie sweep skips a whole title silently and pins no count, so the "101 of 101" ROADMAP claim can shrink without the gate going red

**Labels**: low,import-pipeline,test-gap,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7137

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D4-01 (`/audit-parsers`, HEAD f3713bf5e)

- **Severity**: LOW
- **Dimension**: Corpus Gates
- **Location**: `crates/video/src/lib.rs:866-917` (`every_installed_bink_movie_decodes_to_its_last_frame`)
- **Status**: NEW. #6993 put the sweep in the parsers lane (`real-data-gates.yml:211`, the `video` entry in the crate loop) but did not change the sweep itself.
- **Trigger Input**: a title's `Data/Video` folder that is absent or unreadable, for example a renamed install, a missing DLC, or a moved Steam library. Also a file whose 3-byte magic read fails.
- **Description**: The sweep walks seven `(game, folder)` pairs. A failed `read_dir` is a bare `continue`, and so is a failed magic read. The only coverage assertion is `decoded.iter().sum() > 0`. The sweep ignores `BYROREDUX_REQUIRE_GAME_DATA` (it keys off `BYROREDUX_STEAM_COMMON` alone), unlike every other parser real-data suite (#3850, #4660). Neither per-title nor total counts are pinned, although ROADMAP quotes "101 of 101 movies" (§ Video column, 2026-11-13). This rule (contract d) "cannot go green vacuously" is the one #4660 closed for the other parser crates.
- **Evidence**: The strict-lane run in this audit counted 101 movies (9 / 35 / 1 / 1 / 15 / 4 / 36). If any six of the seven folders went missing, the test would still pass on the seventh. `let Ok(entries) = std::fs::read_dir(..) else { continue };` is at `:880`.
- **Impact**: A missing title's movies, or a decoder regression on the only files of one title (Oblivion's nine Bink 1 movies, say), cannot turn the lane red if that folder stops being read. The ROADMAP figure goes stale without anyone noticing. The test is green today and correct about what it does decode.
- **Related**: #6993, #4660, #3850, PAR-D3-04.
- **Suggested Fix**: Under `BYROREDUX_REQUIRE_GAME_DATA=1`, panic on an unreadable folder. Pin the per-title (Bink 1, Bink 2) counts measured here as exact, or as floors for FO76 the way #6482 did, and print a re-measure line when a count changes.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix
