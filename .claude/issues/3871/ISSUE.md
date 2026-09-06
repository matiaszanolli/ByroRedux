# #3871: TD4-2026-09-05-02: six more LOC figures in `_audit-common.md`'s Binary / Gameplay rows are stale — one by 98%, one contradicted by this audit's own skill file

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD4-2026-09-05-02) via `/audit-publish`, 2026-09-05. Labels: `low,doc-rot,documentation`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3871 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD4-2026-09-05-02), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `.claude/commands/_audit-common.md:70`, `:80`, `:84`, `:85`, `:86`, `:89`
- **Status**: NEW
- **Effort**: trivial (≤30 min)

**Description**
Every hand-typed LOC figure in the Binary-modules and Gameplay-slice rows has drifted:

| Row | Claimed | Live (`wc -l`) | Drift |
|---|---|---|---|
| `:70` `byroredux/src/main.rs` | 1053 | **1267** | +20% |
| `:80` `byroredux/src/interaction.rs` | 1493 | **1626** | +9% |
| `:84` `byroredux/src/combat.rs` | 952 | **1284** | +35% |
| `:85` `byroredux/src/inventory.rs` | 1008 | **1096** | +9% |
| `:86` `byroredux/src/settings_io.rs` | 345 | **7** | **−98%** |
| `:81` `byroredux/src/studio_host.rs` | 252 | **402** | +60% (folded into TD4-…-01) |

Two are more than drift:

1. **`settings_io.rs` is a 7-line re-export shim**, not a 345-LOC module. The
   body moved to `crates/settings-io` in `e05b4a9f` (2026-08-30, *"Share one
   settings model between the launcher and the engine"*):
   ```rust
   //! Settings persistence, re-exported from `byroredux-settings-io`.
   pub(crate) use byroredux_settings_io::{load, save, SettingsPersistence, SETTINGS_PATH_ENV};
   ```
   The row still describes it as *"settings persistence behind the game menu"*
   and the Gameplay-slice header still counts it in the *"~3.8k LOC landed from
   2026-08-15 on"* total. An auditor told to audit the gameplay slice's settings
   persistence opens a shim; the real code sits in a crate on the **un-owned**
   list (`crates/settings-io`, "No owner").

2. **`main.rs`'s 1053 is contradicted inside the audit corpus itself.** This
   audit's own `audit-tech-debt/SKILL.md:239` reads *"main.rs is 1267 LOC
   (re-measured 2026-09-05, up from 1053 …)"*. The two files disagree, and the
   newer one names the older number as the superseded one.

3. **`:89` calls `byroredux/src/scene.rs` "(thin)"** — it is 1,706 total /
   **1,646 production** LOC, the third-largest module in the binary. "Thin"
   was accurate when `scene/` was split out; it now steers auditors away from a
   file that is one growth spurt from Dim 1's primary bucket.

**Impact**
Documentation-only, but these rows are the sizing signal auditors use to
allocate attention, and one of them points at a shim while the real code sits
in an un-owned crate.

**Related**
TD4-2026-09-05-01 (same file, same defect class, promoted for demonstrated
misdirection). #3744 (CLOSED — prior consolidated skill-drift sweep; covered
semantic claims only, explicitly not LOC figures).

**Suggested Fix**
Re-measure all six. For `settings_io.rs`, rewrite the row to say the body moved
to `crates/settings-io` and point the Gameplay-slice reader there. Drop "(thin)"
from the `scene.rs` row. Longer term these literals want the #2420 treatment —
name the measuring command instead of the number.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
