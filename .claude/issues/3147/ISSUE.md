# Issue #3147: UI-D5-02: the archive-backed menu-load path has no engine consumer — the shipped binary cannot open any vanilla Bethesda menu

- **Finding ID**: `UI-D5-02`
- **Severity**: HIGH
- **Labels**: `high,legacy-compat,bug`
- **Source report**: `docs/audits/AUDIT_UI_2026-08-20.md`
- **Filed**: 2026-08-20 (comprehensive 25-audit sweep, `/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3147

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3147 --json state`.

---

- **Severity**: HIGH
- **Dimension**: 5 — Resource Navigator
- **Profile**: both (`SkyrimAvm1`, `Fallout4Avm2`)
- **Location**: `crates/ui/src/lib.rs`:86-102 · `crates/ui/src/player.rs`:203-266 · `crates/ui/src/navigator.rs` (all 911 lines) · `byroredux/src/scene.rs`:1330-1356
- **Status**: NEW

## Description

`UiManager::load_swf_from_resource_provider`, `SwfPlayer::from_resource_provider`,
the whole `navigator.rs` module and the `ScaleformResourceProvider` impls for
`BsaArchive` / `Ba2Archive` are reachable **only from tests**.

The single engine construction site is `byroredux/src/scene.rs`:1330, behind
`--swf`, and it does `std::fs::read(swf_path)` → `UiManager::load_swf` →
`SwfPlayer::new`, which passes `navigator: None`.

Every vanilla Bethesda menu ships inside a BSA/BA2. **No vanilla menu can be
opened by the engine as built.**

## Evidence

```
$ grep -rn "load_swf_from_resource_provider\|load_swf_with_profile\|\
  UiManager::close\|set_input_focus\|invoke_callback" byroredux/src/
(no output)

$ grep -rn "UiManager::new\|load_swf" byroredux/src/
byroredux/src/scene.rs:1333:   let mut ui = UiManager::new(w, h);
byroredux/src/scene.rs:1334:   match ui.load_swf(&swf_data, swf_path) {

$ grep -rn "from_resource_provider" byroredux/ crates/ui/src/
crates/ui/src/lib.rs:86      (definition)
crates/ui/src/player.rs:203  (definition)
crates/ui/src/navigator.rs:775, 819, 878   (tests only)
```

`player.rs`:288-303 — `navigator_runtime` is `Some` only when `from_movie` is
handed a navigator, which only `from_resource_provider` does.

### The documentation asserts the opposite — and that doc rot is part of the defect

- `docs/engine/ui.md`:39 lists **"BSA/BA2-relative `ImportAssets` loading"** in
  the **Status** row. Its Pending row (`:40` — "Host-method behavior, remaining
  GFx stubs, Papyrus↔UI bridge, menu-stack/focus policy, font fidelity, full
  menu pack") does **not** mention archive-backed menu loading at all.
- `ROADMAP.md`:759 (M48) likewise presents the navigator as delivered — "The
  archive-backed navigator resolves relative resources through BSA/BA2 providers
  and pumps Ruffle's local executor" — and lists only "method behavior and
  `_global.gfx` stubs, font fidelity/menu lifecycle, and Papyrus/ECS ↔ UI
  callbacks" as Remaining work.

Both statements are true of the *code* and false of the *engine*. **This is why
nobody noticed that 911 lines of `navigator.rs` have no caller**: the two places
a reader would check to find out both report the capability as shipped. The doc
correction is not cosmetic follow-up — it is the half of the defect that made
the other half survive four audit passes.

## Impact

The entire Dimension-5 investment — #2720's degrade-to-placeholder path, #2734's
lazy futures, #2967's dedup/cap, the URL confinement — is dead code in the
shipped binary. Consequently every runtime claim about vanilla menus rests on
`#[ignore]`d or self-skipping tests.

The obvious workaround (hand-extract a `.swf` and pass `--swf`) is fragile in
exactly the way the severity model means: `hudmenu.swf` imports `fonts_en.swf`,
and the loose path has **no navigator at all**, so the import cannot resolve by
any means.

This is the same class as #2714 ("host bridge has no engine consumer"), which
was accepted, filed and fixed.

Adjacent, from the same root: `UiManager::close()` and `set_input_focus()` exist
and are correct but have no engine caller either, so there is no runtime way to
close a Scaleform menu today. Menu-stack/focus policy needs a load/close entry
point before it needs a policy.

## Related

- #2714 — the accepted precedent (host bridge had no engine consumer)
- #2968 — redundant parses, on this same unreached path
- #2963 — its impact paragraph assumed this path was live; it is not
- #3103 — Skyrim/AVM1 corpus measurement
- #2723 — `UiManager::close` is dead (the close half of the same absence)

## Suggested Fix

Add an archive-backed launch route — e.g. `--menu interface\hudmenu.swf`
resolving through the existing `asset_provider::GameArchive` — so
`load_swf_from_resource_provider` has one real caller.

Until then, correct `docs/engine/ui.md`:39 and `ROADMAP.md`:759 to place
archive-backed *menu* loading in **Pending**, leaving only the already-true
`ImportAssets` claim in Status.

---
**Source**: `docs/audits/AUDIT_UI_2026-08-20.md` (finding `UI-D5-02`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — `UiManager::close` / `set_input_focus` / `invoke_callback` are in the same "implemented, tested, uncalled" bucket
- [ ] **DOCS**: `docs/engine/ui.md`:39/40 and `ROADMAP.md`:759 moved to Pending in the same change that lands (or defers) the caller
- [ ] **TESTS**: A regression test pins this specific fix — prefer a default-suite test asserting the engine-side route exists, not an `#[ignore]`d archive test
