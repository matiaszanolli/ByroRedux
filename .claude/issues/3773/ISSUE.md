# #3773 — UI-D4-2026-08-30-03: the FO4 catalog's Command/Request kind is partly heuristic-derived and partly evidence-derived, and the array records which is which nowhere

**Repo**: matiaszanolli/ByroRedux · **Filed**: 2026-08-30 · **HEAD**: `64f64480`
**Labels**: low, ui, game:fo4, tech-debt, bug

---

**Audit**: `/audit-ui` — `docs/audits/AUDIT_UI_2026-08-30.md` (Dimension 4 — Catalog Fidelity & Drift), HEAD `64f64480`
**Finding ID**: `UI-D4-2026-08-30-03`

- **Severity**: LOW
- **Status**: NEW
- **Profile**: `Fallout4Avm2`

## Location

- `crates/ui/src/catalog.rs` — the FO4 entry array
- `docs/engine/ui.md:441-451` — the provenance statement (`:446` names the heuristic)

## Description

`docs/engine/ui.md:441-451` states the provenance plainly: of the 269 entries, 138 came from the F4CF reconstruction plus installed-ABC inventory, and the **131 added by the #2966 311-movie sweep have their `kind` "inferred from name prefix (`Get*`/`Is*`/`Should*`/`Can*`/`get*`)"**.

The array itself carries **no marker distinguishing the two provenances**, and the doc does not list which 131.

The heuristic's boundary is demonstrably imprecise. All 33 `Request` entries match the prefix set — i.e. **no entry was ever promoted to `Request` on evidence that contradicted the prefix rule** — while the `Command` side needed at least two hand-overrides (`Cancel`, `CancelPlayback`, both matching `^Can`) and still holds 16 names carrying query-shaped verbs the prefix set does not cover:

```
AreModsLoaded                CheckHardcoreModeFastTravel   CheckRequirements
DoQuicksave                  ValidateHackingWord           requestCredits
RequestAudioOptions          RequestDisplayOptions         RequestGameplayOptions
RequestHelpText              RequestHelpTitle              RequestInputMappings
RequestInstalledContentText  RequestInstalledContentTitle
RequestRefreshInstallProgress                              requestLoadingText
```

The audit does **not** claim these are misclassified. Several — the `Request*` family in particular — are very plausibly correct, because Bethesda settings menus use a request→native→push-back pattern rather than a return value, and there was no F4CF checkout or installed FO4 corpus available to decide it.

*That* is the finding: **the catalog gives a maintainer no way to tell a `Command` that was measured from one a prefix rule guessed**, so all 236 have to be re-derived from scratch before a handler can be trusted against any of them.

## Why LOW rather than the "hanging callback" the checklist warns about

That failure mode is Skyrim's. On FO4 there is no callback protocol — the injected forwarder does `ExternalInterface.call(...)` through `Function.apply` and returns synchronously, and `record_call` returns `ExternalValue::Null` for both `Queued` and `MissingResponse` (`host.rs:483-487`). `request_id` is always `None` on the AVM2 normalization path (`host.rs:532-547`), so `GameDelegateResponse` is unreachable for this profile.

The kind therefore decides **only** whether a method lands in `unanswered_methods()` and which dispatch label it logs — no menu hangs either way. What it costs is the completeness of the to-do list the handler work will be driven from.

## Evidence

Re-verified at HEAD: `grep -n 'inferred from name prefix' docs/engine/ui.md` → `:446`. The catalog array carries no provenance field.

## Related

- #2966 (the 311-movie sweep that added the 131 heuristic entries)
- #3103 (asks for a corpus measurement of the sibling **Skyrim** catalog — this could fold into it)

## Suggested Fix

Mark the 131 sweep-derived entries (the #2966 measurement presumably still has the list) with a third field or a delimited comment block, so `unanswered_methods()` consumers know the classification's confidence.

Alternatively fold this into **#3103**, which already asks for a corpus measurement of the sibling Skyrim catalog.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — the AVM1/Skyrim half of `catalog.rs` (#3103's scope)
- [ ] **TESTS**: A regression test pins this specific fix — if a provenance field is added, the corpus test should assert every entry carries one
