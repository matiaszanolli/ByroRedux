# CHAR-2026-09-11-D6-02: four CHARAL-scope references to `byroredux/src/boot.rs` survive the boot-module split — including one in a file edited in this very delta

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/4109
Filed: 2026-09-11 from `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`)

> Immutable snapshot of the issue as filed (TD10-001/#1156). GitHub is
> authoritative for current state: `gh issue view 4109 --json state`.

---

Reported by `/audit-character` — see `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`).

- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: all
- **Location**: `docs/feature-matrix.md:291`; `crates/core/src/character/regen.rs:145`; `crates/core/src/stealth.rs:38`; `crates/core/src/combat.rs:15`

## Description

`byroredux/src/boot.rs` no longer exists — the module is now the directory `byroredux/src/boot/` (`mod.rs`, `cli.rs`, `registries.rs`, `world.rs`, `schedule/`). Commit `f9f64cc1` ("docs(audit): re-point 30 stale path refs at the 2026-09-09 split targets") and `8175cb70` repointed `charal.md:263` from `byroredux/src/boot.rs` to `byroredux/src/boot/schedule/update.rs` — correctly — but four sites in the same CHARAL scope were missed, and three of them are Rust docstrings rather than Markdown, which is presumably why a docs-only sweep did not reach them. Each names a *specific* thing to go look at, so each sends the reader to a file that is not there:
  - `feature-matrix.md:291` — *"its required `PoolRegenConfig` resource is inserted only inside unit tests, never in `boot.rs`"* → the insertion site would be `byroredux/src/boot/world.rs`.
  - `regen.rs:145` — *"inserted unconditionally at boot (`byroredux/src/boot.rs`, `build_world`)"* → `build_world` lives in `byroredux/src/boot/world.rs:25`, and the accumulator really is inserted at `:48`. `regen.rs` is one of the six files in this sweep's delta (151 lines changed), so the stale path was carried through an edit.
  - `stealth.rs:38` — *"`ambient_ai_package_system`, registered unconditionally as a `Stage::Update` exclusive in `byroredux/src/boot.rs`"* → `byroredux/src/boot/schedule/update.rs`.
  - `combat.rs:15` — *"`combat_input_system` + `combat_damage_system` (two `Stage::Update` exclusives, `byroredux/src/boot.rs`)"* → same target.

## Impact

Low but systematic. Three of the four are the *wiring* explanation for the two CHARAL systems this dimension's matrix reports as unwired — i.e. exactly the pointers a contributor picking up "wire regen" or "wire affliction" would follow first, and the one thing they need is where the resource insertion goes. `regen.rs:145` additionally names `build_world`, which does exist, so the reader gets a half-true citation rather than an obviously-dead one. Counting only the CHARAL slice understates the pattern — the same dead path appears in `docs/engine/npc-spawn-ai-packages.md` (×4), `docs/engine/launcher.md` (×3) and `docs/engine/m47-2-design.md`; that wider cleanup is `/audit-tech-debt`'s, not this sweep's.

## Related

`f9f64cc1` (the incomplete repoint sweep); `8175cb70` (which fixed the fifth copy, `charal.md:263`); memory *Session 34/35 Layout* (the general stale-path-translation hazard)

## Suggested Fix

Repoint all four: `feature-matrix.md:291` and `regen.rs:145` → `byroredux/src/boot/world.rs`; `stealth.rs:38` and `combat.rs:15` → `byroredux/src/boot/schedule/update.rs`. Consider extending `f9f64cc1`'s sweep to `*.rs` docstrings, which is where all three code-side copies hid.

---

## Completeness Checks

- [ ] **SIBLING**: every other copy of this fact swept repo-wide (this class has repeatedly had one copy fixed and another missed — grep the symbol/number across `docs/`, `crates/`, `.claude/`, not just the named file)
- [ ] **TESTS**: where the doc states a pinned number or symbol, a test or `_audit-validate.sh` rule keeps it honest