# SAFE-2026-08-27-04: `failed_skin_slots`' safety rationale claims `EntityId` is generational — it is a bare `u32`

- **Issue**: [#3375](https://github.com/matiaszanolli/ByroRedux/issues/3375)
- **Finding ID**: `SAFE-2026-08-27-04`
- **Source report**: `docs/audits/AUDIT_SAFETY_2026-08-27.md`
- **Audit suite preset**: streaming-deep (2026-08-27)
- **Labels**: `low,safety,doc-rot,documentation`

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3375 --json state`.

---

- **Severity**: LOW
- **Dimension**: 2 (memory corruption / UB — documentation of a safety premise)
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:1488-1490`
- **Status**: NEW
- **Description**: The doc comment justifying why a stale
  `failed_skin_slots` entry is harmless reads:

  > `EntityId` is generational so an entry can't poison a re-issued id. See #900.

  `EntityId` is `pub type EntityId = u32` (`crates/core/src/ecs/storage.rs:10`)
  — a plain integer with no generation field. The *conclusion* is still correct,
  but for a completely different reason: `World::spawn` is monotonic and
  `World::despawn` never reclaims an id, which `crates/core/src/ecs/world.rs:140-143`
  states explicitly ("Entity IDs are NOT reclaimed … Reuse without generational
  tagging would cause silent component-data corruption"). The comment therefore
  asserts a property the ECS deliberately does **not** have, and asserts it as
  the load-bearing reason an entity-keyed cache is sound.

  This matters more than ordinary doc rot because several renderer caches are
  keyed on `EntityId` (`skin_slots`, `morph_slots`, `failed_skin_slots`,
  `failed_skin_blas`, `pending_*_unload_victims`) and a reader who trusts this
  line will conclude id recycling is already handled. It is not — the invariant
  is "ids are never recycled", and that invariant lives in `world.rs`, not here.
- **Evidence**:
  ```rust
  // crates/core/src/ecs/storage.rs:10
  pub type EntityId = u32;

  // crates/core/src/ecs/world.rs:110-117 — monotonic, checked_add, never recycled
  pub fn spawn(&mut self) -> EntityId {
      let id = self.next_entity;
      self.next_entity = self.next_entity.checked_add(1)
          .unwrap_or_else(|| panic!("World::spawn overflowed EntityId (u32::MAX reached)"));
      id
  }
  ```
- **Impact**: Documentation only — no runtime effect today. The risk is a future
  change that introduces id recycling (a free list, a generational rework) and
  passes review because this comment says the renderer caches already tolerate it.
- **Related**: #900, #372 (the issue that established the never-reclaim rule), #36
- **Suggested Fix**: Replace "is generational" with "is never recycled —
  `World::spawn` is monotonic and `despawn` does not reclaim (#372)", and
  cross-reference `crates/core/src/ecs/world.rs:140-143` as the owning invariant.

---
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files

---
_Filed by `/audit-publish` from `docs/audits/AUDIT_SAFETY_2026-08-27.md` (audit-suite preset: streaming-deep). Finding ID: `SAFE-2026-08-27-04`._
