# #5265: CONC-D1-2026-10-05-02: FIF rider 8 still points at `groundcover.rs`'s `prepare`; #5089 moved it to `groundcover/frame.rs`, and the contract test does not pin rider 8

**Labels**: low,sync,renderer,documentation,doc-rot,test-gap
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5265

**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-10-05.md` — `CONC-D1-2026-10-05-02` (HEAD `a2c24b16e`)

- **Severity**: LOW (doc rot plus a test gap in a load-bearing list).
- **Dimension**: Vulkan Queue & AS Sync
- **Location**: The rider: `crates/renderer/src/vulkan/sync.rs:78-82`. The moved function: `crates/renderer/src/vulkan/groundcover/frame.rs:16`.
  The test: `crates/renderer/src/vulkan/sync.rs:653-768` (`frames_in_flight_contract_names_every_dependent_resource`).
- **Status**: NEW
- **Description**: Rider 8 names "`groundcover.rs`'s `prepare` on `current_frame` (#4601)", which harvests `counter_readback[frame]`.
  `0f9982177` (#5089) split `groundcover.rs` into `groundcover/construct.rs` and `groundcover/frame.rs`, and `prepare` now lives in
  `frame.rs`. The contract test pins 15 resource needles, including rider 9's `groundcover_models` and rider 15's
  `combustion_occupancy_buffers`, but nothing for rider 8. So the move went through with the rider still pointing at a file that no
  longer contains the function. The block asks to be "re-derived rather than trusted", and a stale site makes that harder at exactly
  the moment it matters: a FIF bump or the #4606/#5117 wait narrowing.
- **Suggested Fix**: Repoint rider 8 to `groundcover/frame.rs`'s `prepare`. Add a `("counter_readback", production_text(include_str!("groundcover/frame.rs")))`
  entry to the contract test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
