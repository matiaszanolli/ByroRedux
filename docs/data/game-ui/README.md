# M48 game-UI data (Phase 0, #6643)

Checked-in measurements for [`docs/prd/game-ui-integration.md`](../../prd/game-ui-integration.md)
§9.1 and the design in [`docs/engine/ui.md`](../../engine/ui.md#menu-registry-stack-and-path-access-m48-design-phase-0).
All files are UTF-8 TSV with a header row and a `source` (or `evidence`) column per row naming the tool and
arguments, the archive entry, or a pinned URL that produced it. Measured 2026-11-08 on this machine's installs.
`unknown` and `undocumented` are results, not gaps to fill by guessing.

| File | Requirement | Produced by |
|---|---|---|
| `menu-inventory-{oblivion,fo3,fnv,skyrim-se,fallout4}.tsv` | GUI-REG-006 | `crates/plugin/examples/menu_gmst_census.rs inventory`, `crates/bsa/examples/archive_list_all.rs`, the UESP wiki dumps; the last four columns (`target_surface`, `fallback_surface`, `surface_basis`, `host_classes`) are derived by `scripts/game-ui-data/join_surfaces.py` from the host classes |
| `menu-codes-{fnv-fo3,oblivion}.tsv` | GUI-REG-006 | xNVSE / xOBSE / xEdit sources pinned in each row |
| `menu-flags-{skyrim,fallout4,fnv-fo3,oblivion}.tsv` | GUI-REG-003 / -004 | SKSE64, F4SE, CommonLibSSE, commonlibf4 sources pinned in each row |
| `host-classes-{skyrim,fallout4}.tsv` | GUI-HOST-001 / -003 | `crates/ui/examples/host_call_sites.rs` plus a per-method review (the `evidence`, `class`, `reason` columns); the class is a *target* |
| `host-diff-skyrim-le-vs-se.tsv` | PRD §7.1 Skyrim LE | the in-tree AVM1 sweep over both installs |
| `gfx-extensions-{skyrim-se,fallout4,starfield}.tsv` | GUI-GFX-003 | `crates/ui/examples/gfx_extension_census.rs` |
| `hud-feeds-{skyrim-se,fallout4}.tsv` | GUI-HUD-003 / -005 | `crates/ui/examples/hud_feed_paths.rs`, the executables' string tables, pinned external sources |
| `menu-string-refs-{fo3,fnv}.tsv` | GUI-FONT-005 | `crates/plugin/examples/menu_gmst_census.rs strings` plus the executables' setting tables |
| `native-page-strings.tsv` | GUI-PAGE-005 | `scripts/game-ui-data/generators/menu-inventory-and-strings/page_strings.py` over the native page sources |
| `race-menu-caller-wait.tsv` | GUI-PAGE-002 | `crates/pex/examples/pex_find_call.rs`, `pex_dump_fn`, `obscript_tools` |

## Limits

- **Re-running.** The Rust tools above run from the repository against the game installs and state their command
  line in their module doc. `scripts/game-ui-data/join_surfaces.py` is idempotent. The scripts under
  `scripts/game-ui-data/generators/` are the generators the Phase 0 research used; they read intermediate files
  (executable string tables, fetched source files) that are not checked in, so they are **provenance, not a
  pipeline**: the pinned URLs / commits in the `source` columns are what to re-fetch.
- **Targets, not behaviour.** `host-classes-*` and `target_surface` say what the engine should do under PRD Q1
  and §3.2. Nothing is served yet.
- **Inferred rows.** Fallout 4 `engine_fed = yes` rows in `hud-feeds-fallout4.tsv` are inferred from "no movie
  writer plus an executable string", never observed; `host-classes-*` rows with a `?` note took the more
  conservative class; Skyrim / Fallout 4 menu-to-movie joins are by file name only.
- **Not measured.** Skyrim LE flags and inventory, Starfield menus, FNV default setting text (packed executable),
  what the original HUD draws under a modal. See "Open at the Phase 0 exit" in the PRD.
