# SpeedTree Subsystem Audit — 2026-06-23

Audit of the `byroredux-spt` crate (Session 33 Phase 1, "S1") — the
`.spt` parameter-section TLV walker and the placeholder-billboard import
fallback. Cross-cuts the cell-loader extension dispatch, the `--tree`
loose-file visualiser, the TREE record parser, and the billboard system.

- **Scope**: `crates/spt/src/` (`parser.rs`, `tag.rs`, `version.rs`,
  `stream.rs`, `scene.rs`, `import/mod.rs`, feature-gated `recon/mod.rs`)
  + `byroredux/src/cell_loader/references.rs`
  + `byroredux/src/cell_loader/spawn.rs`
  + `byroredux/src/scene/nif_loader.rs`
  + `crates/plugin/src/esm/records/tree.rs`
  + `byroredux/src/systems/billboard.rs`.
- **Depth**: deep (source review + corpus acceptance run).
- **Dedup**: `gh issue list … "speedtree OR spt OR TREE billboard"`
  (state=all, saved to `/tmp/audit/speedtree/issues.json`). No OPEN
  `.spt`-path issues. Prior findings #994–#1002 are all CLOSED.
- **Prior report**: `docs/audits/AUDIT_SPEEDTREE_2026-05-13.md`
  (9 findings → #994–#1002).

## Verdict

The subsystem is healthy. Every prior finding's fix is present in the
current tree and its regression guard holds (see Regression Guards
table). The corpus acceptance gate still clears ≥ 95 % on all three
games. The byte-accounting walker, tag dictionary, sizing precedence,
billboard wiring, and NIFAL handoff are all correct.

The only NEW findings are LOW maintainability items: a dead variant-
detection API, a stale crate-level module docstring, a computed-then-
discarded OBND bound on the cell path, and a misleading inline comment
in the billboard system. No CRITICAL / HIGH / MEDIUM findings.

### Corpus acceptance gate (live run, this audit)

```
[FNV] 10 files | 10 with entries | 0 hit unknown tag | 1800 entries  | 100.00 % coverage
[FO3] 10 files | 10 with entries | 0 hit unknown tag | 1800 entries  | 100.00 % coverage
[OBL] 113 files | 113 with entries | 4 hit unknown tag | 20425 entries | 96.46 % coverage
```
`cargo test -p byroredux-spt --release --test parse_real_spt -- --ignored`.
All three ≥ 95 %. The 4 Oblivion bails are the documented tag-104
curve-length confounder (SPT-D1-01 / #999 — recording-without-aborting
is the intended contract; the placeholder still renders).

### Unit tests

`cargo test -p byroredux-spt` — all pass. Note `tests/parse_synthetic_spt.rs`
now ships SHA-pinned byte fixtures (3 tests), which closes the prior
SPT-D3-01 / #998 gap (no in-tree byte-stable sample for CI without game
data).

---

## Regression Guards (prior findings re-verified — all HOLD)

| ID / Issue | What | Guard verified |
|---|---|---|
| SPT-D4-01 / #994 | Cell-path Billboard component dropped | `spawn.rs:248-250` inserts `Billboard::new(mode)` from `cached.placement_root_billboard`; `references.rs:1347-1351` surfaces it. `parse_and_import_spt_surfaces_billboard_mode_on_cache_entry` test. **HOLD** |
| SPT-D4-02 / #995 | `bs_bound` not Z-up→Y-up | `import/mod.rs:168-178` routes center through `coord::zup_to_yup_pos`, half-extents `(hx, hz, hy)`. `placeholder_uses_obnd_bounds_when_present`. **HOLD** (but see SPT-NEW-03 for the cell-path drop) |
| SPT-D5-01 / #996 | `wind` docstring claimed BNAM | `import/mod.rs:69-74` now correctly attributes wind to CNAM, BNAM to `bounds`. **HOLD** |
| SPT-D2-01 / #997 | "first wins" undocumented | `import/mod.rs:121-130` has the first-wins comment; `leaf_texture_override_wins_over_spt_tag`. **HOLD** |
| SPT-D3-01 / #998 | No in-tree byte-stable fixture | `tests/parse_synthetic_spt.rs` pinned-byte fixtures. **HOLD** |
| SPT-D1-01 / #999 | tag-13005 bimodal bail | `MaybeStringElseBare` in `parser.rs:84-120`, dispatch in `tag.rs:92`; `tag_13005_*` tests incl. EOF non-panic guard. **HOLD** |
| SPT-D4-03 / #1000 | normal +Z / winding | `import/mod.rs:276` normals `-Z`, `:282` indices `[0,3,2,2,1,0]`; both geometric-normal tests. **HOLD** |
| SPT-D4-04 / #1001 | Oblivion MODB sizing | `compute_billboard_size` MODB→`(R,2R)`; `modb_drives_placeholder_size_when_obnd_absent`. **HOLD** |
| SPT-D5-02 / #1002 | BNAM unused | `compute_billboard_size` BNAM tier below OBND; `obnd_precedence_over_bnam`, `bnam_*` tests; wired in `references.rs:1328`. **HOLD** |

OBND→BNAM→MODB→default precedence and the `[16, 8192]` clamp are intact
on every path. Byte-accounting dictionary sizes (8003/8005/8009=52,
13008=11, 13013=7, 12002=16, 12003=20, ArrayBytes 10002 stride 1 /
10003 stride 8) match `crates/spt/docs/format-notes.md` and pass the
corpus run with no desync.

---

## SPT-NEW-01: `detect_variant` / `SpeedTreeVariant` are dead code — no production or test consumer

- **Severity**: LOW
- **Dimension**: Per-Game Variants
- **Location**: `crates/spt/src/version.rs:90-100` (`detect_variant`), `crates/spt/src/version.rs:24-49` (`SpeedTreeVariant`), `crates/spt/src/lib.rs:58`
- **Status**: NEW
- **Description**: `detect_variant` and the `SpeedTreeVariant` enum are
  `pub use`-re-exported from `lib.rs` but have **zero** call sites
  outside `version.rs`'s own unit tests. The production parse path
  (`parse_spt`) independently re-validates `MAGIC_HEAD` via
  `bytes.starts_with(MAGIC_HEAD)` (`parser.rs:48`) and never consults
  `detect_variant`. The placeholder importer is variant-agnostic. This
  confirms the audit-checklist expectation that "nothing downstream
  depends on the variant being correct today" — and as a corollary, the
  documented quirk that `detect_variant` defaults every `__IdvSpt_02_`
  file (including Oblivion 4.x) to `V5Fnv` is **benign**, because no
  consumer branches on the result.
- **Evidence**:
  ```
  $ grep -rn "detect_variant\|SpeedTreeVariant" --include='*.rs' byroredux crates | grep -v version.rs
  crates/spt/src/lib.rs:58:pub use version::{detect_variant, SpeedTreeVariant};
  ```
  (single hit — the re-export itself; no caller.)
- **Impact**: None at runtime. Maintenance only: the API reads as a
  live per-game dispatch hook but is inert, which can mislead a future
  contributor into "fixing" the V5Fnv default or wiring it where the
  per-REFR `.spt` route already works without it.
- **Related**: Dimension 4 route-divergence note in the SKILL.
- **Suggested Fix**: Either wire `detect_variant` into the cell-loader
  `.spt` route as a logged sanity check (it would let Oblivion vs FO3/FNV
  be distinguished by the caller's `GameKind` when the geometry-tail
  decoder lands), or mark the API `#[allow(dead_code)]` with a
  "reserved for Phase 2 geometry-tail variant dispatch" note so its
  inert status is explicit.

---

## SPT-NEW-02: Stale crate-level module docstring claims "Phase 1.2 (recon scaffold) … ships only the version dispatch and the recon harness"

- **Severity**: LOW
- **Dimension**: Tag Dictionary (doc-rot)
- **Location**: `crates/spt/src/lib.rs:34-41`
- **Status**: NEW
- **Description**: The `lib.rs` module header reads
  *"## Status — Phase 1.2 (recon scaffold). Today this crate ships only
  the version dispatch and the recon harness. The actual byte-level
  parser (Phase 1.3) lands once the recon results … partition ≥95 % of
  the FNV corpus."* That is stale: the byte-level walker (`parser.rs`),
  the tag dictionary (`tag.rs`), the scene model (`scene.rs`), and the
  placeholder importer (`import/mod.rs`) all shipped, the ≥95 % gate is
  cleared on all three games (live this audit), and the `.spt` REFR
  route + `--tree` visualiser are wired. The crate is at Phase 1.4/1.5,
  not 1.2.
- **Evidence**: `lib.rs` exports `parse_spt`, `import_spt_scene`,
  `dispatch_tag`, `SptScene` (`lib.rs:53-58`) — i.e. the "Phase 1.3"
  parser this docstring says hasn't landed yet.
- **Impact**: Doc-rot only; misleads a reader about the subsystem's
  maturity (the inverse of the usual problem — it understates what
  shipped).
- **Related**: SKILL Phase-1 acceptance section; `docs/feature-matrix.md`
  has no SpeedTree row at all (separate gap, out of scope here).
- **Suggested Fix**: Update the `## Status` block to "Phase 1.4/1.5 —
  parameter-section walker + placeholder-billboard fallback shipped;
  geometry-tail decode (Phase 2) deferred", and drop the "ships only
  version dispatch + recon harness" sentence.

---

## SPT-NEW-03: OBND-derived `bs_bound` is computed then discarded on the cell-loader route (loose `--tree` route keeps it)

- **Severity**: LOW
- **Dimension**: Per-Game Variants & Route Divergence
- **Location**: `crates/spt/src/import/mod.rs:168-178` (computes `bs_bound`), `byroredux/src/cell_loader/references.rs:1353-1377` (`CachedNifImport` has no `bs_bound` field), `byroredux/src/scene/nif_loader.rs:1029-1037` (loose route consumes it)
- **Status**: NEW
- **Description**: `import_spt_scene` correctly produces a Y-up
  `bs_bound` AABB from TREE.OBND (the #995 fix). The **loose `--tree`
  route** consumes it: `nif_loader.rs:1029` attaches a `BSBound`
  component on the root. The **cell-loader route** does not — the
  `CachedNifImport` adapter (`references.rs:1353`) carries no `bs_bound`
  field, so the AABB is dropped. The cell path instead seeds a
  `LocalBound` *sphere* from the per-mesh `local_bound_center` /
  `local_bound_radius` (`spawn.rs:825-832`), which `import/mod.rs:374-375`
  populates correctly. So culling on the cell path works (via the
  sphere), but the more precise OBND AABB is computed and thrown away,
  and the two routes attach different bound components for the same
  placeholder.
- **Evidence**: `CachedNifImport` field list (`references.rs:1353-1377`)
  has `meshes / collisions / lights / particle_emitters / embedded_clip /
  placement_root_billboard / bsx_flags / root_flags / flame_attach_offset /
  attach_points / child_attach_connections` — no bounds field. The
  `bs_bound` local in `import_spt_scene` reaches the loose route only.
- **Impact**: Not a correctness bug — the per-mesh `LocalBound` sphere
  is valid for culling/picking. Minor: (1) wasted OBND→AABB computation
  on every cell-spawned `.spt`; (2) route divergence — a `BSBound`
  component appears on `--tree`-loaded trees but not cell-loaded ones,
  which could surprise a future consumer that keys off `BSBound`
  specifically. Blast radius is one component on tree placeholders.
- **Related**: Regression guard SPT-D4-02 / #995 (the Z-up conversion
  itself is correct; this is about where the result lands).
- **Suggested Fix**: Either (a) thread the AABB through `CachedNifImport`
  (add `bs_bound: Option<([f32;3],[f32;3])>`) and attach `BSBound` in
  `spawn.rs` for route parity; or (b) drop the `bs_bound` computation in
  `import_spt_scene` since the per-mesh `LocalBound` already covers the
  cell path and document that the loose route is the only `bs_bound`
  consumer. (a) is the parity-preserving choice and matches how
  `bsx_flags` already round-trips through the adapter.

---

## SPT-NEW-04: `billboard.rs` `BsRotateAboutUp` comment claims "local Z axis" rotation but the code locks world Y

- **Severity**: LOW
- **Dimension**: Placeholder Fallback (doc nit)
- **Location**: `byroredux/src/systems/billboard.rs:124-136`
- **Status**: NEW
- **Description**: The `BsRotateAboutUp` arm's comment says *"Rotate
  only around the billboard's local Z axis (stays in its local X-Y
  plane). We don't have the local frame here, so fall back to the
  world-up lock."* The code then sets `to_cam.y = 0.0` and uses the
  XZ-projected to-camera vector — i.e. it locks **world Y**, exactly
  like the `RotateAboutUp` arm above it. The "local Z axis" phrasing is
  inaccurate (the fallback rotates about world Y, not local Z), and is
  the kind of comment that can send a future contributor chasing a
  non-existent local-frame requirement. The approximation itself is
  acceptable and documented as such (yaw-lock is visually correct for
  tree imposters whose vertical extent is +Y).
- **Evidence**: `billboard.rs:124-136` — the `BsRotateAboutUp` branch is
  byte-for-byte the same logic as the `RotateAboutUp` branch
  (`:112-123`), both zeroing `to_cam.y`.
- **Impact**: Comment-only. The rotation behaviour is correct for the
  placeholder (textured face `-Z`, height along `+Y`, yaw-locked to
  camera). No visual defect.
- **Related**: SPT-D4-03 / #1000 (the `-Z` front-face convention this
  arc terminates on).
- **Suggested Fix**: Reword to "approximated as a world-up (Y) yaw lock;
  `BsRotateAboutUp`'s true local-Z rotation needs the node's local
  frame, which we don't carry here — visually identical for foliage
  imposters whose vertical axis is world +Y."

---

## Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 4 |
| **Total** | **4** |

### Per-dimension bill of health

| Dimension | Verdict | Notes |
|---|---|---|
| 1 — Walker Byte-Accounting | ✓ Clean | Cursor advances match dictionary kinds; 64 KiB caps on `count×stride` and string length; clean EOF + unknown-in-range bail (non-fatal); only two fatal paths (missing magic, mid-payload underflow). Corpus run: no desync. |
| 2 — Placeholder Fallback | ✓ Clean | `import_spt_scene` is `Err`-free; texture/size precedence + clamps + Z-up→Y-up + `-Z` winding all correct. One doc nit (SPT-NEW-04). |
| 3 — TREE → Billboard Wiring | ✓ Clean | TREE dual-targeted into `statics` + typed `trees` (`records/mod.rs:343-351`), same `fid` key; `Billboard` inserted on the placement root; `bsx_flags=0`/`root_flags=0`/`flame=None` synthetic defaults honoured by spawn. Mixed `.nif`+`.spt` REFRs coexist via per-REFR extension switch. |
| 4 — Per-Game Variants & Route Divergence | ✓ Mostly clean | Both routes call `parse_spt` + `import_spt_scene`; loose route uses `default()` params (no TREE metadata — documented). `detect_variant` inert (SPT-NEW-01). `bs_bound` route divergence (SPT-NEW-03). |
| 5 — Tag Dictionary | ✓ Clean | ~90 tags; fixed-size assignments match `format-notes.md` + the corpus histogram; confounders (`4096`, `5376`, …) stay `Unknown`; the 4 Oblivion tag-104 bails are documented. |
| 6 — NIFAL Material Translation | ✓ Clean | Placeholder mesh defaults `is_pbr:false`, `from_bgsm:false`, `metalness/roughness_override:None`, `emissive_source:None`, two-sided alpha-test cutout; flows through the single `translate_material` boundary via `spawn.rs` on both routes. No parallel "spt material" path. (Boundary-level checks owned by `/audit-nifal`.) |

The S1 placeholder contract — *graceful fallback over zero rendering,
never an `Err` out of the cell loader, `.spt` REFRs routed to the
SpeedTree importer* — holds end to end. All nine prior findings are
fixed and guarded. The four NEW findings are LOW maintainability items
with no runtime impact.

### Suggested next step

Run `/audit-publish docs/audits/AUDIT_SPEEDTREE_2026-06-23.md` to file
the four LOW findings as GitHub issues (domain label: `tech-debt`).
