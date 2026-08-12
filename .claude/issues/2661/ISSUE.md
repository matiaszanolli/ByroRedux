# #2661: SCR-D6-NEW11-04: ALCS collection aliases are not excluded from the single-entity fill loop -- a collection alias binds one candidate and receives the whole collection's injected data

**Severity**: MEDIUM
**Dimension**: Scripting Runtime Systems (Dimension 6)
**Untrusted-Input**: No
**Location**: `crates/scripting/src/scene.rs` -- the alias fill loop in `resolve_alias_bindings`; `ALCS`/`ALMI` decoded at `crates/plugin/src/esm/records/misc/quest.rs:712,722`
**Status**: NEW

## Description

`docs/engine/m47-3-quest-alias-design.md` lists reference-collection aliases as a Phase 4+ deferral -- i.e. they should **decline** and diagnose as unavailable until the collection runtime exists.

Instead, an `ALCS` collection alias carrying match conditions falls through the ordinary single-entity fill path: it binds **exactly one** candidate, and that one entity receives the whole collection's injected factions and inventory. It also diagnoses as `Bound` rather than `ReferenceCollectionRuntimeUnavailable`, so the observability added by `0775df28` reports success.

`ALMI` (the collection fill limit) is parsed at `quest.rs:722` and never read by any consumer.

## Evidence

Probe (temporary test, run, reverted) confirms all three: the single binding, the injection application to that one entity, and the `Bound` diagnostic string.

`ALCS` and `ALMI` are decoded (`crates/plugin/src/esm/records/misc/quest.rs:712`, `:722`) with the parser's own comment at `:726` noting FO4 collection aliases are exactly `ALCS` + `ALMI` -- so the data reaches the runtime; nothing filters on it.

## Impact

Contradicts the design doc's own deferral: a documented "not built yet" path silently half-works instead of declining. This is the decline invariant's failure mode applied to the alias runtime -- an unfilled alias is safe and diagnosable, a wrongly-filled one is neither.

Concretely: one arbitrary member of a reference collection gets faction membership and inventory intended for the whole set, and the diagnostics say the alias filled correctly.

## Related

`docs/engine/m47-3-quest-alias-design.md` section "Remaining subsystem boundary"; `0775df28` (the diagnostics that report the false success)

## Suggested Fix

Detect `ALCS` in the fill loop and decline with the `ReferenceCollectionRuntimeUnavailable` diagnostic the design doc already specifies, until the Phase 4+ collection runtime exists. Either read `ALMI` or drop it explicitly with a comment so it is not mistaken for wired data.

## Completeness Checks
- [ ] **DECLINE-INVARIANT**: The recognizer still declines on every unmodeled term -- a partial lowering is worse than none
- [ ] **SIBLING**: Same pattern checked in related files (other primitives, other parsers, other spawn paths)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-12.md` (eleventh scripting-domain pass, 7 dimension agents).*
