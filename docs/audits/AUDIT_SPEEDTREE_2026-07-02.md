# SpeedTree Subsystem Audit — 2026-07-02

**Scope**: `crates/spt/` (`byroredux-spt`) — the `.spt` TLV parameter-section
walker + placeholder-billboard import fallback (Session 33 Phase 1, "S1"),
plus its cross-cut wiring in `byroredux/src/cell_loader/references.rs`,
`byroredux/src/cell_loader/spawn.rs`, `byroredux/src/scene/nif_loader.rs`,
`crates/plugin/src/esm/records/tree.rs`, and
`byroredux/src/systems/billboard.rs`.

**Depth**: `deep` — corpus acceptance harness run live against on-disk
FNV / FO3 / Oblivion BSAs; classifier premise re-verified against the
compiled `classify_pbr_keyword`; walker byte-accounting hand-checked.

**Method**: Read the full crate + wiring, ran unit + corpus tests,
re-verified the three still-unfiled findings from the immediately-prior
report (`AUDIT_SPEEDTREE_2026-07-01.md`) against current source (the crate
and `material.rs` are byte-for-byte unchanged since that audit — last
commits `parser.rs`/`material.rs` 2026-06-09, `import/mod.rs` 2026-06-15),
and hunted for anything the prior sweep missed.

---

## Verification runs (this audit)

### Corpus acceptance gate — live run

```
BYROREDUX_FNV_DATA=… BYROREDUX_FO3_DATA=… BYROREDUX_OBL_DATA=… \
  cargo test -p byroredux-spt --release --test parse_real_spt -- --ignored --nocapture
```

```
[FO3] 10 files  | 10 with entries | 0 hit unknown tag | 1800  entries  | 100.00 % coverage
[FNV] 10 files  | 10 with entries | 0 hit unknown tag | 1800  entries  | 100.00 % coverage
[OBL] 113 files | 113 with entries| 4 hit unknown tag | 20425 entries  | 96.46 % coverage
  unknown-tag samples:
    trees\treems14willowoakyoungsu.spt | tag=768 (0x0300) at offset 5946
    trees\treecottonwoodsu.spt         | tag=768 (0x0300) at offset 5641
    trees\treems14canvasfreesu.spt     | tag=768 (0x0300) at offset 6211
    trees\shrubms14boxwood.spt         | tag=768 (0x0300) at offset 4507
```

All three gates pass (≥ 95 % floor). The 4 Oblivion bails are the known,
`format-notes.md`-documented 14000-band tail-tag limitation (see the
Regression Guards / SPT-NEW-06 discussion) — placeholder still renders for
all 4, none produce an `Err`.

### Unit tests

`cargo test -p byroredux-spt --release` — all pass (parser, stream, tag,
scene, import, synthetic-fixture). 0 failures.

---

## Findings

Three findings from `AUDIT_SPEEDTREE_2026-07-01.md` (one day prior) are
**re-verified as still valid and still unfiled** — none of SPT-NEW-01 /
-05 / -06 became GitHub issues (issue-list scan confirms; only unrelated
#1624/#1522 mention the classifier). They are carried forward below with
their premises re-confirmed against current source. One **new** LOW finding
(SPT-NEW-07) is added from this audit's walker edge-case tracing.

---

### SPT-NEW-05: Foliage texture-path substring collisions in the PBR keyword classifier mis-tag vanilla trees as wood/glass

- **Severity**: HIGH
- **Dimension**: NIFAL Material Translation
- **Location**: `crates/core/src/ecs/components/material.rs:449-489` (`classify_pbr_keyword` — `"glass"/"crystal"/"ice"/"gem"` arm at :483, `"wood"/"plank"/…` arm at :489), reached via `byroredux/src/material_translate.rs` (`translate_material` → `Material::resolve_pbr`); `crates/spt/src/import/mod.rs:328-334` (`placeholder_billboard_mesh` ships `metalness_override: None, roughness_override: None`)
- **Status**: NEW (raised in `AUDIT_SPEEDTREE_2026-07-01.md` as SPT-NEW-05, never filed; re-verified this audit — the `"ice"`/`"gem"`/`"wood"` substring arms are present unchanged at `material.rs:483`/`:489`)
- **Description**: The SpeedTree placeholder billboard is the only
  production content type that reaches `resolve_pbr`'s keyword-classifier
  "sentinel backstop" arm in practice — every real NIF mesh extractor
  classifies at import time and sets `metalness_override: Some(...)`, so the
  classifier never fires for NIF content. The placeholder mesh literal sets
  both overrides to `None`, so `translate_material` seeds
  `Material.metalness = NaN` and `resolve_pbr` runs `classify_pbr_keyword`
  against the resolved leaf texture path (TREE.ICON or the `.spt` tag-4003
  fallback). That classifier uses plain case-insensitive **substring**
  matching with no word-boundary check and no foliage bucket. Two real
  vanilla Oblivion tree textures collide:
  - `ShrubBoxwoodLeaves*.dds` (`shrubms14boxwood.spt`) contains `"wood"`
    → WOOD (`roughness 0.7`) instead of the matte foliage default (`0.85`).
  - `ShrubGenericElderberryLeaves*.dds` (`ShrubGenericElderberry{FA,SU}.spt`)
    contains `"ic"+"e"` across the `generIC` / `Elderberry` word seam
    (`…generICE lderberry…`) → GLASS (`roughness 0.1`). 0.1 crosses the
    RT-reflection gate (`< 0.6` triggers ray-traced reflections in
    `triangle.frag`), so the leaf billboard would render mirror-smooth —
    a visible "glass leaf" artifact.
- **Evidence**: Prior audit ran the compiled `classify_pbr_keyword` against
  real extracted texture names (Boxwood → `roughness 0.7`; Elderberry →
  `roughness 0.1`; WhiteOak → correct `0.85`). This audit re-confirmed the
  colliding substring arms are present and unchanged in current
  `material.rs`, and that `import/mod.rs:333-334` still emits
  `metalness_override: None, roughness_override: None`.
- **Impact**: Visual-only (no crash / no GPU hazard); metalness stays `0.0`
  (never promoted metallic). But roughness is wrong for at least two vanilla
  Oblivion species and the Elderberry case visibly crosses the RT-reflection
  threshold on foliage. Per `_audit-severity.md`, "wrong/divergent `Material`
  out of NIFAL `translate_material`" is an unconditional HIGH floor — no
  per-draw fallback masks a wrong resolved `f32` once it lands on the
  canonical `Material`. Blast radius: any `.spt`-backed tree whose leaf path
  contains an architecture/weapon/cloth/skin keyword substring; the full
  FO3/FNV corpus was not exhaustively scanned, so this is likely not the
  only collision.
- **Related**: SPT-D4-04 / #1001, SPT-D5-02 / #1002 (the sizing-precedence
  findings — this is the PBR-classification analogue). Distinct from
  #1346/#1365 (classifier doc-framing, not correctness). Cross-cuts
  `/audit-nifal` (single-boundary is respected; this is a classifier-taxonomy
  gap on the backstop arm, not a bypass).
- **Suggested Fix**: (a) Have `placeholder_billboard_mesh` set explicit
  `metalness_override: Some(0.0)` / `roughness_override: Some(0.85)` so the
  SpeedTree importer classifies-at-import like every NIF path — narrow,
  parity-preserving, no shared-classifier change. OR (b) add word-boundary /
  foliage-bucket matching to `classify_pbr_keyword` if the backstop arm is
  meant to stay reachable for future non-NIF content. (a) is lower-risk.

---

### SPT-NEW-01: `detect_variant` / `SpeedTreeVariant` are dead code — no production or test consumer

- **Severity**: LOW
- **Dimension**: Per-Game Variants
- **Location**: `crates/spt/src/version.rs:90-100` (`detect_variant`), `:24-49` (`SpeedTreeVariant` + impl), `crates/spt/src/lib.rs:61`
- **Status**: NEW (raised in the 2026-06-23 and 2026-07-01 reports, never filed as an issue unlike its siblings #1707/#1711/#1715; re-verified still accurate)
- **Description**: `detect_variant` and `SpeedTreeVariant` are re-exported
  from `lib.rs` but have zero call sites outside `version.rs`'s own unit
  tests and one `#[cfg(feature = "recon")]` dev tool (`spt_dissect.rs:63`).
  The production `parse_spt` independently re-validates `MAGIC_HEAD` via
  `bytes.starts_with(...)` (`parser.rs:48`) and never consults
  `detect_variant`; the placeholder importer is variant-agnostic. This
  confirms the Dimension-4 checklist expectation — nothing downstream
  depends on the variant being correct — so the documented `V5Fnv` default
  for every `__IdvSpt_02_` file (including Oblivion 4.x) is benign.
- **Evidence**: `grep -rn "detect_variant\|SpeedTreeVariant" --include='*.rs'
  byroredux crates | grep -v version.rs` hits only `lib.rs` re-export/doc and
  the recon-gated `spt_dissect.rs` — no production or test consumer.
- **Impact**: None at runtime. Maintenance only: the API reads as a live
  per-game dispatch hook but is inert, which can mislead a contributor into
  "fixing" the `V5Fnv` default or wiring it where the per-REFR route already
  works.
- **Related**: Distinct from SPT-NEW-03 / #1711 (that is `bs_bound`, a
  different field).
- **Suggested Fix**: Either wire `detect_variant` into the cell-loader `.spt`
  route as a logged sanity check (useful once the geometry-tail decoder needs
  Oblivion-vs-FO3/FNV disambiguation), or mark it `#[allow(dead_code)]` with
  a "reserved for Phase 2 variant dispatch" note. File it as an issue this
  time so it doesn't fall through a third time.

---

### SPT-NEW-06: `format-notes.md`'s "14000-band tail tags" worked example doesn't byte-align with where the live walker bails

- **Severity**: LOW
- **Dimension**: Walker Byte-Accounting (doc-precision)
- **Location**: `crates/spt/docs/format-notes.md:588-609` ("Open: 14000-band tail tags in the 4 outliers")
- **Status**: NEW (raised 2026-07-01, never filed; re-verified — `format-notes.md` unchanged, live corpus bail offsets confirmed 4507/5641/5946/6211 at tag `768`)
- **Description**: The doc's worked example attributes the 4 Oblivion
  outliers' second bail to tag `14007` (out of `TAG_MAX = 13999`) at an
  eyeballed hex offset, dismissing the walker's actually-recorded value
  `768` as a mis-decode. But the walker's deterministic byte accounting
  genuinely lands the next tag read on the `00 03 00 00` (= 768) u32 — the
  prior entry `tag=13013` (`FixedBytes(7)`) consumes exactly 4 + 7 = 11
  bytes, placing the cursor at the offset that reads `768`, not 3 bytes
  earlier where `14007` sits. `SptScene::unknown_tags == [(768, …)]` in this
  audit's live run matches the walker's real cursor, not the doc's hex table.
- **Evidence**: This audit's live corpus run reports `tag=768 (0x0300)` at
  offsets 4507 / 5641 / 5946 / 6211 for the 4 outliers — exactly the walker's
  own byte math, which contradicts the doc's `14007`-at-a-different-offset
  narrative.
- **Impact**: Documentation-only; parser behaviour is correct and the
  acceptance gate is unaffected. Forward-looking risk: the doc recommends a
  "re-run with TAG_MAX = 16000, extend `dispatch_tag`" follow-up anchored on
  this worked example. A contributor adding a `14007` arm from the doc's
  byte table would find the walker never reads `14007` there (it reads `768`),
  silently wasting the dictionary-expansion attempt until someone re-derives
  the alignment.
- **Related**: SPT-D1-01 / #999 (the 13005 fix this doc section chronicles
  the follow-up to; the 13005 disambiguation itself is confirmed correct).
- **Suggested Fix**: Re-run `spt_recon`/`spt_dissect` starting from the
  walker's actual `tail_offset` (not a manual hex-dump position) and correct
  the worked example's offset/value pairing before anyone acts on the
  "extend TAG_MAX" follow-up.

---

### SPT-NEW-07: `MaybeStringElseBare` (tag 13005) can misparse a bare 13005 sitting immediately before the geometry tail as a length-prefixed string

- **Severity**: LOW
- **Dimension**: Walker Byte-Accounting
- **Location**: `crates/spt/src/parser.rs:84-120` (the `MaybeStringElseBare` arm)
- **Status**: NEW
- **Description**: The `MaybeStringElseBare` disambiguation (added for #999)
  works by: consume the tag's u32, peek the NEXT u32, and if that next value
  is a known dictionary tag treat the entry as `Bare`, otherwise read a
  length-prefixed string. The Bare classification therefore depends on the
  value following 13005 being an **in-range known tag**. If a bare 13005 is
  the last parameter entry and is immediately followed by the binary
  **geometry tail** (an out-of-range u32, e.g. a 14000-band value or raw
  geometry bytes), the peek sees a non-tag value, `next_is_known_tag` is
  `false`, and the walker takes the String branch — calling
  `read_string_lp()` which reads that tail u32 as a **byte length** and
  consumes that many bytes of the geometry tail as a bogus string.
  Crucially the walker's tail-detection guard (`parser.rs:69`, the
  out-of-range peek check) runs *before* `dispatch_tag` on the current tag,
  but it never re-checks the value *after* 13005 has been consumed — so the
  tail sentinel that would normally stop the walker is instead swallowed as
  string-payload length. The entry is pushed to `scene.entries` with no
  `unknown_tags` diagnostic, so the corpus harness would score such a file
  as "clean" even though it desynced.
- **Evidence**: Trace of `parser.rs:102-114`: `let _ = stream.read_u32_le()?`
  consumes the 13005 tag; `peek_u32_le()` reads the value at the tail
  boundary; for an out-of-range tail value `next_is_known_tag == false` →
  `SptValue::String(stream.read_string_lp()?)`. `read_string_lp` caps at
  64 KiB but a typical tail u32 (e.g. `768` or `14007`) is well under the
  cap, so it does not `Err` — it reads `len` tail bytes and succeeds. Live
  corpus run shows **0** such misparses today: in all 113 Oblivion files the
  bare 13005 is followed by a known tag (that is exactly why the 109 bare
  files are clean and the 4 outliers carry a real curve string), so this is a
  latent gap, not an active corpus regression. The existing guard
  `tag_13005_at_eof_does_not_panic` only covers the *EOF-immediately-after*
  case (peek returns `None` → `false`, then `read_string_lp` `UnexpectedEof`s
  cleanly); it does not cover 13005 followed by a non-EOF out-of-range tail
  value.
- **Impact**: Defense-in-depth only at present (no vanilla file triggers it).
  If a mod-authored or DLC `.spt` ever emits a bare 13005 directly before the
  geometry tail, the walker would consume a slab of the tail as a garbage
  string, mis-set `tail_offset` past the real tail start, and silently drop
  any remaining parameter entries — while still reporting a clean parse. The
  placeholder billboard still renders (the import ignores curve strings), so
  no crash and no `Err` out of the cell loader; the practical damage is a
  wrong `tail_offset` / lost trailing parameters, which only matters once the
  Phase 2 geometry-tail decoder consumes `tail_offset`.
- **Related**: SPT-D1-01 / #999 (introduced the `MaybeStringElseBare` arm);
  the `parser.rs` doc comment at :95-101 already acknowledges the *inverse*
  pathological case (a string length coinciding with a dictionary tag value
  misparsing as Bare) but not this tail-swallow direction.
- **Suggested Fix**: In the `MaybeStringElseBare` String branch, before
  reading the string, gate on the peeked value being a plausible length
  rather than a tail sentinel — e.g. only take the String branch when the
  peeked value is `< remaining_bytes` **and** below a sane string-length
  ceiling (the corpus max is ~525 B; the 64 KiB cap is far too loose to
  distinguish a curve length from a tail u32). Alternatively, require the
  bytes read as a string to be printable-ASCII (BezierSpline blobs always
  are) and fall back to `Bare` + `tail_offset` when they aren't. Add a
  regression fixture: bare 13005 immediately followed by an out-of-range
  tail u32 must resolve as `Bare` with `tail_offset` at the 13005 successor,
  not consume the tail as a string.

---

## Regression Guards (verified in place, NOT re-reported)

All twelve prior findings are fixed and their guards hold this audit:

| Finding | Issue | Guard verified |
|---|---|---|
| SPT-D4-01 (cell placeholder loses `Billboard`) | #994 | `spawn.rs:248` inserts `Billboard` when `placement_root_billboard.is_some()`; `parse_and_import_spt` sets `Some(BsRotateAboutUp)` |
| SPT-D4-02 (`bs_bound` Z-up→Y-up) | #995 | `import/mod.rs:168-178` routes center via `zup_to_yup_pos`, half-extents `(hx,hz,hy)`; `placeholder_uses_obnd_bounds_when_present` passes |
| SPT-D5-01 (`wind` docstring) | #996 | `SptImportParams.wind` doc says CNAM, not BNAM |
| SPT-D2-01 ("first wins" leaf tex) | #997 | `import/mod.rs:127-130` `.first()`; `leaf_texture_override_wins_over_spt_tag` passes |
| SPT-D3-01 (pinned regression sample) | #998 | `tests/parse_synthetic_spt.rs` byte-pinned fixture passes |
| SPT-D1-01 (13005 bimodal) | #999 | `MaybeStringElseBare`; both `tag_13005_*` guards pass (but see SPT-NEW-07 for a residual tail edge) |
| SPT-D4-03 (normal/winding) | #1000 | `-Z` normals + `[0,3,2,2,1,0]` winding; both geometric-normal guards pass |
| SPT-D4-04 (default size / MODB) | #1001 | `compute_billboard_size` OBND→BNAM→MODB→default; `modb_drives_placeholder_size_when_obnd_absent` passes |
| SPT-D5-02 (BNAM precedence) | #1002 | OBND-beats-BNAM; `obnd_precedence_over_bnam` passes |
| BSXFlags dropped at spawn | #1214 | `bsx_flags = 0` synthetic default |
| SceneFlags / root_flags | #1235 | `root_flags = 0` synthetic default |
| SPT-NEW-02/03/04 doc/route | #1707/#1711/#1715 | #1707/#1715 CLOSED; #1711 (`bs_bound` route divergence) resolved-by-documentation at `nif_import_registry.rs:110-126`, still in place |

The **14000-band Oblivion tail** (4 files bail at tag `768`) is the
documented-in-`format-notes.md` Phase-1 limitation, above the 95 % gate,
placeholder-covered — not re-reported (only its doc-precision nit is, as
SPT-NEW-06).

---

## Per-Dimension Bill of Health

| Dimension | Verdict | Notes |
|---|---|---|
| 1 — Walker Byte-Accounting | Clean (1 residual edge + 1 doc nit) | Cursor advances match `SptTagKind` sizes exactly; 64 KiB caps on `count×stride` and string length confirmed on byte count; clean EOF + non-fatal unknown-in-range bail; only `Err` paths (magic-missing, mid-payload underflow, oversized string/array) all funnel to the caller's graceful `warn→None→skip-REFR`. New: SPT-NEW-07 (bare-13005-before-tail can misparse the tail as a string — latent, 0 corpus hits). Doc: SPT-NEW-06. |
| 2 — Placeholder Fallback | Clean | `import_spt_scene` is infallible (takes `&SptScene`); the "handle `Err` gracefully" question resolves at the caller, which does. Texture/size precedence, clamps, Z-up→Y-up, `-Z` winding all correct + test-covered. |
| 3 — TREE → Billboard Wiring | Clean | TREE dual-targeted into `statics` + typed `trees`; `Billboard` inserted on placement root (#994 holds); `bsx_flags=0`/`root_flags=0`/`flame=None` honoured; CNAM 5-float (Oblivion) vs 8-float (FO3/FNV) shape-tolerance test-verified; mixed `.nif`+`.spt` REFRs coexist via per-REFR extension switch; billboard mesh flows through the generic BLAS/despawn path (no leaks). |
| 4 — Per-Game Variants & Route Divergence | Mostly clean | Both routes call `parse_spt` + `import_spt_scene` identically; loose route uses `default()` params (documented). `detect_variant` dead outside its own tests + one recon example (SPT-NEW-01). `bs_bound` route divergence (#1711) resolved-by-documentation. `MAGIC_HEAD` exact-20-byte / one-flip-rejects / short-input-rejects test-confirmed. |
| 5 — Tag Dictionary | Clean | ~90 tags; fixed-size assignments (8003/8005/8009=52, 13008=11, 13013=7, 12002=16, 12003=20, ArrayBytes strides 1/8) match `format-notes.md` + the live histogram; confounders (`4096`, `5376`, …) stay `Unknown`; the 4 Oblivion bails traced to the documented 14000-band limitation. |
| 6 — NIFAL Material Translation | 1 HIGH | Placeholder canonicalised at the single `translate_material` boundary on both routes (no parallel path). `is_pbr:false`, `from_bgsm:false`, `emissive_source:None`, two-sided alpha-test cutout all confirmed. SPT-NEW-05 (HIGH): the placeholder is the only production path that reaches the sentinel keyword classifier, and its substring matching mis-tags real vanilla foliage (Boxwood→wood, Elderberry→glass). |

The S1 placeholder contract — graceful fallback over zero rendering, never
an `Err` out of the cell loader, `.spt` REFRs routed to the SpeedTree
importer — holds end to end (live corpus run + unit suite green).

---

## Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 0 |
| LOW | 3 |
| **Total** | **4** |

- **HIGH**: SPT-NEW-05 (foliage keyword-collision mis-tags vanilla trees; carried forward from 2026-07-01, still unfiled, premise re-verified against unchanged `material.rs`).
- **LOW**: SPT-NEW-01 (`detect_variant` dead code; carried forward, still unfiled), SPT-NEW-06 (`format-notes.md` byte-offset doc nit; carried forward, still unfiled), SPT-NEW-07 (new: 13005-before-tail String misparse edge).

Note: the crate and its classifier are byte-for-byte unchanged since the
2026-07-01 audit, so its three findings persist verbatim; the delta this
audit adds is SPT-NEW-07 and a fresh live corpus confirmation.

### Suggested next step

```
/audit-publish docs/audits/AUDIT_SPEEDTREE_2026-07-02.md
```

File SPT-NEW-05 (HIGH, `import-pipeline`), SPT-NEW-07 (LOW, `import-pipeline`),
SPT-NEW-01 and SPT-NEW-06 (LOW, `tech-debt`) — none of the carried-forward
findings from 2026-07-01 were ever filed as issues.
