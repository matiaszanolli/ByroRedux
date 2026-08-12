# #2726: FO4 catalog carries `functiononGPSModeButtonClicked`, a whitespace-collapse extraction artifact

- **Severity**: LOW
- **Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
- **Location**: `crates/ui/src/catalog.rs:280`
- **Status**: NEW
- **Description**: `FALLOUT4_BGS_CODE_OBJECT_METHODS` contains
  `ScaleformHostMethod::command("functiononGPSModeButtonClicked")`. That is
  `function onGPSModeButtonClicked` with the space removed — an artifact of
  scraping the F4CF/Interface ActionScript sources. It is the only entry in
  either catalog that is not a plain Camel/camelCase identifier; all 137 other
  FO4 entries and all 74 Skyrim entries are well-formed. The genuine method
  `onGPSModeButtonClicked` is **absent** — sorted order would place it between
  `onFadeDone` (`:294`) and `onGridAddedToStage` (`:295`), and it is not there.
- **Evidence**: `crates/ui/src/catalog.rs:280` verbatim; the sorted-window
  assertion at `host/tests.rs:271-274` passes because the mangled name still
  sorts correctly, so sortedness cannot detect it, and `len() == 138` at `:268`
  counts it as a valid entry.
- **Impact**: Two-sided, both small. (1) `build_adapter_abc` emits one
  forwarding method + one method body + one trait + two constant-pool strings
  per catalog entry, so every FO4 SWF the engine patches carries a dead helper
  and a dead `BGSCodeObj.functiononGPSModeButtonClicked` property. (2) The
  Pip-Boy map's real GPS-mode button, when it fires, normalizes to
  `onGPSModeButtonClicked`, misses the catalog, and is classified
  `ScaleformHostDispatch::Unknown` — logged as an unknown method rather than
  queued. Neither breaks anything today (nothing drains the queue — see
  TD8-2026-08-12-03), but the catalog is documented as the recognition surface
  future work is specified against.
- **Related**: Only detectable by TD9-2026-08-12-01's guard if that guard ran
  and were bidirectional; it is neither.
- **Suggested Fix**: Replace the entry with `onGPSModeButtonClicked` (same
  sort position, so `len()` stays 138 and no test needs touching). While there,
  re-run the extraction with a whitespace-tolerant pattern to confirm this was
  the only collapse artifact — the four intentional case-pairs
  (`CloseMenu`/`closeMenu`, `GetButtonFromUserEvent`/`getButtonFromUserEvent`,
  `OnAcceptPress`/`onAcceptPress`, `PlaySound`/`playSound`) are documented as
  deliberate at `docs/engine/ui.md` and must be preserved.
- **Effort**: trivial

---
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-12.md` (finding `TD8-2026-08-12-02`)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan/wgpu objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (prefer a default-suite test, not `#[ignore]`d)

