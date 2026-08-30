# #3566 — REN-2026-08-30-D5-02: the mesh-side `StagingPool` — a second 128 MB retained `CpuToGpu` pool — has no ledger row, and #3298's 64 MiB chunking made its retention permanent

**Labels**: `medium,renderer,memory,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3566 --json state`.

---

- **Severity**: Medium
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/mesh.rs:403` (`geometry_staging_pool`),
  `crates/renderer/src/vulkan/buffer.rs:53` (`DEFAULT_STAGING_BUDGET_BYTES`),
  `crates/renderer/src/mesh.rs:55` (`GEOMETRY_REBUILD_CHUNK_BYTES`);
  `docs/engine/memory-budget.md:422` + the VRAM roll-up at `:495-515`
- **Status**: Open
- **Description**: `memory-budget.md` mentions a staging pool exactly once — a
  `Staging pool cap | 128 MB` row inside the **Texture Registry** section — and the
  VRAM Rough Budget table has no staging line at all. There are two live pools:
  `TextureRegistry::staging_pool` (`texture_registry.rs:526`) and
  `MeshRegistry::geometry_staging_pool` (`mesh.rs:919, 1195, 1509`). Both are
  built with `StagingPool::new`, i.e. `DEFAULT_STAGING_BUDGET_BYTES` = 128 MB of
  **retained** `MemoryLocation::CpuToGpu` capacity each, and there is no
  production `trim_to(0)` caller anywhere in the workspace — the only shrink is
  `release`'s auto-trim back to budget.
  #3298 changed the mesh pool's steady state rather than its cap. The pre-#3298
  atomic path staged the *whole* vertex buffer in one `acquire`, so a large scene
  released a single 600 MiB entry that immediately blew the budget and was
  evicted largest-first, leaving the pool near empty. The chunked path stages
  `GEOMETRY_REBUILD_CHUNK_BYTES` = 64 MiB at a time, so after any chunked rebuild
  the pool holds one ~64 MiB vertex-chunk entry plus one 64 MiB index-chunk entry
  — 134,217,672 B against a 134,217,728 B budget, i.e. exactly at the ceiling and
  therefore never trimmed. That is up to ~128 MB of resident host-visible memory
  (VRAM on a ReBAR-enabled 4070 Ti) that the ledger does not carry for a subsystem
  the ledger does describe.
- **Evidence**:
  - `buffer.rs:53`: `pub const DEFAULT_STAGING_BUDGET_BYTES: vk::DeviceSize = 128 * 1024 * 1024;`
  - `buffer.rs:159-165`: `acquire` allocates `MemoryLocation::CpuToGpu`.
  - `buffer.rs:205-215`: `release` auto-trims only when
    `total_capacity() > budget_bytes`.
  - `grep -rn "trim_to" crates/renderer/src` → only `buffer.rs` internals
    (`:214`, `:254`, `:259` in `destroy`); no production caller.
  - `mesh.rs:1512-1513`: `GEOMETRY_REBUILD_CHUNK_BYTES / size_of::<Vertex>()` and
    `/ size_of::<u32>()` — the two chunk sizes released back into the pool at
    `buffer.rs:1577` (`staging.release_to(staging_pool, capacity)`).
- **Impact**: A per-session ~128 MB residency that no budget row accounts for, on
  top of the two-generation geometry peak. It is not a leak (bounded, freed at
  `destroy_all`), but the "Estimated total ~1.74 GB / ~3.4 GB at native 4K"
  roll-up is computed without it, and the page is the stated authority for the
  `< 4 GB` target.
- **Suggested Fix**: Add a Staging Pools section (or a row in the VRAM table)
  naming both pools, `DEFAULT_STAGING_BUDGET_BYTES`, and the
  `2 × GEOMETRY_REBUILD_CHUNK_BYTES` floor the chunked rebuild now parks in the
  mesh pool. If the 128 MB retention is not wanted on the geometry side,
  `StagingPool::with_budget` already exists — but that is a policy call, not a
  doc fix, and should be measured first.
- **Dedup note**: NOT #3463 — that issue is about the vertex/index *pool* row not
  carrying #3298's two-generation device-local peak. This is the host-visible
  staging side, a different allocation class and a different (missing) row.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D5-02

## Dedup cross-reference

NOT **#3463** — that issue is the vertex/index *pool* row missing #3298's two-generation
device-local peak. This is the host-visible `StagingPool` side: a different allocation
class and a different (missing) row.


## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
