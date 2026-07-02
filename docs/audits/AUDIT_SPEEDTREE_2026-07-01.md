# SpeedTree Subsystem Audit — 2026-07-01

Audit of the `byroredux-spt` crate (Session 33 Phase 1, "S1") — the
`.spt` parameter-section TLV walker and the placeholder-billboard import
fallback. Cross-cuts the cell-loader extension dispatch, the `--tree`
loose-file visualiser, the TREE record parser, the billboard system,
and the NIFAL material-translation boundary the placeholder mesh flows
through.

- **Scope**: `crates/spt/src/` (`parser.rs`, `tag.rs`, `version.rs`,
  `stream.rs`, `scene.rs`, `import/mod.rs`, feature-gated `recon/mod.rs`)
  + `byroredux/src/cell_loader/references.rs` (`.spt` dispatch,
  `parse_and_import_spt`) + `byroredux/src/cell_loader/nif_import_registry.rs`
  (`CachedNifImport` — the cell-loader split moved this struct out of
  `references.rs` since the last audit) + `byroredux/src/cell_loader/spawn.rs`
  + `byroredux/src/scene/nif_loader.rs` + `crates/plugin/src/esm/records/tree.rs`
  + `byroredux/src/systems/billboard.rs` + `byroredux/src/material_translate.rs`
  + `crates/core/src/ecs/components/material.rs` (`classify_pbr_keyword`,
  `resolve_pbr`).
- **Depth**: deep (source review + live corpus acceptance run + a
  compiled-classifier verification against real vanilla texture names).
- **Dedup**: `gh issue list … "speedtree OR spt OR TREE"` (state=all,
  saved to `/tmp/audit/speedtree/issues.json` and a follow-up
  `"SPT"`-scoped query). Findings SPT-D4-01..04 / SPT-D5-01..02 /
  SPT-D2-01 / SPT-D3-01 / SPT-D1-01 (#994–#1002) are all CLOSED and
  verified still fixed (regression guards below). SPT-NEW-02/03/04
  (#1707/#1711/#1715) from the 2026-06-23 report are also CLOSED —
  #1707 and #1715 were fixed by commit `b312951b` (doc/comment
  corrections), #1711 was resolved-by-documentation in `af359ed8`
  (intentional route divergence, explicitly commented at the adapter
  struct). SPT-NEW-01 (`detect_variant` dead code) was flagged in the
  06-23 report but **never filed as its own issue** — re-verified
  still accurate against current code, carried forward below.
- **Prior reports**: `docs/audits/AUDIT_SPEEDTREE_2026-05-13.md` (9
  findings → #994–#1002), `docs/audits/AUDIT_SPEEDTREE_2026-06-14.md`,
  `docs/audits/AUDIT_SPEEDTREE_2026-06-23.md` (4 findings, SPT-NEW-01..04).

## Verdict

The subsystem remains healthy on every dimension the prior audits
covered: the byte-accounting walker, tag dictionary, sizing precedence,
billboard wiring, and TREE→placeholder handoff are all correct, and
every one of the nine original findings' regression guards still holds.
The three prior-report LOW findings that were filed as issues
(#1707/#1711/#1715) are now closed with fixes verified in the current
tree.

Going one level deeper than prior passes on **Dimension 6 (NIFAL
Material Translation)** — actually tracing what happens when the
SpeedTree placeholder's `metalness_override: None` / `roughness_override:
None` sentinel reaches `Material::resolve_pbr`'s keyword classifier,
rather than just confirming the sentinel is set — surfaced a real,
live-corpus-reachable finding (**SPT-NEW-05**, HIGH per the NIFAL
severity floor): the classifier's substring keyword matching mis-tags
at least two vanilla Oblivion shrub species (`Boxwood`, `Elderberry`)
as WOOD or GLASS material because their names happen to contain
"wood" / "ice" as substrings. Confirmed against the actual compiled
classifier, not just static analysis. A second LOW finding
(**SPT-NEW-06**) documents a byte-offset imprecision in
`format-notes.md`'s "14000-band tail tags" worked example that would
mislead a future dictionary-extension pass.

No CRITICAL findings. One HIGH, two LOW.

### Corpus acceptance gate (live run, this audit)

```
[FO3] 10 files | 10 with entries | 0 hit unknown tag | 1800 entries total | 100.00 % coverage
[FNV] 10 files | 10 with entries | 0 hit unknown tag | 1800 entries total | 100.00 % coverage
[OBL] 113 files | 113 with entries | 4 hit unknown tag | 20425 entries total | 96.46 % coverage
```
`BYROREDUX_FNV_DATA=… BYROREDUX_FO3_DATA=… BYROREDUX_OBL_DATA=… cargo test
-p byroredux-spt --release --test parse_real_spt -- --ignored --nocapture`
(env vars point at each game's `Data` directory root). All three ≥ 95 %,
identical to the 2026-06-23 run. The 4 Oblivion bails are the same 4
files as every prior report (`treems14canvasfreesu`, `treecottonwoodsu`,
`shrubms14boxwood`, `treems14willowoakyoungsu`) — traced this run to
tag `768` at the exact byte offset the walker lands on after correctly
consuming a `FixedBytes(7)` payload (see SPT-NEW-06 for the doc-precision
gap this uncovered).

### Unit tests

`cargo test -p byroredux-spt` — 45 unit tests + 3 pinned-fixture
synthetic-corpus tests, all pass. `tests/parse_synthetic_spt.rs`'s
SHA/byte-pinned fixtures (closing #998) still hold.

---

## Regression Guards (prior findings re-verified — all HOLD)

| ID / Issue | What | Guard verified |
|---|---|---|
| SPT-D4-01 / #994 | Cell-path Billboard component dropped | `spawn.rs:248-250` inserts `Billboard::new(mode)` from `cached.placement_root_billboard`; `references.rs` surfaces it via `parse_and_import_spt`. **HOLD** |
| SPT-D4-02 / #995 | `bs_bound` not Z-up→Y-up | `import/mod.rs:159-178` routes center through `coord::zup_to_yup_pos`, half-extents `(hx, hz, hy)`. `placeholder_uses_obnd_bounds_when_present`. **HOLD** |
| SPT-D5-01 / #996 | `wind` docstring claimed BNAM | `import/mod.rs:68-74` correctly attributes wind to CNAM, BNAM to `bounds`; `references.rs:1333-1337` sets `wind = None` with an explicit "not consumed" comment (TD5-011 gate, not a drop). **HOLD** |
| SPT-D2-01 / #997 | "first wins" undocumented | `import/mod.rs:121-130` first-wins comment; `leaf_texture_override_wins_over_spt_tag` test. **HOLD** |
| SPT-D3-01 / #998 | No in-tree byte-stable fixture | `tests/parse_synthetic_spt.rs` pinned-byte fixtures, 3 tests, all pass. **HOLD** |
| SPT-D1-01 / #999 | tag-13005 bimodal bail | `MaybeStringElseBare` in `parser.rs:84-120`, dispatch in `tag.rs:92`; verified against the live Oblivion corpus this audit — 13005 correctly resolves as `String` (104-byte curve) in all 4 outlier files before a *separate*, unrelated tag-768 bail 168 bytes later (see SPT-NEW-06). **HOLD** |
| SPT-D4-03 / #1000 | normal +Z / winding | `import/mod.rs:274-282` normals `[0,0,-1]`, indices `[0,3,2,2,1,0]`; both geometric-normal tests pass. **HOLD** |
| SPT-D4-04 / #1001 | Oblivion MODB sizing | `compute_billboard_size` MODB→`(R,2R)`; `modb_drives_placeholder_size_when_obnd_absent`. **HOLD** |
| SPT-D5-02 / #1002 | BNAM unused | `compute_billboard_size` BNAM tier below OBND; `obnd_precedence_over_bnam`, `bnam_*` tests; wired via `references.rs`. **HOLD** |
| SPT-NEW-02 / #1707 | Stale "Phase 1.2, ships only version dispatch" docstring | Fixed in `b312951b`; `lib.rs:34-44` now correctly describes the shipped walker + importer. **HOLD** |
| SPT-NEW-03 / #1711 | `bs_bound` computed-then-discarded on cell route | Resolved by documentation in `af359ed8`: `nif_import_registry.rs:110-126` carries an explicit comment explaining the intentional divergence, why `BSBound` has no functional consumer, and the exact re-enable path. **HOLD** (documented, not threaded — issue stays open by design per the owner's own comment, not a live gap) |
| SPT-NEW-04 / #1715 | `BsRotateAboutUp` comment claimed "local Z axis" | Fixed in `b312951b`; `billboard.rs:124-130` now correctly describes the world-Y yaw-lock approximation. **HOLD** |

OBND→BNAM→MODB→default precedence and the `[16, 8192]` clamp are intact
on every path. Byte-accounting dictionary sizes (8003/8005/8009=52,
13008=11, 13013=7, 12002=16, 12003=20, ArrayBytes 10002 stride 1 /
10003 stride 8) match `crates/spt/docs/format-notes.md` and the corpus
histogram; no desync observed in the live run.

---

## SPT-NEW-01: `detect_variant` / `SpeedTreeVariant` are dead code — no production or test consumer

- **Severity**: LOW
- **Dimension**: Per-Game Variants
- **Location**: `crates/spt/src/version.rs:90-100` (`detect_variant`), `crates/spt/src/version.rs:24-37` (`SpeedTreeVariant` enum) + `:39-49` (its `impl` block), `crates/spt/src/lib.rs:61`
- **Status**: NEW (carried forward from `docs/audits/AUDIT_SPEEDTREE_2026-06-23.md`'s SPT-NEW-01 — raised there but **never filed as its own GitHub issue**, unlike its three siblings SPT-NEW-02/03/04 which became #1707/#1711/#1715. Re-verified against current code this audit; still accurate.)
- **Description**: `detect_variant` and the `SpeedTreeVariant` enum are
  `pub use`-re-exported from `lib.rs` but have zero call sites outside
  `version.rs`'s own unit tests, aside from one feature-gated recon dev
  tool. The production parse path (`parse_spt`) independently
  re-validates `MAGIC_HEAD` via `bytes.starts_with(MAGIC_HEAD)`
  (`parser.rs:48`) and never consults `detect_variant`. The placeholder
  importer is variant-agnostic. This confirms the audit-checklist
  expectation that "nothing downstream depends on the variant being
  correct today" — and as a corollary, the documented quirk that
  `detect_variant` defaults every `__IdvSpt_02_` file (including
  Oblivion 4.x) to `V5Fnv` remains benign, because no consumer branches
  on the result.
- **Evidence**:
  ```
  $ grep -rn "detect_variant\|SpeedTreeVariant" --include='*.rs' byroredux crates | grep -v version.rs
  crates/spt/src/lib.rs:36:  (doc comment mentioning it)
  crates/spt/src/lib.rs:61:pub use version::{detect_variant, SpeedTreeVariant};
  crates/spt/examples/spt_dissect.rs:36:use byroredux_spt::version::{detect_variant, MAGIC_HEAD};
  crates/spt/examples/spt_dissect.rs:63:    let variant = detect_variant(&bytes);
  ```
  The `spt_dissect.rs` hit is a `#[cfg(feature = "recon")]`-gated
  single-file dev dissector tool (predates the original 2026-06-23
  finding — `git log --diff-filter=A` shows it landed in the Phase-1.3-
  prep commit), not a production or test consumer.
- **Impact**: None at runtime. Maintenance only: the API reads as a
  live per-game dispatch hook but is inert, which can mislead a future
  contributor into "fixing" the `V5Fnv` default or wiring it where the
  per-REFR `.spt` route already works without it.
- **Related**: Dimension 4 route-divergence checklist item in the
  audit-speedtree skill. Distinct from SPT-NEW-03/#1711 (that finding
  was about `bs_bound`, a completely different field, being dropped on
  the cell-loader route).
- **Suggested Fix**: Either wire `detect_variant` into the cell-loader
  `.spt` route as a logged sanity check (it would let Oblivion vs FO3/FNV
  be distinguished by the caller's `GameKind` when the geometry-tail
  decoder lands), or mark the API `#[allow(dead_code)]` with a
  "reserved for Phase 2 geometry-tail variant dispatch" note so its
  inert status is explicit. This time, actually file it as a GitHub
  issue so it doesn't fall through the cracks a second time.

---

## SPT-NEW-05: Foliage-texture-path substring collisions in the PBR keyword classifier mis-tag vanilla trees as wood or glass

- **Severity**: HIGH
- **Dimension**: NIFAL Material Translation
- **Location**: `crates/core/src/ecs/components/material.rs:449-592` (`classify_pbr_keyword`), reached via `byroredux/src/material_translate.rs:157-160` (`translate_material` → `Material::resolve_pbr`); `crates/spt/src/import/mod.rs:320-334` (`placeholder_billboard_mesh` — the mesh that ships `metalness_override: None, roughness_override: None`)
- **Status**: NEW
- **Description**: The SpeedTree placeholder billboard is the **only
  production content type** that reaches `Material::resolve_pbr`'s
  keyword-classifier "sentinel backstop" arm in practice. Every real
  NIF mesh extractor (`ni_tri_shape.rs`, `bs_tri_shape.rs`,
  `bs_geometry.rs`) calls `classify_legacy_pbr` **at import time** and
  populates `metalness_override: Some(...)`, so `resolve_pbr`'s
  classifier never actually fires for NIF content — its comment at
  `material_translate.rs:60-69` even calls this arm "a sentinel-backstop
  (only fires when the override is `NaN`, i.e. for future non-NIF
  paths)". SpeedTree ships exactly that non-NIF path: the placeholder
  mesh literal sets both overrides to `None`
  (`import/mod.rs:333-334`), so `translate_material` seeds
  `Material.metalness = f32::NAN` and `resolve_pbr` runs
  `classify_pbr_keyword` against the **resolved leaf/bark texture
  path** — the TREE.ICON string or the `.spt`'s tag-4003 fallback.

  `classify_pbr_keyword`'s dispatch is plain case-insensitive
  **substring** matching (`contains_any_ci`), with no word-boundary
  check and no foliage-aware bucket (its keyword taxonomy is tuned for
  architecture / weapons / cloth / skin, never anticipating tree
  species names). Two real vanilla Oblivion tree textures collide:
  - `ShrubBoxwoodLeaves.dds` / `ShrubBoxwoodLeavesFA.dds` /
    `ShrubMS14BoxwoodLeaves01.dds` (Boxwood shrub, `.spt`-backed TREE
    records `shrubms14boxwood.spt` et al.) contain `"wood"` as a
    substring → misclassified as **WOOD** (`roughness: 0.7, metalness:
    0.0`) instead of the correct foliage matte-default fallback
    (`roughness: 0.85, metalness: 0.0`).
  - `ShrubGenericElderberryLeavesFA.dds` / `...SU.dds` (Elderberry
    shrub, `.spt`-backed TREE records `ShrubGenericElderberryFA.spt` /
    `ShrubGenericElderberrySU.spt`) contain `"ice"` as a cross-word
    substring (`"gener**ic**` + `**e**lderberry"` = `"...generICE
    lderberry..."`) → misclassified as **GLASS**
    (`roughness: 0.1, metalness: 0.0`) instead of the matte default.
    Roughness 0.1 crosses the RT-reflection gate documented at
    `material.rs:538-539` (`< 0.6` triggers ray-traced reflections in
    `triangle.frag`), so this specific tree's leaf billboard would
    render with mirror-smooth ray-traced reflections — a visible,
    incongruous "glass leaf" artifact, not just a subtle roughness
    nudge.
- **Evidence**: Confirmed against real game data and the actual
  compiled classifier (not just static keyword-list inspection).
  Extracted real vanilla `.spt`-linked texture names via
  `strings -n 6 "Oblivion.esm" | grep -iE "leaf|leaves|bark"`, found
  `ShrubBoxwoodLeaves.dds` and `ShrubGenericElderberryLeaves{FA,SU}.dds`
  among 40+ real tree-texture names, confirmed the matching `.spt`
  files (`shrubms14boxwood.spt`, `ShrubGenericElderberryFA.spt`,
  `ShrubGenericElderberrySU.spt`) exist in `Oblivion - Meshes.bsa`.
  Ran the actual compiled `classify_pbr_keyword` (temporarily added a
  test, ran it, reverted — no diff left in the tree) with
  `env_map_scale: 0.0, has_gloss_map: false` (the SpeedTree
  placeholder's literal values):
  ```
  Boxwood:    metalness=0 roughness=0.7   (WOOD arm fired)
  Elderberry: metalness=0 roughness=0.1   (GLASS arm fired)
  WhiteOak:   metalness=0 roughness=0.85  (correct matte fallback — no keyword collision)
  ```
  `WhiteOakLeaves01.dds` (a real FNV tree texture, no keyword hit)
  round-trips correctly, confirming the *mechanism* is otherwise sound
  — this is specifically a keyword-taxonomy/substring-matching gap, not
  a broken classifier.
- **Impact**: Visual-only (no crash, no data corruption, no GPU
  hazard). Every metalness output stays `0.0` (never promoted to
  metallic — the checklist's specific "never promote the billboard to
  metallic-roughness" concern does NOT materialize), but roughness is
  wrong for at least two vanilla Oblivion tree species, and the
  Elderberry case is severe enough to visibly cross the RT-reflection
  threshold on a foliage billboard. Blast radius: any `.spt`-backed
  tree/shrub whose TREE.ICON or embedded leaf-texture path happens to
  contain a keyword substring from `classify_pbr_keyword`'s architecture/
  weapon/cloth/skin-tuned dictionary. Not scanned exhaustively across
  all three games' corpora (FO3/FNV tree names weren't checked beyond
  the samples already pulled for Dimension 4) — likely not the only
  collision in the full corpus. Per `_audit-severity.md`'s special-rules
  table, "Wrong/divergent `Material` out of NIFAL `translate_material`"
  is an unconditional HIGH floor — there is no per-draw classifier
  fallback downstream to mask a wrong roughness/metalness value once it
  lands on the canonical `Material` component, which is exactly what's
  observed here (confirmed against the live compiled classifier and
  real vanilla texture names, not a theoretical substring scan).
- **Related**: SPT-D4-04 / #1001 and SPT-D5-02 / #1002 (the sizing
  precedence findings that established `compute_billboard_size`'s
  layered fallback design — this finding is the PBR-classification
  analogue of the same "does the fallback chain actually produce a
  sane placeholder" question, just one level deeper than prior audits
  traced it). Distinct from #1346/#1365 (which were about **doc
  framing** of the import-time-vs-translate-time classifier split, not
  about classifier correctness on foliage input).
- **Suggested Fix**: Two independent options, either sufficient alone:
  (a) Have `placeholder_billboard_mesh` set an explicit non-`NaN`
  `metalness_override: Some(0.0)` / `roughness_override: Some(0.85)`
  (the correct foliage matte default) instead of leaving both `None` —
  this makes the SpeedTree importer classify-at-import like every NIF
  path already does, matching the "classify-at-import + clamp-at-
  translate" structure `material_translate.rs`'s own doc comment
  describes as the intended architecture (#1346). This is the
  parity-preserving fix and needs no change to the shared classifier.
  (b) Add word-boundary-aware matching (or a small foliage-specific
  keyword bucket checked before the generic WOOD/GLASS arms) to
  `classify_pbr_keyword` if the sentinel-backstop arm is meant to stay
  reachable for future non-NIF/non-SpeedTree content too. (a) is
  narrower-scoped and lower-risk; (b) fixes the taxonomy gap at its
  root for any future content type that also lands on the backstop.

---

## SPT-NEW-06: `format-notes.md`'s "14000-band tail tags" worked hex example doesn't byte-align with where the live walker actually bails

- **Severity**: LOW
- **Dimension**: Walker Byte-Accounting (doc-precision)
- **Location**: `crates/spt/docs/format-notes.md:588-609` ("Open: 14000-band tail tags in the 4 outliers")
- **Status**: NEW
- **Description**: The doc's worked example claims the 4 Oblivion
  outlier files' second bail point is caused by tag `14007` (out of
  `TAG_MAX = 13999` range) at "offset 6208" for
  `treems14canvasfreesu.spt`, with the walker's `Unknown`-recorded
  value `768` dismissed as a mis-decode of the same bytes ("Byte-level
  inspection shows these bytes are *not* a 768-tag"). Re-tracing this
  live against the actual game file this audit: the walker's prior
  entry is `tag=13013` (`FixedBytes(7)`) landing at offset 6200,
  consuming exactly 4 (tag) + 7 (payload) = 11 bytes — i.e. `[6200,
  6211)` — so the next tag read genuinely starts at offset **6211**,
  not 6208. The four bytes at `[6211, 6215)` are `00 03 00 00`, which
  little-endian-decodes to **768**, exactly what `SptScene::unknown_tags`
  reports. The `14007` value the doc describes (`B7 36 00 00` at bytes
  `[6208, 6212)`) is a real value in the file, but it's not the u32 the
  walker's deterministic byte accounting actually reads next — it's 3
  bytes earlier than where the prior tag's declared payload size places
  the cursor.
- **Evidence**: Direct extraction + parse of
  `trees\treems14canvasfreesu.spt` from `Oblivion - Meshes.bsa`
  (6908 bytes) confirms `scene.tail_offset == 6211`,
  `scene.unknown_tags == [(768, 6211)]`, and the last successful entry
  is `tag=13013, offset=6200, value=Fixed([0xCC,0xCC,0x4C,0x3D,0xB7,
  0x36,0x00])` (7 bytes, exactly matching `FixedBytes(7)`'s declared
  size). Byte-by-byte little-endian decode of `[6195, 6220)` confirms
  offset 6208 reads `14007` only if you start counting from a
  3-bytes-early anchor; offset 6211 (where the walker's own math
  actually lands) reads `768`. The doc's own hex table (`format-notes.md:
  596-603`) labels its first row `"4504: B7 36 00 00 = 14007"` for a
  *different* file (offsets differ per file; the `treems14canvasfreesu`
  numbers in this finding are this audit's own cross-check of the same
  claim against a different sample from the same 4-file set).
- **Impact**: Documentation-only; the parser's actual behaviour is
  correct and the corpus acceptance gate (96.46% Oblivion, ≥95% floor)
  is unaffected. The risk is purely forward-looking: `format-notes.md`
  explicitly recommends a "Follow-up: re-run the recon harness with
  `TAG_MAX = 16000` ... extend `dispatch_tag`" using this worked
  example as the starting anchor. A future contributor extending the
  dictionary from this doc's byte table would add a `14007` dispatch
  arm expecting the walker to read it there, but the walker's real
  landing point is a different u32 value (`768`) — the extension
  wouldn't take effect until someone re-derives the correct alignment,
  silently wasting the next dictionary-expansion attempt.
- **Related**: SPT-D1-01 / #999 (the original 13005 bimodal fix this
  doc section chronicles the follow-up to). Does not affect the
  regression guard for #999 — the 13005 disambiguation itself is
  confirmed correct in this audit (see Regression Guards table).
- **Suggested Fix**: Re-run `spt_recon`/`spt_dissect` against the 4
  outlier files with the corrected offset anchor (start from the
  walker's actual `tail_offset`, not a manually-eyeballed hex-dump
  position) and update the worked example's offset/value pairing
  before anyone acts on the "extend TAG_MAX to 16000" follow-up.

---

## Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 1 (SPT-NEW-05) |
| MEDIUM | 0 |
| LOW | 2 (SPT-NEW-06 new this audit; SPT-NEW-01 carried forward, still unfiled from the prior report) |
| **Total** | **3** |

### Per-dimension bill of health

| Dimension | Verdict | Notes |
|---|---|---|
| 1 — Walker Byte-Accounting | Clean (1 doc nit) | Cursor advances match dictionary kinds exactly; 64 KiB caps on `count×stride` and string length confirmed on byte count not just count; clean EOF + unknown-in-range bail (non-fatal); the only `Err` paths (magic-missing, mid-payload underflow, oversized-string, oversized-array) all funnel through the identical graceful `log::warn → None → skip REFR` handling at the caller — no new fatal path breaks the cell-loader fallback contract. Corpus run: byte accounting hand-verified correct against real game bytes; SPT-NEW-06 is a doc-precision gap, not a parser bug. |
| 2 — Placeholder Fallback | Clean | `import_spt_scene` takes already-parsed `&SptScene` and is infallible — it never sees a `Result`, so the "does it handle Err gracefully" question resolves one level up at the caller, which does. Texture/size precedence + clamps + Z-up→Y-up + `-Z` winding all correct and test-covered. |
| 3 — TREE → Billboard Wiring | Clean | TREE dual-targeted into `statics` + typed `trees` map; `Billboard` inserted on the placement root (#994 guard holds); `bsx_flags=0`/`root_flags=0`/`flame=None` synthetic defaults honoured by spawn; CNAM 5-float (Oblivion) vs 8-float (FO3/FNV) shape-tolerance test-verified, not a silent mis-parse; mixed `.nif`+`.spt` REFRs coexist via per-REFR extension switch; no leaked BLAS (billboard mesh flows through the generic `build_blas_batched` path, no special-casing); cell unload has no `Billboard`-specific teardown code, so it's covered by the generic despawn path. |
| 4 — Per-Game Variants & Route Divergence | Mostly clean | Both routes call `parse_spt` + `import_spt_scene` identically; loose route uses `default()` params (documented, no TREE metadata). `detect_variant` still dead outside its own tests + one recon example (SPT-NEW-01, carried forward, still unfiled). `bs_bound` route divergence (#1711) is resolved-by-documentation, confirmed still in place at `nif_import_registry.rs:110-126`. `MAGIC_HEAD` exact-20-byte / one-flip-rejects / short-input-rejects all test-confirmed. |
| 5 — Tag Dictionary | Clean | ~90 tags; spot-checked fixed-size assignments (8003/8005/8009=52, 13008=11, 13013=7, 12002=16, 12003=20, ArrayBytes strides 1/8) all match `format-notes.md` and the corpus histogram, hand-verified against real bytes this audit; confounders (`4096`, `5376`, …) stay `Unknown`; the 4 Oblivion bails traced to their exact root cause this audit (SPT-NEW-06). |
| 6 — NIFAL Material Translation | 1 HIGH finding | Placeholder canonicalised at the single `translate_material` boundary on both routes, no parallel material path. `is_pbr:false`, `from_bgsm:false`, `emissive_source:None`, two-sided alpha-test cutout all confirmed. **New this audit**: traced what actually happens when `metalness_override:None`/`roughness_override:None` reaches `resolve_pbr` — SpeedTree is the only production path that exercises the "sentinel backstop" keyword classifier (every NIF mesh classifies at import time instead), and the classifier's substring matching mis-tags real vanilla foliage (Boxwood→wood, Elderberry→glass) because its keyword taxonomy has no foliage bucket (SPT-NEW-05, HIGH per the NIFAL severity floor — metalness stays non-metallic, but roughness is wrong and the Elderberry case visibly crosses the RT-reflection gate). |

The S1 placeholder contract — *graceful fallback over zero rendering,
never an `Err` out of the cell loader, `.spt` REFRs routed to the
SpeedTree importer* — holds end to end, confirmed again this audit
with a live corpus run and hand-verified byte accounting. All twelve
prior findings (#994–#1002, #1707, #1711, #1715) are fixed and guarded.
This audit's new findings came from tracing one dimension (NIFAL
material translation) one level deeper than any prior SpeedTree audit
had — actually running the classifier against real vanilla texture
names rather than stopping at "the sentinel is set correctly" — and
from hand-verifying the walker's byte math against live game bytes
instead of trusting the prior report's summary framing.

### Suggested next step

Run `/audit-publish docs/audits/AUDIT_SPEEDTREE_2026-07-01.md` to file
SPT-NEW-05 (HIGH, `import-pipeline` domain label) and SPT-NEW-06 (LOW,
`tech-debt`). Also file SPT-NEW-01 (`detect_variant` dead code, LOW,
`tech-debt`) — it was described in the 2026-06-23 report but never
actually turned into a GitHub issue, unlike its three siblings.
