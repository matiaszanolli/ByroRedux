# #5274: UI-D3-2026-10-05-01: #4720's sweep measures all 66 `HeuristicNamePrefix` Skyrim entries, but the provenance, the four-argument floor and the doc numbers still describe them as guesses

Labels: low,ui,documentation,doc-rot,test-gap,game:skyrim
Filed from: docs/audits/AUDIT_UI_2026-10-05.md

**Source**: `docs/audits/AUDIT_UI_2026-10-05.md` (UI-D3-2026-10-05-01) · audit HEAD `a2c24b16e`

- **Severity**: LOW. Diagnostic metadata only; every `kind` is correct.
- **Dimension**: Catalog & AVM1 Scanner
- **Profile**: SkyrimAvm1
- **Location**:
  - `crates/ui/src/catalog.rs`:
    - `:30-42` (`HeuristicNamePrefix`: "kind was inferred … a `Command` here is a weaker claim")
    - `:180` ("27 of them (one per request-typed method)")
    - `:186-189`, `:197` ("these 68 names"), `:712-714`, `:758` ("68 sweep entries")
    - `:733-734` (a broken line join inside the assertion messages)
  - `crates/ui/src/avm1_host/tests.rs`: the `four_arg >= 14` floor and its message
  - `crates/ui/src/avm1_host.rs:107-112,333-338` (the arity merge keeps the maximum)
  - `docs/engine/ui.md:603-609` ("14 by SkyUI's fourth-argument rule … and 2 by the name-prefix heuristic") and
    `:749` ("101 default tests plus 3 ignored")
- **Status**: NEW. #4720 and #4721 are closed; this is drift those fixes left behind.
- **Description**:
  - **All 16 requests are measured.** The data run reports 16 four-argument methods. The pin asserts that every
    four-argument name is typed `Request`, and the catalog holds exactly 16 `Request` entries. So both
    `request_heuristic` entries (`GetMouseButtonForSetDestination`, `ShouldShowMod`) were measured at four arguments.
  - **All 64 heuristic commands are measured.** 125 two-argument methods = 62 measured commands + 64 heuristic
    commands − `SliderClose`, which is never called. So every `command_heuristic` entry was measured at two arguments.
  - **The floor is too low.** The sweep floor `four_arg >= 14` sits two below the measured 16, so the scanner can
    silently lose two request sites. Its message ("the corpus previously measured 14 request-typed methods") misstates
    the measurement, and so does the skill's Dim 3 premise.
  - **Mixed arities are undetectable.** `arg_counts` keeps `max(count)` per name. A name called at both 2 and 4
    arguments collapses to 4, so the documented "no mixed-arity name" claim cannot be checked.
  - **Test count.** `ui.md`'s "101 default tests plus 3 ignored" counts the ignored tests twice: `--list` gives 101
    including the 3 ignored ones.
- **Evidence**:
  ```
  arities: 16 four-argument (request) methods, 125 two-argument (command) methods
  ```
  This line is from `installed_skyrim_host_calls_are_all_cataloged`, run with data. The re-derived split is
  `command 62 / request 14 / command_heuristic 64 / request_heuristic 2`.
- **Impact**:
  - The provenance is the stated input for future handler work ("a `Command` here is a weaker claim"), and it is now
    wrong for every Skyrim sweep entry.
  - The floor no longer fails loudly when the scanner loses request sites.
- **Related**: #4720, #4721 (both closed), UI-D3-2026-09-21-01
- **Suggested Fix**:
  - Promote the 66 entries to `Measured`, or add a measured-arity provenance.
  - Raise the floor to 16.
  - Record every arity seen for a name (or its min and max) and assert that no name mixes them.
  - Correct the numbers in the doc comments and in `ui.md`.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in sibling docs / comments / skill files
- [ ] **TESTS**: Where a number is restated, a test or measured source pins it
