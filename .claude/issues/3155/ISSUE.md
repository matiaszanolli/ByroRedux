# Issue #3155: UI-D7-02: the unknown-host-method warn is deduplicated per process, not per menu, so the second menu to need a handler is silent

- **Finding ID**: `UI-D7-02`
- **Severity**: LOW
- **Labels**: `low,tech-debt,bug`
- **Source report**: `docs/audits/AUDIT_UI_2026-08-20.md`
- **Filed**: 2026-08-20 (comprehensive 25-audit sweep, `/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3155

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3155 --json state`.

---

- **Severity**: LOW
- **Dimension**: 7 — Engine Wiring & Input Routing
- **Location**: `byroredux/src/app_frame.rs`:246-269 · `byroredux/src/main.rs`:123-125, 444-445 · `crates/ui/src/lib.rs`:104-116
- **Status**: NEW

## Description

The unknown-host-method warning is deduplicated **for the process**, not per menu.

`ui_reported_host_methods: HashSet<String>` lives on `App` (`main.rs`:123) and is
keyed on the bare method name, while the warn text it guards names the menu:

```
"Scaleform menu '{}' called host method '{}'"     // uses ui.menu_name
```

So menu A calling `ShowTutorial` prints once naming A; menu B calling the same
method prints nothing. `UiManager::install_player` does not clear the set either
— it is not visible from `crates/ui`.

The *other* half of this contract is correct and was verified: the set is
deliberately **not** reset per menu load, so it cannot re-spam. Only the key is
too coarse.

## Evidence

```
$ grep -rn "ui_reported_host_methods" byroredux/src/
byroredux/src/app_frame.rs:250:  ) && !self.ui_reported_host_methods.contains(&call.method)
byroredux/src/app_frame.rs:262:  self.ui_reported_host_methods.insert(call.method.clone());
byroredux/src/main.rs:123:      ui_reported_host_methods: std::collections::HashSet<String>,
byroredux/src/main.rs:444:      ui_reported_host_methods: std::collections::HashSet::new(),
```

The inserted key is `call.method`; the printed context is `ui.menu_name`.

## Impact

Diagnostic only, but it degrades precisely the signal M48's Pending-row
host-method-handler work consumes: **"which menus need which handlers" collapses
to "which method was first seen anywhere"**.

## Related

- #2964 — `MAX_DISTINCT_HOST_METHOD_NAMES`, the cap on this same set
- #3147 (UI-D5-02) — until an archive-backed menu can be opened, this signal is
  only reachable via `--swf`

## Suggested Fix

Key the set on `(menu_name, method)` rather than `method`. The
`MAX_DISTINCT_HOST_METHOD_NAMES` cap from #2964 already bounds it, and the
`contains` fast-path means a known pair is never blocked once the set is full.

---
**Source**: `docs/audits/AUDIT_UI_2026-08-20.md` (finding `UI-D7-02`)

## Completeness Checks
- [ ] **SIBLING**: The three other bounded diagnostic sets #2964 introduced checked for the same key-too-coarse shape
- [ ] **TESTS**: A regression test pins this specific fix — two menus, same method name, two warns
