# UI-D3-02: no idempotency guard on adapter injection — a second pass would double-install the bootstrap and duplicate the adapter script

**Issue**: #2970
**Severity**: LOW
**Dimension**: AVM2 Adapter Injection
**Labels**: `low,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_UI_2026-08-16.md`
**Filed**: 2026-08-16 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_UI_2026-08-16.md` (Dimension 3 — AVM2 Adapter Injection). Profile: `Fallout4Avm2`.

**Location**: `crates/ui/src/avm2_host.rs`:32, 45-163, 439-479

## Description

`ADAPTER_NAME` (`"byroredux.fallout4.BGSCodeObj"`) is written as the injected `DoAbc2` tag's name and **never read back**. A workspace grep finds exactly two occurrences: the `const` and the single write site.

`inject_host_object_adapter` has no check for an already-present adapter tag, and `patch_root_constructor` has no check for an already-present `__byro_fallout4_install` call — it simply finds the first `InitProperty` / `SetProperty` naming `BGSCodeObj` and splices in front of it again.

Feeding already-patched bytes back through would therefore emit a second adapter script with the same trait names into the same domain and call the installer twice (re-firing `onCodeObjCreate`).

## Evidence

```
$ grep -n "ADAPTER_NAME" crates/ui/src/*.rs
crates/ui/src/avm2_host.rs:32:const ADAPTER_NAME: &str = "byroredux.fallout4.BGSCodeObj";
crates/ui/src/avm2_host.rs:154:        name: SwfStr::from_utf8_str(ADAPTER_NAME),
```

Definition and write. No read.

## Impact

**No live double-injection path exists today** — each of the three `SwfPlayer` constructors calls `inject_host_object_adapter` exactly once on bytes freshly read from disk or archive, and a menu reload re-reads the original. This is a **latent hazard, not a live bug**, which is why it is LOW.

It becomes live the moment anything caches patched bytes (an obvious response to UI-D1-01) or rebuilds a player from its current movie on resize.

## Suggested Fix

Early-return `AdapterInjected` when a `DoAbc2` tag already carries `ADAPTER_NAME`, and reject a constructor that already contains a `__byro_fallout4_install` call site. Both are cheap scans over data already in hand.

This is worth landing **before** UI-D1-01's caching fix, not after.

## Related

- UI-D1-01 — the natural fix for that finding is exactly the change that would make this reachable

## Completeness Checks
- [ ] **SIBLING**: `patch_root_constructor` guarded too, not just the `DoAbc2` tag scan
- [ ] **ORDERING**: Landed before or with UI-D1-01's byte caching
- [ ] **TESTS**: A regression test feeds already-patched bytes back through and asserts a single adapter and one installer call

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2970 --json state` when live state is needed.*
