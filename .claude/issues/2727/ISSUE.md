# #2727: The only catalog-drift guard is `#[ignore]`d and one-directional, so dead catalog entries are structurally undetectable

- **Severity**: LOW
- **Dimension**: 9 (Test Hygiene)
- **Location**: `crates/ui/src/avm2_host.rs:988-1017`, `crates/ui/src/host/tests.rs:246-283`
- **Status**: NEW
- **Description**: `installed_fallout4_host_calls_are_cataloged` extracts the
  BGSCodeObj method names actually referenced by three installed FO4 movies and
  asserts `methods ⊆ catalog`. It never asserts anything about catalog entries
  the corpus does *not* reference, so a bogus entry passes forever. It is also
  `#[ignore = "requires an installed Fallout 4 corpus"]`, so it runs only on an
  explicit `--ignored` invocation on a machine with FO4 installed. Skyrim has
  **no** equivalent at all — `SKYRIM_SKYUI_METHODS` is pinned to a SkyUI git
  tree (`835428728e…`) by comment only, with nothing checking the tree still
  says that.
- **Evidence**: `catalog.contains(method)` filter at `avm2_host.rs:1006` — the
  assertion is `unknown.is_empty()`, one direction only. The default-suite
  catalog test (`host/tests.rs:246`) asserts length, sortedness, one membership
  and one kind — all of which TD8-2026-08-12-02's mangled entry satisfies.
  Verified empirically: the full suite passes (16 passed, 3 ignored) *with* the
  bad entry present.
- **Impact**: The gate that exists reads as coverage but cannot fail on the one
  defect class it looks like it addresses. This is how a malformed entry
  survived into a checked-in, doc-referenced, ROADMAP-cited 138-method catalog.
- **Related**: Rhymes with today's **#2702** (tests that re-implement production
  logic and therefore cannot fail) — same outcome (a test that cannot detect
  its nominal target), different mechanism (asymmetric assertion + opt-in
  gating rather than logic duplication). Not a re-file.
- **Suggested Fix**: Add the reverse assertion behind the same `#[ignore]` —
  report catalog entries no representative movie references, as a *warning
  list* rather than a hard failure (legitimate entries exist for menus outside
  the three-movie sample). Separately, add a default-suite well-formedness
  assertion that every catalog name matches `^[A-Za-z][A-Za-z0-9]*$` **and**
  contains no embedded ActionScript keyword (`function`, `var`, `return`) —
  that single cheap check catches TD8-2026-08-12-02's whole artifact class
  without needing a game install.
- **Effort**: small

---
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-12.md` (finding `TD9-2026-08-12-01`)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan/wgpu objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (prefer a default-suite test, not `#[ignore]`d)

