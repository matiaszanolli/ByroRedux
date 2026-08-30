# #3747 — TD8-2026-08-30-02: two dead `pub fn` NPC-spawn compatibility shims with 11 doc comments pointing readers at them

**Labels**: bug, low, tech-debt

---

- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `byroredux/src/npc_spawn.rs` — `spawn_npc_entity` and `spawn_prebaked_npc_entity`
- **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-30.md` (`TD8-2026-08-30-02`), HEAD `64f64480`

## Description

Both are `pub fn` carrying `#[allow(dead_code)]`, and both have **zero call sites**. All
grep hits across `crates/core`, `crates/plugin`, `byroredux/src/systems`, `save_io`,
`cell_loader` and `scene.rs` are *doc or code comments* naming them, not calls
(re-verified at HEAD: 10 non-`npc_spawn.rs` hits, all prose).

Each is a ~30-line wrapper whose doc calls it a "synchronous compatibility entry point"
around the resumable job that superseded it (`byroredux/src/npc_spawn/resumable.rs`,
`NpcSpawnJob`):

```rust
let mut job = NpcSpawnJob::runtime(npc, race, game, ref_pos, ref_rot, ref_scale);
let mut budget = crate::cell_loader::FrameTimeBudget::unlimited();
match job.advance(...) {
    NpcSpawnProgress::Complete(result) => result.root,
    NpcSpawnProgress::Pending => unreachable!("an unlimited NPC spawn budget cannot yield"),
}
```

Per this dimension's rule — ByroRedux has no external consumers, so a "for compatibility"
entry point with no caller is pure rot.

## The amplifying detail is the doc surface

`spawn_npc_entity` is one of the most-cited function names in the codebase's prose (perks
stamping, AI-package collapse #2031, save round-trip #1835, idle phase-desync), all
describing behaviour that now lives in `NpcSpawnJob`. **A reader following those
references lands on a dead wrapper.** Deleting the two shims forces those comments to be
re-pointed at the live code — which is the actual value here.

Effort: small.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — sweep for other `#[allow(dead_code)] pub fn` "compatibility entry point" wrappers
- [ ] **TESTS**: A regression test pins this specific fix — `byroredux/src/npc_spawn/tests.rs` currently exercises helpers "extracted out of `spawn_npc_entity`"; re-point those to `NpcSpawnJob` rather than dropping coverage
