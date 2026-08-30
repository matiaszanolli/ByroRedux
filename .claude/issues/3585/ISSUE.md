# #3585 — REN-2026-08-30-D4-06: `renderer.md` names a "HOST→AS_BUILD" barrier as what gates the ray-query consumers — no such barrier exists

**Labels**: `low,renderer,sync,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3585 --json state`.

---

- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `docs/engine/renderer.md:290–293` (per-frame order, step 10)
- **Status**: NEW — doc is wrong, code is right.
- **Description**: Step 10 reads: "Rebuild/refit the TLAS over visible BLASes
  … **HOST→AS_BUILD memory barrier before the ray-query consumers.**" The
  barrier that actually gates the ray-query consumers is
  `ACCELERATION_STRUCTURE_BUILD_KHR` / `ACCELERATION_STRUCTURE_WRITE_KHR` →
  `FRAGMENT_SHADER | COMPUTE_SHADER` / `ACCELERATION_STRUCTURE_READ_KHR`
  (`crates/renderer/src/vulkan/context/draw.rs:2688`), and it is the frame's
  *only* AS_WRITE→AS_READ barrier — it publishes the skinned BLAS refits as
  well as the TLAS build (#2931). There is no `HOST → ACCELERATION_STRUCTURE_
  BUILD_KHR` barrier anywhere in the renderer: the only HOST-source barrier on
  the AS path is `HOST_WRITE → TRANSFER_READ` on the TLAS instance staging
  buffer (`acceleration/tlas.rs:206–212`), which orders the host write against
  the staging→device-local copy, not against any ray-query consumer.
  (`grep -rn "PipelineStageFlags::HOST" crates/renderer/src/vulkan/` returns
  eleven sites; none pairs HOST with an AS-build destination.)
  The same step list also predates the depth-history copy, the #3308 depth
  capture, and the overlay's move into step 23 — the `renderer.md` counterpart
  of D4-01.
- **Evidence**: `crates/renderer/src/vulkan/context/draw.rs:2688–2697`
  (the `memory_barrier(...)` call with `ACCELERATION_STRUCTURE_BUILD_KHR` /
  `ACCELERATION_STRUCTURE_WRITE_KHR` source and
  `FRAGMENT_SHADER | COMPUTE_SHADER` / `ACCELERATION_STRUCTURE_READ_KHR`
  destination, plus its #2931 both-arms comment);
  `crates/renderer/src/vulkan/acceleration/tlas.rs:200–212` (`host_to_transfer`,
  `HOST → TRANSFER`).
- **Impact**: `/audit-severity` sets "Missing AS barrier (build → shader read)"
  at HIGH minimum, so this is the one edge in the frame graph a doc must
  describe correctly. Describing it with the wrong source stage and access
  would let a reader conclude the real barrier is redundant. Runtime behaviour
  is correct.
- **Needs RenderDoc**: no
- **Suggested Fix**: Correct step 10 to name the actual barrier
  (`AS_BUILD/AS_WRITE → FRAGMENT_SHADER|COMPUTE_SHADER/AS_READ`, emitted on
  both the build-success and build-failure arms), and mention the
  `HOST_WRITE → TRANSFER_READ` instance-staging barrier separately if it is
  worth listing at all. No code change.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D4-06

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
