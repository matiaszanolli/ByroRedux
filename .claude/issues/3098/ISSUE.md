# SUBSYS-2026-08-16-03: REFR XLOC is never parsed — every locked door opens on activation

**Issue**: #3098
**Severity**: MEDIUM
**Labels**: `medium,import-pipeline,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-16.md` (subsystem-gap sweep).

**Location**: `crates/plugin/src/esm/cell/walkers.rs`:691-870 (the REFR sub-record match — **no `b"XLOC"` arm**) · `byroredux/src/components.rs`:65-75 (`DoorTeleport` carries no lock state) · `byroredux/src/interaction.rs`:820-825 (`collect_candidates` makes every `DoorTeleport` unconditionally activatable)

## Description

REFR `XLOC` is **never parsed** — so every locked door and container in every game opens on activation.

## Evidence

```
$ grep -c 'b"XLOC"' crates/plugin/src/esm/cell/walkers.rs
0
```

Re-verified 2026-08-17. The REFR sub-record match has no `XLOC` arm, `DoorTeleport` has no lock field, and `collect_candidates` treats every `DoorTeleport` as activatable without consulting any lock state.

## Impact

Lock state is a core gameplay gate in every target game — locked doors, locked containers, key requirements, lockpick difficulty. None of it exists: the data is on disk and never read.

This is a whole-subsystem gap rather than a bug, which is why it sits in the legacy-compat sweep. It becomes player-visible immediately in the P0 door-interaction slice, where every door is openable.

## Suggested Fix

Parse `XLOC` (lock level, key `FormID`, flags) in the REFR walker, carry it onto a canonical lock component, and have `collect_candidates` / the activation path consult it.

Scope note: the full system (lockpicking minigame, key checks, difficulty) is a milestone, not a fix. The tractable first step is **parsing and storing** the data so the gate exists, even if the initial policy is "locked ⇒ not activatable".

## Related

- #3009 (RT-10 — the P2-slice modules with no runtime gate; lock state would need one too)
- `docs/engine/playable-vertical-slice.md` (the P0 door slice this affects)

## Completeness Checks
- [ ] **PARSE-FIRST**: `XLOC` is decoded and stored before any gameplay policy is built on it
- [ ] **CANONICAL-COMPONENT**: Lock state lives on a shared component, not on `DoorTeleport` alone (containers need it too)
- [ ] **SIBLING**: Container REFRs covered as well as doors
- [ ] **SCOPE-STATED**: The deferred half (lockpicking, key checks) is recorded rather than implied
- [ ] **TESTS**: A regression test parses a known locked FNV door and asserts its lock level

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3098 --json state` when live state is needed.*
