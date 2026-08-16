# UI-D2-02: the AVM1 request-ID heuristic strips a leading integral Number from every AVM1 host call, not only GameDelegate ones

**Issue**: #2965
**Severity**: MEDIUM
**Dimension**: Host Bridge Transport
**Labels**: `medium,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_UI_2026-08-16.md`
**Filed**: 2026-08-16 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_UI_2026-08-16.md` (Dimension 2 — Host Bridge Transport). Profile: `SkyrimAvm1`.

**Location**: `crates/ui/src/host.rs`:415-431, 458-464

## Description

`normalize_call` treats the first argument of any `SkyrimAvm1` call as a `GameDelegate` request ID whenever it is a finite, non-negative, integral `Number`. There is no marker distinguishing "this call came through `GameDelegate.call`" from "this movie called `ExternalInterface.call` directly" — the heuristic is the only evidence used.

When it fires on a non-`GameDelegate` call, the first real argument is **silently removed** from `ScaleformHostCall::arguments` and a bogus `request_id` is attached.

`SkyrimAvm1` is also the fallback profile for *every* non-AS3 movie (`ScaleformProfile::from_movie`), not only Skyrim ones, so the heuristic applies to loose demo SWFs and any third-party AVM1 content too.

## Evidence

The crate's own test pins the behaviour on an **uncataloged** method, where the `GameDelegate` premise cannot hold:

```rust
// crates/ui/src/host/tests.rs:221 — "UnmappedMethod" with a single arg 3
bridge.record_call("UnmappedMethod", &[ExternalValue::from(3_i32)]);
...
assert_eq!(calls[2].dispatch, ScaleformHostDispatch::Unknown);
```

and `calls[0]` (`PlaySound`, args `[1, "UIMenuOK"]`) records `request_id: Some(1)`, `arguments: ["UIMenuOK"]` — the leading `1` is gone from the argument list in both cases.

## Impact

Deferred but structural. Today no responses are registered (the documented *Pending* row), so `callback_response` is always `None` and the wrong `request_id` never reaches ActionScript.

The moment the first engine handler lands, a mis-normalised call delivers the handler an argument list **short by one** *and* re-enters `respond` with an ID that is really a data value — and `GameDelegate.as` looks up `_callbacks[id]`, so a wrong ID is an AS-side error, not a no-op. **The failure is invisible from either side.**

## Suggested Fix

Only apply the request-ID strip when `catalog.find(method)` returns a `Request` entry, or when the movie has registered a `respond` callback (`has_callback("respond")`) — both are already available at the call site.

Record the un-stripped argument list either way so the raw transport payload is never lost.

## Related

- UI-D4-01 — the same "the catalog is the only signal" weakness on the FO4 side. Note the interaction: gating on `catalog.find(method) == Request` is only as good as the catalog, so these two should be fixed with each other in mind.

## Completeness Checks
- [ ] **SIBLING**: The `Fallout4Avm2` normalize path checked for an equivalent implicit-argument assumption
- [ ] **LOSSLESS**: Raw pre-normalisation arguments retained regardless of which branch fires
- [ ] **CATALOG-COUPLING**: If the fix gates on catalog `kind`, UI-D4-01's coverage gap is accounted for
- [ ] **TESTS**: A regression test asserts a non-`GameDelegate` AVM1 call with a leading integer keeps all its arguments and gets no `request_id`

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2965 --json state` when live state is needed.*
