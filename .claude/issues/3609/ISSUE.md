# #3609 — REN-2026-08-30-D16-03: `procedural-volumetric-fog.md` still ships `froxel_xy_divisor` default 4 and a four-volume footprint

**Labels**: `low,renderer,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3609 --json state`.

---

- **Severity**: Low
- **Dimension**: Volumetrics
- **Location**: `docs/engine/procedural-volumetric-fog.md:47`, `:284`, `:294`, `:306`, `:327`
- **Status**: OPEN — new
- **Description**: The M55 design spec — the doc `ROADMAP.md:809` names as the
  authoritative M55 spec — states four things the live code contradicts:
  (a) *"Defaults are one froxel per 4×4 render pixels"* (`:47`);
  (b) `--froxel-xy-divisor <2..32>   default 4` (`:284`), repeated in the
  worked example (`:294`); (c) the measurement table marks the divisor-4 row
  `214×120×64` as "default" (`:306`); (d) *"At the default 214×120×64 grid,
  the **four** RGBA16F fields (raw, integrated, chemistry, velocity) plus the
  R32F emissive-history sidecar consume about 56 MiB per frame slot"*
  (`:327`) — the live set is **five** RGBA16F (it omits
  `combustion_optical_volumes`) plus the R32F, i.e. 44 B/froxel/slot, not the
  36 B that yields 56 MiB.
- **Evidence**:
  - `crates/renderer/src/vulkan/upscaling.rs:135` — `froxel_xy_divisor: 8`
  - `crates/renderer/src/vulkan/volumetrics.rs:600`–`606` — `FROXEL_VOLUMES_PER_SLOT: usize = 6`, `FROXEL_BYTES_PER_SLOT: u64 = 44`
  - `crates/renderer/src/vulkan/volumetrics.rs:1029`–`1039` — `combustion_optical` volume, `COMBUSTION_FIELD_FORMAT` (RGBA16F), the fifth RGBA16F the doc does not list
  - `byroredux/src/cli_args.rs:97` — the CLI default *is* `VolumetricsConfig::default().froxel_xy_divisor`, so the shipped default is 8 and the doc's "default 4" is wrong on both the flag and the grid
  - `docs/engine/memory-budget.md:235`, `:238`, `:250` — the *other* doc states divisor 8 / six volumes / 44 B and is test-pinned by `volumetrics.rs:3661`
- **Impact**: The two volumetrics docs now disagree with each other. The
  memory-budget one is right (and enforced); the design spec is wrong and
  unenforced, so it is the one a reader hits first from the ROADMAP link.
  Anyone sizing the grid or reproducing the measurement table from `:294`
  will silently run at 4× the shipped froxel count.
- **Suggested Fix**: Update `:47`, `:284`, `:294` to 8; re-label the table's
  "default" marker onto the divisor-8 row (keeping the divisor-4 measurements
  as historical rows, per the doc's own "keep rows even when a path is not
  implemented" rule); rewrite `:327` for five RGBA16F + one R32F = 44 B/froxel
  and re-derive the per-slot MiB. Extend
  `froxel_grid_cost_matches_the_memory_budget_doc` to `include_str!` this doc
  too, so both ledgers are pinned by the one test.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D16-03

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
