# Issue #3151: UI-D3-04: InitCodeObj / ReleaseCodeObj are dropped from the host-call inventory with no justification, and the corpus test cannot see past the exclusion

- **Finding ID**: `UI-D3-04`
- **Severity**: MEDIUM
- **Labels**: `medium,legacy-compat,bug`
- **Source report**: `docs/audits/AUDIT_UI_2026-08-20.md`
- **Filed**: 2026-08-20 (comprehensive 25-audit sweep, `/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3151

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3151 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: 3 — AVM2 Adapter Injection
- **Profile**: `Fallout4Avm2`
- **Location**: `crates/ui/src/avm2_host.rs`:291-293 (the skip), `:255-298` (`referenced_host_methods_in_tags`), `:1362-1372` (the corpus gate)
- **Status**: NEW

## Description

`referenced_host_methods_in_tags` — the #2718 scan whose whole purpose is to
install a forwarder for every `BGSCodeObj.X` call site a movie actually contains
— silently discards two names:

```rust
if matches!(method.as_slice(), b"InitCodeObj" | b"ReleaseCodeObj") {
    continue;
}
```

There is no comment, no issue, no doc reference, and neither name appears in the
269-entry catalog. It landed in `3a02b02d` (the original FO4 feature commit,
pre-#2718) and survived both the #2718 union rewrite and the #2966 regeneration
without being re-examined.

Contrast `OBJECT_PROTOTYPE_MEMBERS` ten lines below (`:308-316`), which carries a
full docstring explaining why shadowing an `Object` member would be wrong.

## Evidence — the exclusion is NOT vacuous: both names are in shipped content

`/audit-fo4`'s 2026-08-20 pass resolved this conclusively (`AUDIT_FO4_2026-08-20.md`,
Lead D). `Fallout4 - Interface.ba2` is **BTDX v8 GNRL, 1,101 entries** — 548
`.dds`, **311 `.swf`**, 184 `.png`, **4 `.gfx`**, 16 `.txt`, 5 `.xml`, 33 string
tables. All 311 SWFs are `CWS` (zlib) and all 4 GFX are `CFX`; every one inflates
cleanly with a plain zlib pass over `bytes[8..]`.

Scanning all 315 decompressed movies:

- **54** name `BGSCodeObj`.
- **8** name `InitCodeObj` **and** `ReleaseCodeObj`, every one of them also
  naming `BGSCodeObj` in the same ABC constant pool:
  `interface\pipboymenu.swf`, `pipboy_datapage.swf`, `pipboy_invpage.swf`,
  `pipboy_mappage.swf`, `pipboy_radiopage.swf`, `pipboy_statspage.swf`,
  `examinemenu.swf`, `examineconfirmmenu.swf`
  — i.e. the entire Pip-Boy family plus the two examine menus.

In the Pip-Boy family the three strings appear as a contiguous constant-pool run
`PipboySubMenu · BGSCodeObj · InitCodeObj · ReleaseCodeObj · codeObj`, which is
what a `BGSCodeObj.InitCodeObj(...)` / `.ReleaseCodeObj(...)` call pair
serialises to (the pool is emitted in first-use order).

**Honesty bound**: this is a **constant-pool measurement, not an ABC opcode
walk**. `referenced_host_methods_in_tags` matches a `GetLex` / `GetProperty` /
`FindPropStrict` whose multiname local name is `BGSCodeObj` followed by a call,
so proving the skip is *live* would need multiname resolution that has not been
implemented. What is settled is that both excluded names are present in shipped
content, in the same movies as the receiver they would be called on, and those
movies are the entire Pip-Boy.

*(The original audit carried an "I could not confirm whether either name appears
in shipped content" caveat, on the strength of a raw byte scan of the BA2
returning 0 hits even for the control string `BGSCodeObj`. That 0 was **BA2 zlib
plus SWF `CWS` zlib double-compression**, not a capability limit. The caveat is
struck.)*

### The gate that should catch this structurally cannot

Because the exclusion lives *inside* `referenced_host_methods`, the corpus gate
`installed_fallout4_host_calls_are_all_forwarded` (`avm2_host.rs`:1362-1372)
consumes the already-filtered set. If a shipped menu calls either name,
`uncataloged` is still empty and the assertion is still green. **The test asserts
on the output of the thing it is meant to audit.**

Catalogued as instance **#10** in the "verification layer is green by
construction" table of `docs/audits/AUDIT_SUITE_SUMMARY_2026-08-20.md`.

## Impact

If any of the 8 identified movies calls `BGSCodeObj.InitCodeObj(...)` at an ABC
call site, no forwarder is installed, the property resolves to `undefined` on a
dynamic object, and AVM2 raises **Error #1006** — aborting the executing frame
handler, so the menu renders and then stops responding. That is precisely the
failure mode #2718 exists to remove, re-armed for two names, and structurally
invisible to the gate supposed to catch it. The blast radius is the Pip-Boy.

## Related

- #2718 — the union rewrite this exclusion survived
- #2966 — the catalog regeneration this exclusion survived
- `AUDIT_FO4_2026-08-20.md` Lead D — the corpus measurement above
- `docs/audits/AUDIT_SUITE_SUMMARY_2026-08-20.md` — green-by-construction #10

## Suggested Fix

Either document why the two names are not host methods (with the same rigour as
`OBJECT_PROTOTYPE_MEMBERS`), or lift the skip to a named `const` applied
**outside** `referenced_host_methods` so the corpus sweep can report when a real
menu names one.

Independently worth doing given the measurement above: complete the ABC opcode
walk (multiname resolution) on the 8 named movies to settle whether the constant
pool entries correspond to live `BGSCodeObj.InitCodeObj` / `.ReleaseCodeObj` call
sites or to unrelated declarations.

---
**Source**: `docs/audits/AUDIT_UI_2026-08-20.md` (finding `UI-D3-04`), amended with `docs/audits/AUDIT_FO4_2026-08-20.md` Lead D

## Completeness Checks
- [ ] **SIBLING**: Any other filter applied *inside* a scan whose output a corpus gate consumes — same shape, same blindness
- [ ] **TESTS**: A regression test pins this specific fix — one that can go RED when a shipped menu names an excluded method
