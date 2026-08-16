# UI-D4-02: docs/engine/ui.md has drifted from the crate on five verifiable points

**Issue**: #2971
**Severity**: LOW
**Dimension**: Catalog Fidelity & Drift (doc rot)
**Labels**: `low,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_UI_2026-08-16.md`
**Filed**: 2026-08-16 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_UI_2026-08-16.md` (Dimension 4 — Catalog Fidelity & Drift, doc rot).

**Location**: `docs/engine/ui.md`:109, 147, 159, 205-206, 225, 490, 527

## Description

The catalog **numbers** in the doc are correct (verified during the audit); its API and wiring descriptions are not. Five concrete mismatches:

1. **Lines 147 and 225** declare `provider: Rc<dyn ScaleformResourceProvider>`. The trait has been `Send + Sync` and the parameter `Arc` since #2734 (`crates/ui/src/navigator.rs`:95, 137; `crates/ui/src/player.rs`:189).
2. **Line 490**: "26 default tests plus three ignored installed-corpus smokes." Actual, measured 2026-08-16 via `cargo test -p byroredux-ui`: **36 passed, 2 ignored**.
3. **Line 109** (`byroredux::main → texture_registry.update_rgba`) and **line 527** ("the main loop now drains it once per frame … `byroredux/src/main.rs`"). The per-frame UI block moved to `byroredux/src/app_frame.rs` under #2731.
4. **The `SwfPlayer` struct block (lines 134-160)** omits `host_bridge`, `uploaded_once`, `resource_errors`, `resource_errors_capped`, `preload_stall_frames` and `preload_stalled`; the API list omits `resource_errors()`, `preload_stalled()`, `invoke_callback()`, `profile()` and `host_bridge()`.
5. **Lines 205-206** describe `render()` as returning the buffer whenever it rendered. Since #2719 the decision is made on **content**, not on the render having happened.

## Impact

`docs/engine/ui.md` is the named ground truth for the `/audit-ui` skill and for whoever lands the host handlers.

Item 1 in particular sends a reader looking for a single-threaded provider when the whole point of #2734 was to make it shareable with a worker.

## Suggested Fix

Refresh the four code blocks and the test count; replace the two `byroredux/src/main.rs` references with `byroredux/src/app_frame.rs`.

While in the file, fix the "installed-corpus catalog" / "138 installed-corpus methods" characterisation flagged by UI-D4-01 — the 311-movie sweep contradicts the implication of completeness.

## Related

- UI-D4-01 (the catalog's own coverage claim in the same doc)
- #2729 (the same M48 doc-lag class, in `ROADMAP.md`)
- #2731 (the `main.rs` → `app_frame.rs` split that stranded items 3)

## Completeness Checks
- [ ] **SIBLING**: `ROADMAP.md` and `docs/feature-matrix.md` UI rows checked for the same lag
- [ ] **PATH-GATE**: `.claude/commands/_audit-validate.sh` still passes after the edits
- [ ] **ALL-FIVE**: Every one of the five mismatches addressed, not just the `Rc`/`Arc` one

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2971 --json state` when live state is needed.*
