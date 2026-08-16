# UI-D4-01: the Fallout 4 catalog recognises 93 of the 224 methods shipped menus actually call, and 45 of its 138 entries are called by nothing

**Issue**: #2966
**Severity**: MEDIUM
**Dimension**: Catalog Fidelity & Drift
**Labels**: `medium,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_UI_2026-08-16.md`
**Filed**: 2026-08-16 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_UI_2026-08-16.md` (Dimension 4 — Catalog Fidelity & Drift). Profile: `Fallout4Avm2`.

**Location**: `crates/ui/src/catalog.rs`:192-331 · `crates/ui/src/avm2_host.rs`:129-138, 307-317

## Description

**Measured, not inferred.** Running `installed_fallout4_host_calls_are_all_forwarded` against `Fallout4 - Interface.ba2` on 2026-08-16 reports **131 distinct `BGSCodeObj` methods called by the 311-movie corpus that are outside the 138-entry catalog**, and **45 catalog entries referenced by no menu in the corpus**.

So the real surface is 93 cataloged + 131 uncataloged = **224 methods, of which the catalog covers ~42%**.

Since #2718 an uncataloged method still gets a forwarder, so this is no longer a crash — but a forwarder built from the per-movie scan carries **no `ScaleformHostMethodKind`**. `record_call`'s classifier can only reach `MissingResponse` through `catalog_method.is_some_and(|m| m.kind == Request)`, so every one of those 131 methods is permanently `Unknown`, and the Command-vs-Request distinction — the thing that decides whether a menu waits for a response — is unavailable for 58% of the live surface.

Many of the 131 are unambiguously queries: `GetHasSavedGames`, `IsDLCReady`, `IsMainMenuReady`, `GetDetailColorCount`, `GetFeatureData`, `getSaveData`, `getTextReplaceValue`.

## Evidence

Verbatim from the sweep:

```
note: 131 BGSCodeObj method(s) called by the 311-movie corpus are outside the
138-entry catalog. These are forwarded (and land as
`ScaleformHostDispatch::Unknown`) since #2718, but carry no catalog `kind`
note: 45 of 138 catalog entries are unreferenced by the whole 311-movie corpus
```

`docs/engine/ui.md` describes this array as an "installed-corpus catalog" and as "138 installed-corpus methods", and cites a bytecode-inventory test that proves coverage for **three** representative movies. The 311-movie sweep is the first measurement of the whole corpus and it contradicts the implication.

## Impact

Not a crash — every call still forwards and still lands in the bridge. But:

- The catalog is the only place a method's response contract can be declared, so as engine handlers land they can only be wired for 42% of the real surface without first extending the table.
- `unanswered_methods()`, the diagnostic that is supposed to tell whoever lands a handler "this menu is waiting on you", is **structurally unreachable** for the majority of methods.
- The 45 dead entries are the mirror image: they cost a forwarder in every patched menu and represent transcription from a reconstruction rather than from the shipped bytecode.

## Suggested Fix

Regenerate the FO4 array from the corpus sweep's own inventory (the sweep already computes both directions), classifying by name prefix as a first pass (`Get*` / `Is*` / `Should*` / `Can*` / `get*` → `Request`).

Separately, let a per-movie uncataloged forwarder carry an inferred `kind` so `MissingResponse` becomes reachable for it.

Update `docs/engine/ui.md`'s characterisation of the array at the same time (see UI-D4-02 for the rest of that doc's drift).

## Related

- UI-D3-01. Both are "the FO4 host model was derived from a reconstruction, and the shipped corpus disagrees".
- UI-D2-02 — the AVM1 fix may want to gate on catalog `kind`, which only works if this gap closes.
- #2718 (uncataloged methods forward instead of crashing) — the change this finding builds on.

## Completeness Checks
- [ ] **SIBLING**: The Skyrim/AVM1 catalog measured the same way, not assumed complete
- [ ] **MEASURED**: New entries come from the corpus sweep, not from a reconstruction
- [ ] **DEAD-ENTRIES**: The 45 unreferenced entries removed or justified (DLC/mod surface is a valid justification — state it)
- [ ] **DOCS**: `docs/engine/ui.md`'s "installed-corpus catalog" wording matches the measured numbers
- [ ] **TESTS**: The sweep's two-directional counts become an asserted bound, not just a `note:`

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2966 --json state` when live state is needed.*
