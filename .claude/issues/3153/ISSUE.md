# Issue #3153: UI-D4-03: four new docs/engine/ui.md + ROADMAP.md drifts introduced by the 08-16→08-20 delta (catalog count 138 vs 269, dropped destroy-trait predicate)

- **Finding ID**: `UI-D4-03`
- **Severity**: LOW
- **Labels**: `low,tech-debt,documentation`
- **Source report**: `docs/audits/AUDIT_UI_2026-08-20.md`
- **Filed**: 2026-08-20 (comprehensive 25-audit sweep, `/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3153

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3153 --json state`.

---

- **Severity**: LOW
- **Dimension**: 4 — Catalog Fidelity & Drift (doc rot)
- **Profile**: both
- **Location**: `docs/engine/ui.md`:19, 158, 400, 415 · `ROADMAP.md`:758, 759, 1094
- **Status**: NEW — **distinct points**. `#2971` is OPEN and covers five *other* lines; these four were introduced by the 08-16 → 08-20 delta.

## Description

Four statements that were true on 2026-08-16 and are false at HEAD:

1. **`ui.md`:158** — documents
   `pub fn resource_loads(&self) -> Vec<ScaleformResourceLoad>;`
   It is now `-> &[ScaleformResourceLoad]` (`crates/ui/src/player.rs`:504).
   Changed by #2967.

2. **`ui.md`:400** — "locates the class that declares `BGSCodeObj`,
   `onCodeObjCreate`, **and `onCodeObjDestruction`**". #2963 reduced the predicate
   to the first two (`crates/ui/src/avm2_host.rs`:82-99). This is the *exact*
   sentence that encoded the four-menu bug #2963 fixed; it now documents a
   requirement the code deliberately dropped.

3. **`ui.md`:415** — "dropping `SwfPlayer` invokes the latter, and a private
   acknowledgement increments `code_object_destruction_count()`". Now conditional
   on the class carrying the trait — see #3149.

4. **`ui.md`:19** and **`ROADMAP.md`:758 / 759 / 1094** — "**138**-method
   installed-corpus catalog". Counted at HEAD:

   ```
   SKYRIM_SKYUI_METHODS               74 entries, 12 request
   FALLOUT4_BGS_CODE_OBJECT_METHODS  269 entries, 33 request
   ```

   The `ui.md` *body* (390, 427, 434) was updated by #2966; the status blockquote
   at `:19` and all three ROADMAP mentions were not.

## Evidence

```
$ sed -n '504p' crates/ui/src/player.rs
    pub fn resource_loads(&self) -> &[ScaleformResourceLoad] {

$ grep -c '^\s*ScaleformHostMethod' crates/ui/src/catalog.rs
343            # = 74 (Skyrim) + 269 (FO4); ui.md:19 and ROADMAP still say 138

$ sed -n '19p' docs/engine/ui.md
> `BGSCodeObj` lifecycle, a 138-method installed-corpus catalog, and
```

## Impact

`docs/engine/ui.md` is named by `/audit-ui` as the ground-truth host contract, so
a reader checking the FO4 contract against line 400 would **reintroduce the #2963
bug**. The stale 138 is the same drift class #2730 was filed for.

## Related

- **#2971 (OPEN)** — five other `ui.md` drift points. **Fix these four in the
  same edit pass**; the audit's own recommendation was to fold them in rather
  than open a second issue, and this issue exists only so the four new points are
  tracked somewhere other than the report.
- #2966, #2963, #2967 — the three commits that caused the drift
- #3149 (UI-D3-03) — point 3's live-code counterpart
- #2730 — prior precedent for this drift class

Separately, `docs/engine/ui.md`:39 and `ROADMAP.md`:759 carry a *fifth* drift
(archive-backed menu loading listed under Status when it has no engine caller) —
that one is tracked inside **#3147**, because there the doc rot is load-bearing
rather than cosmetic.

## Suggested Fix

Four one-line edits. Preferably applied to #2971's branch so `ui.md` is corrected
once.

---
**Source**: `docs/audits/AUDIT_UI_2026-08-20.md` (finding `UI-D4-03`)

## Completeness Checks
- [ ] **SIBLING**: Every other statement of the catalog size re-counted, not re-copied — `ui.md`:19/390/427/434 and `ROADMAP.md`:758/759/1094
- [ ] **TESTS**: Consider a source-pin test asserting the documented catalog count matches `SKYRIM_SKYUI_METHODS.len()` / `FALLOUT4_BGS_CODE_OBJECT_METHODS.len()`, so this drift class stops recurring
