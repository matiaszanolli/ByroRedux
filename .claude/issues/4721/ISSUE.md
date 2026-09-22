# UI-D3-2026-09-21-02: the #3103 regeneration left the Skyrim catalog's size and shape stale in ui.md, ROADMAP and three in-source docs

**Issue**: #4721
**Severity**: LOW
**Labels**: low,ui,documentation,doc-rot

## Description
The #3103 catalog regeneration (`900c5d239` / `ba13f8c46`) brought `SKYRIM_SKYUI_METHODS` from 74 to 142 entries (14 requests, not 12), but never touched `docs/engine/ui.md`, `ROADMAP.md`, or several in-source doc comments, all of which still describe the pre-#3103 74-entry catalog.

## Evidence
Verified at HEAD `ee6d3fb39`:
- `docs/engine/ui.md:579-583`: "`for_profile(SkyrimAvm1)` contains the 74 literal `GameDelegate.call` method names … 12 are marked as callback requests."
- `docs/engine/ui.md:658`: "74 recognized methods, 12 request contracts".
- `docs/engine/ui.md:737`: "the 74-method sorted catalog" (test coverage description).
- `docs/engine/ui.md:722-727`: "59 default tests plus 2 ignored" — measured at HEAD, 82 pass + 3 ignored in the lib alone.
- `ROADMAP.md:43`: "M48 now carries Skyrim's 74-method `GameDelegate` catalog", present tense.
- `crates/ui/src/catalog.rs:22-24`: `Measured`'s doc says Skyrim is "every entry a direct command" — contradicted by the array's own 12 (now 14, counting the 2 heuristic requests) `request`/`request_heuristic` entries and by the array's own doc at `:152-154`.
- `crates/ui/src/catalog.rs:29-30,76-77,87-88`: the heuristic-provenance doc comments name only Fallout 4's 131 heuristic entries, omitting Skyrim's 68.
- `crates/ui/src/avm1_host.rs:56-59`: `unresolved` "reads **0** on the vanilla corpus" — measured 72 today, and the sweep test asserts `<= 72`, not `== 0`.
- `crates/ui/src/avm1_host.rs:84-85`: calls the completeness assertion sound "only at 0", yet it asserts at 72.
- Actual current catalog counts (re-derived from `crates/ui/src/catalog.rs`, matches the report's table): `SKYRIM_SKYUI_METHODS` = 142 (74 measured: 62 command + 12 request; 68 heuristic: 66 command + 2 request); `FALLOUT4_BGS_CODE_OBJECT_METHODS` = 269 (138 measured: 121 command + 17 request; 131 heuristic: 115 command + 16 request) — the FO4 figure already matches its own docs.

## Impact
Documentation only — no runtime behavior is affected. `ui.md` undercounts the Skyrim surface by nearly half, and the in-source completeness-assertion text misstates what "0 unresolved" vs. the actual 72-site upper bound means for trusting the sweep.

## Related
- #3153 (closed) — an earlier instance of the same catalog-count doc-rot pattern.
- UI-D3-2026-09-21-01 (this report) — the same array's classification gap.

## Suggested Fix
Update the listed sentences to 142 = 74 measured + 68 heuristic, 14 requests (12 measured + 2 heuristic). Replace "reads 0" in `avm1_host.rs` with the 72-runtime-name upper bound the sweep test already asserts. Generalize the heuristic-provenance doc comments to name both games' heuristic counts. Re-measure and correct the test-count line in `ui.md:722-727`.

## Completeness Checks
- [ ] **DROP**: N/A — documentation-only change
- [ ] **DOC**: All six cited locations (`ui.md` ×4, `ROADMAP.md` ×1, `catalog.rs`/`avm1_host.rs` in-source comments) updated together so they do not drift independently again

Source: docs/audits/AUDIT_UI_2026-09-21.md (UI-D3-2026-09-21-02)
