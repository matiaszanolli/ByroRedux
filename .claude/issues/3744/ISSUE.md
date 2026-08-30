# #3744 — TD4-2026-08-30 (consolidated): 12 stale premises across 9 of the 30 audit-skill corpus files — two of them would have hidden a real HIGH, one manufactures a phantom CRITICAL

**Labels**: documentation, medium, tech-debt, doc-rot

---

- **Severity**: MEDIUM (highest of the 12 folded items; 3 MEDIUM + 9 LOW)
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `.claude/commands/audit-*/SKILL.md` — 9 of the 30 corpus files
- **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-30.md` (`TD4-2026-08-30-01` … `-12`), HEAD `64f64480`

> **Consolidated filing.** This is one issue for 12 confirmed drift items across 9 files
> because the defect is *systemic*, not per-file: nothing in the toolchain checks whether
> a skill's **claims** are still true. Filing 12 near-identical issues would obscure that.
> Per-report skill drift found by sibling audits is correctly filed against those reports'
> own subjects; the corpus **as a whole** is this report's audited subject matter.

## Why this is fileable at all

`_audit-validate.sh` was itself born as a `TD7-*` finding, and it works — it checks that
every backticked path and symbol *resolves*. It came back **GREEN** here: 0 STALE across
2 305 refs / 99 files. Dimension-count claims are all in sync. GPU struct sizes across the
corpus are all in sync (160 / 368 / 432 B). The crate→owner map covers all 25 live crates.

**Nothing checks whether the sentence around a resolving symbol is still true.** All 12
items below have symbols that resolve and paths that exist; the drift is purely semantic.

**Two of them would have caused an auditor to SKIP the check that finds a real
HIGH/MEDIUM, and one manufactures a phantom CRITICAL against correct code.**

## The 12 confirmed items

### MEDIUM — these invert the auditor's task

**1. `audit-save/SKILL.md` (Dim 1)** — asserts `ReferenceEnableState` *"has no consumer
anywhere in cell_loader/streaming yet (`is_enabled` is called only from its own test
module)"* and instructs *"don't raise it as a save finding"*.
FALSE since `265f0c9b` (Fix #3256, Fix #3278). Live consumers: `byroredux/src/cell_loader/spawn.rs`
(gates REFR spawn, plus a log line) and a dedicated regression file
`byroredux/src/cell_loader/reference_enable_gate_tests.rs`. **That exact line would have
hidden a HIGH** — it steers an auditor away from verifying the round-trip of a component
that now has observable live effect. Fix: reframe as "wired since #3278; verify the spawn
gate still reads it after a load-apply".

**2. `audit-character/SKILL.md` (Dim 5)** — *"`GameKind::Fallout3NV` resolves to the FNV
ruleset for both FO3 and FNV … if any actor-general coefficient differs, the collapse is
wrong and every FO3 NPC is mis-statted."*
The collapse **no longer exists**. `crates/plugin/src/esm/records/mod.rs`
(`character_rules_profile`) splits on HEDR version: `GameKind::Fallout3NV if hedr_version
< 1.0 => CharacterRulesProfile::FALLOUT3`, else `FALLOUT_NEW_VEGAS`. The rulesets are
demonstrably distinct — `crates/core/src/character/profile.rs` dispatches `fallout3_ruleset`
vs `falloutnv_ruleset`, and its pinning test asserts `fo3_health.evaluate(5.0, 2.0) == 210.0`
vs `fnv_health … == 205.0`, plus `SkillSet::FALLOUT3` vs `SkillSet::FALLOUT_NV`. **An
auditor following this bullet files "every FO3 NPC is mis-statted" as a CRITICAL against
correct code.** Fix: delete the bullet.

**3. `audit-esm/SKILL.md` (Dim 5)** — *"`parse_refr_group` … still recurses on
`reader.group_content_end(&sub)` with no depth counter … if so it is the live regression
case for this bullet, not a hypothetical."*
Closed by `fa511bbf` (Fix #3503, 2026-08-29). `crates/plugin/src/esm/cell/walkers.rs` now
splits into `parse_refr_group` → `parse_refr_group_inner(…, 0)`, routes `sub_end` through
`reader.bounded_group_content_end(&sub, depth, "parse_refr_group")`, and threads
`depth + 1`. The "if so" hedge saves it from hard misdirection, but it burns auditor time
and invites a duplicate filing.

### LOW

**4. `audit-ecs/SKILL.md`** — cites *"the `write_lazy!` macro (5 color-target arms) … + `ensure_subtree_cache`"*.
Both were **removed by #2399** (`f46fcfd8`, content-determined lock order in animation
channel apply) — the macro was the *cause* of the lock-order defect, so removal was
deliberate, not DRY-undo drift. Only two residual grep hits remain and both are historical
comments in `byroredux/src/systems/animation.rs`. `write_root_motion` and
`apply_bool_channels` survive. Reading the bullet literally, an auditor files exactly the
DRY-undo finding the bullet invites — against an intentional fix.

**5. `audit-ecs/SKILL.md`** — says the boot guard runs
`debug_assert_eq!(scheduler.access_report().undeclared_parallel_count(), 0)`.
`byroredux/src/boot.rs` uses **release-level `assert_eq!`** for all three guards
(`undeclared_parallel_count`, `known_conflict_count`, `unknown_pair_count`), with an
explicit comment: *"Keep these as release assertions … a release-only divergence must not
ship without the proof"* (#2690). An auditor trusting the skill flags the guard as
debug-only — a finding for a property that was deliberately **strengthened**.

**6. `audit-scripting/SKILL.md`** — describes `index.rs::base_record_script_instance` as
checking *"ACTI/CONT/NPC/CREA base records in order, then (#2189) the item family"*, then
instructs "verify the record types covered match the VMAD-bearing set".
`crates/plugin/src/esm/records/index.rs` has **seven** arms — the five listed plus, per
#2663, `self.cells.statics` (STAT/MSTT/FURN/DOOR/LIGH/FLOR/IDLM/BNDS/ADDN/TACT) and
`self.terminals` (FO4 ships 207 VMAD-bearing TERM records). Both have guards
(`base_record_script_instance_resolves_a_statics_familys_vmad`, `…_resolves_a_terminals_vmad`).
An auditor verifying "covered == VMAD-bearing set" against the skill's list re-derives
#2663 as a fresh gap.

**7. `audit-scripting/SKILL.md`** — *"this increment deliberately doesn't trace a local
back to the property it aliases, so a local receiver must decline via
`scope.quest_locals`/`scope.decl_locals`, not silently resolve."*
`crates/scripting/src/translate/effects.rs` now carries a third map,
`object_locals: HashMap<String, ObjectRef>`, populated from `Binding::Object(via)` and
consulted first thing in `receiver_object` — so an object-typed local **does** resolve
(introduced by `0ff8612b`, MQ101 cinematic effects). The map is absent from the entire
`.claude/commands/` corpus (`grep object_locals .claude/commands/` → **0 hits**). The same
skill restates the drift a second time in its "still declines a local-variable receiver"
claim.

**8. `audit-speedtree/SKILL.md`** — Dim 3 and Dim 5 contradict each other **in the same
file**. Dim 3: *"every path clamped to `[16, 8192]` (#1001/#1002)."* Dim 5:
*"Billboard extent clamping is `Option`-returning, **not** `f32::clamp` … Regression = a
bare `clamp` reinstated on any tier"* (#3529). The code
(`crates/spt/src/import/mod.rs`, `clamp_billboard_extent`) matches Dim 5:
`value.is_finite().then(|| value.abs().clamp(MIN…, MAX…))`. An auditor working the Dim 3
checklist top-down reads the stale line first, accepts a bare `f32::clamp` as conforming,
and **misses precisely the NaN regression #3529 fixed**. Fix: make Dim 3 defer to Dim 5.

**9. `audit-speedtree/SKILL.md`** — *"only SPT-NEW-07 (… **#1822**) remains open."*
`gh issue view 1822` → **CLOSED** (fixed by `19813460`, Fix #3531 — reject a zero-length
13005 candidate). This is the skill's Phase-1 **orientation** step, so the error lands
before any dimension runs: the auditor starts with a false open-item list.

**10. `audit-starfield/SKILL.md`** — *"The residual truncation tail in Meshes01/MeshesPatch
is tracked at **#746/#747** — confirm it has not grown."*
Both **CLOSED**, and neither is a truncation-tail tracker — they are the *version-gating*
defects (`bsver == 155`) whose fix reduced the tail. The live residual-truncation tracker
is the `bsweakreferencenode_2byte_gap` line of work (6/29,849 in MeshesPatch.ba2). An
auditor "confirming it has not grown" against two closed shader-gating issues learns
nothing.

**11. `audit-renderer/SKILL.md`** — prescribes
`grep -rl "struct GpuInstance" crates/renderer/shaders/` → *"5 declaration sites"*.
The command **as written returns 6** — it also matches
`crates/renderer/shaders/skin_vertices.comp`, whose hit is a *comment* saying that shader
has no `struct GpuInstance`. The substantive claim (5 real declarations) is correct; the
*recipe* is not, and it guards the codebase's single highest-stated lockstep risk
(`feedback_shader_struct_sync.md`, severity floor HIGH). Fix: anchor to a declaration —
`grep -rlE '^struct GpuInstance'`.

**12. `audit-tech-debt/SKILL.md` (Dim 2) — the file this audit ran from** — *"Z-up → Y-up
coordinate flips reimplemented outside the canonical homes
(`crates/nif/src/import/coord.rs`, `crates/nif/src/anim/coord.rs`) — any other call site
is a leak."*
Since **#1044 / TD3-002** the single source of truth is `crates/core/src/math/coord.rs`
(`zup_to_yup_pos` / `zup_to_yup_quat_wxyz`). Both named files say so themselves —
`crates/nif/src/anim/coord.rs` is now a 14-line `pub use` re-export whose own header reads
*"Pre-#1044 / TD3-002 this file owned a divergent copy … The single source of truth now
lives in `byroredux_core::math::coord`"*. An auditor applying the bullet as written flags
all ~15 **correct** call sites as leaks — the exact inversion of the truth, on a
consolidation that is fully converged.

## Additional confirmed drift folded in (found by concurrent sibling audits)

Not separately filed; listed so a single sweep closes them:
- `audit-renderer` Dim 1 ×2 — a deleted `build_blas_for_mesh` entry point and a "no
  recovery path exists" premise closed 2026-08-16 (**tracked at #3576**).
- `audit-speedtree` Dim 2 ×1 — a "vanilla Oblivion ships MODB-only" premise falsified at
  142/142 by corpus measurement.
- **Per-game skills nominate block/shape types their game ships ZERO of**: FNV 2, FO3 1,
  FO4 3, Starfield 2, Oblivion 10. Each is an auditor sent to check a path the title never
  authors.

**Corpus-wide this cycle: 15 confirmed drift items across 9 of 30 files (30 %).**

## Impact

Not a runtime defect — an *audit-integrity* defect, and the audit corpus is how this
project finds runtime defects. Six sibling audits this cycle independently reported that
their own skill file carries a stale checklist premise. `/audit-renderer` documented the
same mechanism producing a **positive false statement in the audit record**: its SKILL's
"recast, don't re-report" instruction turned a closed-on-2026-08-16 gap into a
"re-verified as unchanged" line in the 2026-08-27 report.

## Suggested Fix

1. Correct the 12 lines above (each is one paragraph; items 1, 2, 8 and 12 are the
   load-bearing ones).
2. **Structural follow-up worth its own issue**: `_audit-validate.sh` proves the
   *resolvability* leg is automatable. Consider a second gate for the *claim* leg —
   at minimum, mechanically verify every `#NNNN` a SKILL calls "open" is actually open
   (items 3, 9, 10 are all this one check), and re-run each SKILL's own prescribed grep
   recipes and diff the hit count against the stated expected answer (item 11 is exactly
   that).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the remaining 21 corpus files — the 12 above are what one static pass found, not a proof of absence
- [ ] **TESTS**: A regression test pins this specific fix — the `#NNNN`-is-open check and the prescribed-grep-returns-stated-count check are both scriptable
