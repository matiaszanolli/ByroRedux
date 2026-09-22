# UI-D1-2026-09-21-01: #3434's reserved callback band is keyed on a movie-spoofable prefix, so 32 movie-registered __byro* names still lock the adapter's lifecycle callbacks out

**Issue**: #4719
**Severity**: LOW
**Labels**: low,ui,bug

## Description
`insert_bounded` (`crates/ui/src/host.rs`, #3434's reserved-band guard) admits an over-budget name into a bounded set only when it carries `ENGINE_NAME_PREFIX` ("__byro") AND the count of *already-present* names carrying that prefix is below `RESERVED_HOST_METHOD_NAMES` (32):
```rust
if value.starts_with(ENGINE_NAME_PREFIX)
    && set.iter().filter(|name| name.starts_with(ENGINE_NAME_PREFIX)).count() < RESERVED_HOST_METHOD_NAMES
{ set.insert(value); return; }
```
The count includes **every** prefixed name already in the set, regardless of whether it got there via the normal (pre-cap) insertion path or the reserved-band path. A movie that registers 32 (or more) `__byro`-prefixed callback names of its own — while the set is still under the main 1024 budget, so they insert normally — permanently occupies the entire reserved band. Once the main budget fills, the engine's own later-registered lifecycle names (`__byroBGSAdapterLoaded`, `__byroBGSCodeObjReady`, `__byroBGSCodeObjDestroy`) are refused, because the count is already at 32 from movie-chosen names.

The doc comment at `host.rs:58-62` claims this cannot happen ("[RESERVED_HOST_METHOD_NAMES] exists so it cannot: engine-authored names get their own band and never compete with movie-chosen ones for this budget") — that claim is false for this ordering.

## Evidence
Verified at HEAD `ee6d3fb39`: `crates/ui/src/host.rs`, `insert_bounded` (admission test) and the doc comment above `RESERVED_HOST_METHOD_NAMES`, both unchanged. The report's probe (verbatim copy of `insert_bounded`) reproduces two orderings:
- Case A: 32 spoofed `__byro*` names inserted first, then 992 plain ones (filling the set to 1024) — `__byroBGSAdapterLoaded`, `__byroBGSCodeObjReady`, `__byroBGSCodeObjDestroy` are all refused afterward.
- Case B: 1024 plain names first, then 32 spoofed ones — same result.
The existing #3434 tests (`crates/ui/src/host/tests.rs`) cover plain-name exhaustion and reserved-band boundedness, but not a spoofed band ahead of the engine's own registrations.

## Impact
Only hostile or malformed movie content can trigger the 32-name spoof; when it does, the movie's own readiness probe (`__byroBGSCodeObjReady`) and destroy acknowledgement (`__byroBGSCodeObjDestroy`) are silently disabled — the refusal produces no diagnostic beyond the one-shot cap error, which already fired for the movie's own names. No engine code consumes readiness today, so this is a partial (not full) fix of #3434 rather than a functional regression in current builds — but the doc and #3434's own test suite assert a guarantee the code does not actually provide.

## Related
- #3434 (closed) — the original reserved-band fix; this is a gap in that fix's own guarantee, not a regression of it.

## Suggested Fix
Reserve by exact membership in the compile-time list of engine callback names (`READY_CALLBACK`, `LOADED_CALLBACK`, `DESTROY_CALLBACK`) instead of by prefix-count. That is spoof-proof (a movie cannot register the exact reserved names before the adapter does, since `insert_bounded` already de-dupes by value) and bounded at exactly 3. Add a test that fills the set with 32+ spoofed `__byro*` names before the adapter installs its own, and asserts the three real callbacks still get in.

## Completeness Checks
- [ ] **TESTS**: A spoofed-band regression test (32+ movie-registered `__byro*` names before the adapter's own installer runs) pins the fix
- [ ] **SIBLING**: `known_methods` (the other bounded set noted as taking engine registrations through this same door) gets the same audit

Source: docs/audits/AUDIT_UI_2026-09-21.md (UI-D1-2026-09-21-01)
