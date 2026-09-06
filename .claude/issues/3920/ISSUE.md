# #3920: FO3-2026-09-05-D3-01: `landscape_texture_sets` — FO3's only live TXST consumer — has no assertion in any gate

Filed from `docs/audits/AUDIT_FO3_2026-09-05.md` (FO3-2026-09-05-D3-01) via `/audit-publish`, 2026-09-05 (`/audit-suite --preset per-game-all`). Labels: `low,game:fo3,legacy-compat,terrain-exterior,test-gap,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3920 --json state`.

---

**Source**: `docs/audits/AUDIT_FO3_2026-09-05.md` (FO3-2026-09-05-D3-01), `/audit-suite --preset per-game-all`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: 3 — ESM record coverage
- **Location**: `crates/plugin/src/esm/cell/mod.rs` (`EsmCellIndex::landscape_texture_sets`);
  consumers `byroredux/src/cell_loader/terrain.rs` and `byroredux/src/cell_loader/exterior.rs`;
  the gap is in `crates/plugin/tests/parse_real_esm.rs`.
- **Status**: NEW.
- **Description**: `/audit-fo3` Dim 3 correctly redirects the auditor from REFR
  overlays to `LTEX.TNAM → TXST → TX00 → landscape_texture_sets`, calling it "the FO3
  TXST consumer worth auditing". Nothing asserts it. `parse_real_esm.rs` has five
  `texture_sets` assertions (all on `cells.texture_sets`, the raw TXST map, and all on
  FO4/Skyrim), and zero on `landscape_texture_sets` for any game. Because FO3 reaches
  TXST *only* through this path, a regression that empties the map would blank every
  FO3 exterior's terrain splat diffuse and normal with the whole ESM suite green.
- **Evidence**: measured this run against `Fallout3.esm` — `texture_sets` = 243,
  `landscape_texture_sets` = **51**, of which **51** carry a non-empty diffuse
  (e.g. `0x000357BC` → `Landscape\ChemicalWastes01.dds` + `..._N.dds`,
  `0x000009CA` → `Landscape\Asphalt02.dds` + `..._n.dds`). So the chain **works
  today**; this is a missing guard, not a defect.
  `grep -rn "landscape_texture_sets" crates/plugin byroredux` returns two definition
  sites, one merge site, and six consumer sites in `cell_loader` — no test site.
- **Impact**: Silent loss of FO3 (and Oblivion/FNV) terrain splat textures on any
  future regression in the LTEX→TXST join. Latent, not live.
- **Related**: `#3511` (the redirect that made this the FO3 TXST path of record).
- **Suggested Fix**: One assertion in `parse_rate_fo3_esm` — e.g.
  `landscape_texture_sets.len() >= 50` and "every entry has a non-empty diffuse" —
  mirroring how the PROJ/PGRE chain is pinned by form id.

---

### Dimension 4 — FO3 cell loading end-to-end

**Verified clean. No new findings.** Static review of the delta (`spawn/mesh_instance.rs`
+575, `unload.rs` +229, `transition.rs` +167, `lod_bands.rs` +285, `object_lod.rs` +127,
`placement_lod.rs` +151, `water.rs` +107):

- **Capital Wasteland is not FNV-hardcoded.** `cell_loader/exterior.rs`'s worldspace
  preference list carries `Wasteland` (FO3) alongside `WastelandNV` (FNV), `Tamriel`
  and `Skyrim`, and `select_worldspace_key` prefers a grid-containing worldspace before
  falling back to it (the #444 fix). No FNV worldspace name, origin coord or default
  grid leaks into a shared path.
- **#3502's object-LOD coarsening stayed object-only**, as the checklist requires:
  `coarsen_to_available: true` appears only in `cell_loader/object_lod.rs`;
  `cell_loader/terrain_lod.rs` sets it `false`, so terrain still subdivides.
  `LodBandLadder::for_object_game(GameKind::Fallout3NV)` resolves and
  `object_lod_scheme(Fallout3NV) == FalloutLegacyBlocks`.
- **`placement_lod_supported` is still Oblivion-only** and pinned by
  `assert!(!placement_lod_supported(GameKind::Fallout3NV))` — the #2086 boundary the
  skill file asks not to re-litigate holds.
- `legacy_landscape_lod_supported(Fallout3NV)` true / `combined_lod_supported(Fallout3NV)`
  false — the FO3 band split is intact.
- **Structural note (not a finding)**: `GameKind::Fallout3NV` still collapses FO3 and FNV
  into one enum variant (`crates/plugin/src/esm/reader.rs`), so no cell-loader code can
  express an FO3-only behaviour. The 2026-08-30 report raised this; it is unchanged and
  remains correct-by-design rather than a defect.

*Not covered this run*: no live cell load or GPU bench (standing rule against launching
the engine). The exterior open item remains "fresh GPU bench pending (R6a-stale-15)",
unchanged.

---

### Dimension 5 — FO3 collision import

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other block parsers, other games)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `translate_material` / `Material::resolve_pbr` / the emitter params, per-game logic stays at the NIFAL parser→`Material` boundary
