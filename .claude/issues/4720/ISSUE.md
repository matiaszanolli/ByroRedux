# UI-D3-2026-09-21-01: LoadDLC and RequestLoadingText are typed Command by the #3103 name-prefix guess, but vanilla passes both a response callback

**Issue**: #4720
**Severity**: LOW
**Labels**: low,ui,bug,game:skyrim

## Description
`ScaleformHostCatalog::for_profile(SkyrimAvm1)`'s doc comment (`crates/ui/src/catalog.rs:168-175`) asserts that SkyUI's fourth-argument rule ("a fourth argument means the call expects a response, i.e. `Request`") "cannot classify" the 68 #3103-regenerated entries, "because every vanilla call site passes exactly two arguments. That is still true." Measured against the real Skyrim SE corpus, it is not: `LoadDLC` and `RequestLoadingText` — both currently typed `Command` via `command_heuristic` in the catalog array — are each called with 4 arguments at their one measured call site, and `GetMouseButtonForSetDestination`/`ShouldShowMod` (already typed `Request` via `request_heuristic`) confirm the rule holds where it has been applied. Among all 526 resolved call sites in the corpus, every one of the 27 measured-`request` sites is 4-argument and every one of the 320 measured-`command` sites is 2-argument — no name has mixed arities.

The AVM1 scanner (`crates/ui/src/avm1_host.rs`) already has the per-site `CallMethod` argument count in hand during the walk but discards it rather than recording it in `Avm1HostCallInventory`.

## Evidence
Verified at HEAD `ee6d3fb39`:
- `crates/ui/src/catalog.rs`: `LoadDLC` and `RequestLoadingText` are both `ScaleformHostMethod::command_heuristic(...)` in `SKYRIM_SKYUI_METHODS`; the array totals 142 entries (62 `command` + 12 `request` = 74 measured, 66 `command_heuristic` + 2 `request_heuristic` = 68 heuristic), matching the report's re-derived catalog table.
- `crates/ui/src/catalog.rs:168-175` and `crates/ui/src/avm1_host/tests.rs:308-318` still state the "every vanilla call site passes exactly two arguments" premise.
- `crates/ui/src/avm1_host.rs` (`Avm1HostCallInventory`, the scanner) tracks `unresolved` but not per-site argument count.

## Impact
Diagnostic only — `kind` never drops a call or changes a return value. But a vanilla Skyrim menu waiting on a `respond` for `LoadDLC` or `RequestLoadingText` dispatches as `Queued` rather than `MissingResponse`, so it never surfaces through `unanswered_methods()`, `hud.debug`, or the one-shot missing-handler warning — a silent gap in the diagnostic surface these two methods are supposed to feed.

## Related
- #3103 (closed) — the regeneration that added these 68 entries as heuristic.
- #3773 (closed) — the heuristic name-prefix classification these entries fell back to.
- UI-D3-2026-09-21-02 (this report) — the same array's stale documentation.

## Suggested Fix
Record the argument count per resolved call site in `Avm1HostCallInventory` and classify the 68 heuristic entries by the fourth-argument rule instead of the name-prefix fallback. Retype `LoadDLC` and `RequestLoadingText` as `request_heuristic`/promote to `Measured`, and pin "four-arg ⇔ Request" as an invariant in the corpus sweep test.

## Completeness Checks
- [ ] **TESTS**: The corpus sweep test asserts four-arg call sites are typed `Request` and two-arg sites `Command`, for all 142 entries
- [ ] **DOC**: `catalog.rs:168-175` and `avm1_host/tests.rs:308-318`'s "cannot classify" premise text is corrected alongside the retyping

Source: docs/audits/AUDIT_UI_2026-09-21.md (UI-D3-2026-09-21-01)
