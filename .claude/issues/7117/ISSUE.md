# #7117 — ESM-D7-05: `docs/engine/index.md` / `architecture.md:36` list `PluginManifest`/`DataStore` with no #5609 caveat

Labels: low,esm-plugin,documentation,doc-rot
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D7-05)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D7-05**

- **Severity**: LOW (doc only; weak: it is a type-name lookup table, not a statement that this is the load path)
- **Dimension**: 7 (Redux-native tier rot)
- **Record / Sub-record**: none
- **Location**: `docs/engine/index.md:59` (Plugin Loading row: "`PluginManifest`, `DataStore`, `DependencyResolver`, Form ID system, ESM parser, conflict resolution") and `:192` (Quick Reference: "Plugin manifests | `byroredux_plugin::PluginManifest`"); `docs/engine/architecture.md:36` ("manifests, conflict resolution, legacy bridge") has no caveat either, although `:297` carries the banner
- **Status**: NEW (#5609's banners went on `lib.rs`, `plugin-loading.md`, `architecture.md:297` and ROADMAP only)
- **Description**: a reader of the index sees the dormant tier listed in the same row and table as `parse_esm` / `parse_esm_cells`, the live path. The linked `plugin-loading.md` opens with the banner, so the reader is one click from the truth, which is why this is only a nit.
- **Suggested fix**: append "(designed; no production caller, #5609)" to those two rows and to `architecture.md:36`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
