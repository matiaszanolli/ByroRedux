# #5907: PAR-D5-2026-10-15-03: 26.5% of vanilla FO4 BGSM chains run through the self-referential `defaulttemplate_wet.bgsm` and are never cached, so every resolve re-extracts and re-parses about 3.6 files

- **Labels**: low,bug,nifal,performance,game:fo4
- **Filed from**: `docs/audits/AUDIT_PARSERS_2026-10-15.md`

**Full title**: 26.5% of vanilla FO4 BGSM chains run through the self-referential `defaulttemplate_wet.bgsm` and are never cached, so every resolve re-extracts and re-parses about 3.6 files. Since #5010, every one of those re-parses also re-fires the per-file diagnostics hook

- **Severity**: LOW
- **Dimension**: Decode-Consumer Wiring
- **Location**:
  - `crates/bgsm/src/template.rs:248`: `let key = path.to_ascii_lowercase()`. This is the raw spelling, not normalised.
  - `:273`: `resolver.parsed(&key, &diagnostics)`.
  - `:280`: `let parent_key = pp.to_ascii_lowercase()`, compared against `visited`.
  - `:307`: `if !truncated { self.insert(..) }`. The truncation propagates to every ancestor.
  - Consumer: `byroredux/src/asset_provider/material/provider.rs:395-500` (`resolve_bgsm`). Callers are `merge.rs:1214` (per mesh merge) and `cell_loader/refr.rs:294` (per REFR swap).
- **Status**: NEW. The never-cache rule is #2701 (closed). The re-report is a side effect of 746e0858e (#5010). A search for "defaulttemplate_wet" and "truncated chain cached BGSM" across all issues found nothing.
- **Trigger Input**: vanilla FO4 `materials\template\defaulttemplate_wet.bgsm`, whose `root_material_path` is `template/defaulttemplate_wet.bgsm` (itself). Any mod template chain that ends in a self-reference behaves the same way.
- **Description**:
  - #2701 stopped caching cycle-truncated chains, because where a *multi-node* cycle breaks depends on the entry node.
  - A self-loop breaks at the same place from every entry, but it is treated the same way. The only cycle in vanilla FO4 is that self-loop.
  - So every leaf whose chain reaches it is never cached, and neither is any template above it, because `truncated` propagates upward. Every `resolve_bgsm` repeats the archive extract, inflate and parse of the whole chain.
  - The cycle check compares the raw `root_material_path` spelling against the normalised leaf key the provider passes in, so the self-loop is detected one level late. `defaulttemplate_wet.bgsm` appears twice in a leaf's chain, and three times when it is resolved directly. That is harmless to the merge (`walk()` is first-Some-wins on identical fields) but costs an extra extract and parse each time.
  - #5010's `parsed` hook documents "Cache hits are not re-reported". On these chains nothing is ever a cache hit, so a mod template with drift or lossy strings on such a chain warns again on every mesh merge that touches it, rather than once per file.
- **Evidence**: probe `probe-archives`, real `TemplateCache` plus a resolver that normalises like `ArchiveReader`, run over all 6,616 vanilla FO4 BGSMs:
  ```
  bgsm files 6616 lossy 0 unconsumed 0
  resolve errs 3; leaves re-parsed on a repeat resolve 1753; parsed() calls on repeats 6291
  self-loop anchors 1 (chains 1753), multi-node cycles 0
    1753 materials\template\defaulttemplate_wet.bgsm
  materials\setdressing\greebs\greebs02alpha.bgsm: chain len 4 parsed [leaf, "template/vehiclebustemplate_wet.bgsm",
    "template/defaulttemplate_wet.bgsm", "template/defaulttemplate_wet.bgsm"]   repeat resolve parsed 4 more
  materials\template\defaulttemplate_wet.bgsm: chain len 3 …  repeat resolve parsed 3 more
  ```
  Vanilla has zero diagnostics, so there is no warning flood on vanilla data today. The repeated extract, inflate and parse does happen on vanilla.
- **Impact**:
  - **Performance (vanilla):** 1,753 of 6,616 vanilla materials pay about 3.6 archive extracts plus BGSM parses per `resolve_bgsm`, which runs per mesh merge and per REFR swap on the cell-load path, instead of zero after the first resolve.
  - **Diagnostics (mod content):** the #5010 diagnostic contract degrades into a warning per merge.
  - No wrong bytes result.
- **Related**: #2701, #5010, #1148, #3899, `/audit-performance`, `/audit-fo4`.
- **Suggested Fix**:
  - Run `visited` and the cache key through one canonical form, either a key-normalising hook on `TemplateResolver` or normalisation inside the provider before `resolve`. The self-loop would then be caught at its first occurrence.
  - Treat a self-loop (parent key equals own key) as not truncating, since its cut is entry-independent. Only a multi-node cycle needs #2701's no-cache rule.
  - Pin it with a cache-hit test on a self-referential fixture: the second resolve gives zero `parsed` calls.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
