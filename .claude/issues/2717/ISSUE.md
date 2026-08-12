# #2717: Every FO4 AVM2 menu is round-tripped through a full parse_swf -> write_swf re-serialization

- **Severity**: MEDIUM
- **Dimension**: 2 (data integrity on the content boundary)
- **Location**: [`crates/ui/src/avm2_host.rs`](../../crates/ui/src/avm2_host.rs):54-137
- **Status**: NEW
- **Description**: `inject_host_object_adapter` decompresses the SWF, **parses
  every tag into the `swf` crate's typed representation**, mutates two of them,
  and then re-serializes the whole movie with `write_swf`. Every tag in the
  file — fonts, bitmaps, sprites, sounds, shapes — is decoded and re-encoded,
  not copied. Any imperfection in the `swf` crate's write path for a tag that
  Bethesda's authoring tool emitted becomes silent content corruption in a
  menu that still "loads".
  The contrast inside this very crate is the tell: the sibling rewrite in
  `prepare_import_asset_swf` ([`crates/ui/src/navigator.rs`](../../crates/ui/src/navigator.rs):383-424)
  deliberately avoids this, walking `raw_tag_records` and emitting through
  `swf::write::write_swf_raw_tags` so untouched tags pass through byte-for-byte.
  The injection path does not take that care.
- **Evidence**:
  ```rust
  // crates/ui/src/avm2_host.rs:56  — full typed parse of the entire movie
  let mut movie = parse_swf(&decompressed).map_err(...)?;
  ...
  // :134 — full typed re-encode of the entire movie
  let mut patched = Vec::new();
  write_swf(movie.header.swf_header(), &movie.tags, &mut patched)
  ```
  Coverage: `Fallout4 - Interface.ba2` holds **1101 files, 311 of them `.swf`**
  (BA2 GNRL name-table walk). The only test that drives a real menu through the
  injection path, `installed_fallout4_representative_menus_obey_host_object_lifecycle`
  ([`crates/ui/src/host/tests.rs`](../../crates/ui/src/host/tests.rs):348), covers
  **three** paths, two of which are AVM2-injected, and is `#[ignore]`d behind
  "requires an installed Fallout 4 corpus" — so it does not run in CI.
- **Impact**: Not a demonstrated failure — I ran the ignored corpus tests and
  all three pass on real Fallout 4 data (see §4), which is why this is MEDIUM
  and framed as an unverified-surface finding rather than a bug. But the
  evidence base for "re-serializing every FO4 menu is lossless" is 2 menus out
  of 311, checked by a test nothing runs automatically. A silently corrupted
  glyph table or sprite is exactly the failure this shape produces, and it
  would surface as an unexplained rendering defect far from its cause.
- **Related**: SAFEUI-04 shares the coverage root cause.
- **Suggested Fix**: Move the injection to the same raw-tag strategy the
  navigator already uses (`raw_tag_records` + `write_swf_raw_tags`), splicing
  the adapter `DoABC2` and the patched root `DoABC` as opaque byte records so
  no untouched tag is ever re-encoded. Failing that, widen the corpus test to
  sweep all 311 SWFs asserting parse→inject→parse succeeds, and gate it on a
  data-present environment variable rather than `#[ignore]`.

---
**Source**: `docs/audits/AUDIT_SAFETY_UI_2026-08-12.md` (finding `SAFEUI-03`)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan/wgpu objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (prefer a default-suite test, not `#[ignore]`d)

