# #7257 PSR Phase 2: live reload against the current plugin set, preflight before teardown, acceptance

State: OPEN
Labels: enhancement, save-load

Part of #7258 (PRD [§9 Phase 2](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/portable-save-references.md#9-delivery-sequence)). Depends on Phase 1.

## Acceptance Criteria
- [ ] PSR-LOAD-001: live load (`byroredux/src/save_io/live_load.rs`) rebuilds the saved location against the current complete plugin set, not the snapshot's plugin context.
- [ ] PSR-LOAD-005 / 006: conversion is preflighted before teardown; a refused load leaves the live session and its `LiveReloadLedgers` untouched, and the refusal reaches the player through `notify_player` with the PSR-LOAD-004 text.
- [ ] PRD §7.2 scenarios 1–3 pass headless against the live reload path.
- [ ] Scenario 4, owner-run on Oblivion: save, add a plugin, reload, and confirm with `byro-dbg` that `GetAV Sneak` and the base record of an inventory stack whose plugin's slot moved are unchanged.
- [ ] Scenario 5: the touched crates' tests and `python3 scripts/check-prd-conformance.py` pass; the owner-run save and incremental audits report no unresolved blocking findings.
- [ ] PRD §8: a "Changed-order load" entry is added to each in-scope game's row of the ROADMAP compatibility matrix, and the catalog is linked from the save design.


## Comments
