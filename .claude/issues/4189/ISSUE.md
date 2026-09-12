# PERF-D1-2026-09-11-01: Six AI-locomotion systems clone `NavPath` on write-back when the source is already dead

Labels: medium,performance,ai,bug

**Description**: All six AI-locomotion systems (`follow`, `escort`, `guard`, `patrol`, `travel`, `wander`) share a Pass-1/Pass-2 shape. Pass 2 writes `d.nav_path` into `NavPath` component storage via `path.clone()` even though `scratch.decisions` (which owns it) is fully cleared and repopulated next frame and nothing reads `d.nav_path` again after this loop. The input side of the same functions already eliminated the analogous clone via `mem::take` with an explicit comment ("hand the list over instead of cloning it every tick") — the write-back side was not given the same treatment.

**Evidence**:
`follow.rs:267` documents the already-fixed input-side elimination (confirmed: `d.nav_path = Some(NavPath {...`); the write-back loop at `follow.rs:311` (and the five siblings `escort.rs:442`, `guard.rs:299`, `patrol.rs:230`, `travel.rs:359`, `wander.rs:400`) still does `nq.insert(d.entity, path.clone())`.

**Impact**: One `VecDeque<Vec3>` heap allocation + element-wise copy per actively-pathing entity per frame, recurring every tick for the path's lifetime (not just on repath), across six systems, scaling with concurrently-active NPC AI in a populated exterior cell.

**Related**: Same class as the already-fixed input-side clone in the same six files.

**Suggested Fix**: `for d in &mut scratch.decisions { match d.nav_path.take() { Some(path) => nq.insert(d.entity, path), None => nq.remove(d.entity) } }` — behavior-preserving since `scratch.decisions` is rebuilt from scratch next frame with no intervening read.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
