# #4962: REN-D5-2026-09-27-03: `geometry_rebuild_row_matches_the_constants` still asserts "#3443 bounds the doubling below the 256 MiB threshold" — the opposite of what the pinned section now says

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4962
- **Labels**: low,renderer,memory,test-gap,documentation,doc-rot

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D5-2026-09-27-03**._

- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/mesh.rs` (`memory_budget_doc_pin_tests::geometry_rebuild_row_matches_the_constants`, the trailing `#3443` assert, its comment, and the `.expect` message; also the `chunked_rebuild_consults_the_idle_threshold_before_duplicating` expect text ">= 256 MiB rebuild duplicates …")
- **Status**: NEW (made stale by `5226d73e2`)
- **Description**: The test comment reads "#3443 landed the idle gate, so the doubling cannot reach the hard caps. A page that doubles those is arithmetic against a path the code no longer takes". Its failure message says the row must record that the gate "bounds the doubling below the 256 MiB threshold". Since `5226d73e2` the section says the reverse, and the assertion only passes because the literal `#3443` is still there.
- **Impact**: The pin checks for a token, not a claim. The next reader who trusts the test's prose will double-count or under-count.
- **Suggested Fix**: Reword the comment and message to the headroom rule: bounded by the 80% line with a reading, by 256 MiB without one. Consider pinning the needle "2× the `VERTEX_POOL_HARD_CAP`".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
