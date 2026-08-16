# UI-D5-01: NavigatorState.loads is push-only for the life of the player — no cap, no dedup, no clear on menu swap

**Issue**: #2967
**Severity**: MEDIUM
**Dimension**: Resource Navigator
**Labels**: `medium,memory,bug`
**Source report**: `docs/audits/AUDIT_UI_2026-08-16.md`
**Filed**: 2026-08-16 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_UI_2026-08-16.md` (Dimension 5 — Resource Navigator). Profile: both.

**Location**: `crates/ui/src/navigator.rs`:81, 117-119, 245-250

## Description

Every successful archive fetch pushes a `ScaleformResourceLoad` (two `String`s, a `usize`, a `bool`) onto `NavigatorState.loads`. The only reader is `ScaleformNavigatorRuntime::loads`, which **clones** the vector rather than draining it.

Nothing caps it, nothing deduplicates repeated fetches of the same path, and nothing clears it when a menu is replaced.

The contrast is inside the same struct: the sibling `errors` field got draining (`take_errors`), dedup and a 64-entry cap under #2720. `loads` got none of the three.

## Evidence

```rust
// crates/ui/src/navigator.rs:245 — the only mutation of `loads`
state.borrow_mut().loads.push(ScaleformResourceLoad {
    request_url, archive_path, byte_len: body.len(), import_preload_rewritten,
});
```

```rust
// crates/ui/src/navigator.rs:117 — the only reader, and it clones
pub(crate) fn loads(&self) -> Vec<ScaleformResourceLoad> {
    self.state.borrow().loads.clone()
}
```

## Impact

`fetch` serves `ImportAssets` **and** every runtime `loadMovie`/`URLLoader` a menu issues. A menu that reloads an asset on a timer or per frame grows engine-resident heap monotonically, with the growth rate set by untrusted content.

Even without a hostile movie, `resource_loads()` is advertised as the observability channel for "what did this menu request" and becomes progressively more expensive to read (a full clone) the longer the menu is open.

## Suggested Fix

Cap `loads` the way `resource_errors` is capped, or deduplicate by `archive_path` with a hit counter. Either keeps the diagnostic useful while making the memory bounded by the archive's file count instead of by the movie's behaviour.

Dedup-with-counter is the better fit here: repeated fetches of the same path are the expected shape, and a hit count is more informative than N identical entries.

## Related

- UI-D2-01 — same unbounded-diagnostics class, different channel. Worth fixing together.
- #2720 (`errors` got drain + dedup + cap) — the sibling treatment `loads` should mirror.

## Completeness Checks
- [ ] **SIBLING**: Every other `Vec`/`Set` on `NavigatorState` checked for the same push-only shape
- [ ] **CLEAR-ON-SWAP**: `loads` cleared or reset when a menu is replaced, not only capped
- [ ] **READER-COST**: The reader drains or borrows rather than cloning the whole vector each call
- [ ] **TESTS**: A regression test fetches the same path N times past the cap and asserts bounded growth

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2967 --json state` when live state is needed.*
