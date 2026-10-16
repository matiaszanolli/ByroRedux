# #5900: PAR-D2-2026-10-15-05: the #5672 Parent-walk warnings fire once per `collect_settings` call, with no memo, so a cyclic or over-deep mod chain floods the log on every merge

- **Labels**: low,bug,nifal,game:starfield
- **Filed from**: `docs/audits/AUDIT_PARSERS_2026-10-15.md`

**Full title**: the #5672 Parent-walk warnings fire once per `collect_settings` call, with no memo, so a cyclic or over-deep mod chain floods the log on every merge. A cycle back through the start object stays silent

- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**:
  - `crates/sfmaterial/src/index.rs:381-405` (`collect_settings`).
  - `:386`: `.filter(|&&p| p != obj)` ends the walk silently on a return to the start.
  - Called at `:302` (root) and `:338` (each layer material) in `lookup`.
  - Consumer: `byroredux/src/asset_provider/material/cdb.rs:230-242` (`lookup_cdb_material`, not memoised) ← `provider.rs:368` ← `merge.rs:134`, per mesh merge.
- **Status**: NEW. It is the residual of fa6666053 (#5672). The issue asked to "warn once per lookup", which landed, but a lookup is per mesh and there is one walk per layer.
- **Trigger Input**: a mod CDB where a widely shared ancestor (for example a layered template that many materials inherit) has a `Parent` cycle not through the looked-up object, or a chain deeper than 64.
- **Description**:
  - `MaterialIndex` is process-lifetime, but the warnings are emitted per walk.
  - Every merge of every material under the bad ancestor warns, once for the root and once per layer material. Starfield cells merge thousands of shapes.
  - The `Objects`-table half of #5672 warns once per build. The walk half has no equivalent memo.
  - A cycle through the start (`A → B → A`, or the self-loop `A → A`) is dropped by the `p != obj` filter with no warning, while the structurally identical `A → B → C → B` warns. Both describe the same corrupt graph.
  - No inheritance is lost in either cycle case: every node on the cycle has been visited by the time of the revisit. So the "inheritance truncated" wording is accurate only for the depth cap.
- **Evidence**: code reading. `lookup` is uncached (`cdb.rs:234-240` re-walks per call). The #5672 test asserts on message content because "the binary's other cycle test warns concurrently", which shows the repetition in miniature.
- **Impact**: log noise on corrupt mod CDBs only (vanilla has 0 cycles and a maximum depth of 24). It is diagnostic only.
- **Related**: #5672, #5502, PAR-D2-2026-10-12-04 (same once-per-path flood shape in menuxml).
- **Suggested Fix**: detect cycles and over-deep chains once at build time over `parents` (an O(n) colouring walk), warn once with a count, and keep the per-walk guards silent. Alternatively, memoise warned ancestors in a `OnceLock<Mutex<HashSet<u32>>>`. Treat the start-revisit like any other revisit.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
