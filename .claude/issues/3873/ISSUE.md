# #3873: TD4-2026-09-05-04: ~13 backtick-convention violations in the docs advisory — deliberately-absent, forward-looking and deleted names asserted as existing, one of them self-contradictory

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD4-2026-09-05-04) via `/audit-publish`, 2026-09-05. Labels: `low,doc-rot,documentation`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3873 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD4-2026-09-05-04), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `docs/engine/nifal.md:228`, `:486`; `docs/engine/exal.md:222`, `:240`, `:614`; `docs/engine/ui.md:376`; `docs/engine/ecs.md:210`; `docs/engine/game-loop.md:173`; `docs/engine/starfield-esm-roadmap.md:122`; `docs/engine/cxx-interop.md:12`; `docs/engine/watal.md:567`; `docs/engine/sandboxed-linked-mods.md:292`
- **Status**: NEW
- **Effort**: trivial (≤30 min)

**Description**
Triage of the gate's 240-symbol `docs/engine` advisory. The convention
(`_audit-common.md:277-286`) is that a backticked symbol **asserts it exists
right now**; historical, deleted, or not-yet-built names must be *italicised*.
Filtering the 240 for negation / forward-looking context yields these genuine
violations — every one is a name the sentence itself says does not exist,
carried in backticks that say it does:

| Site | Text | Class |
|---|---|---|
| `nifal.md:228` | "there is no single `translate_node` boundary to collapse them into" | never-built |
| `exal.md:614` | "No separate `translate_sun`/`SunModel`" | never-built (×2 symbols) |
| `exal.md:222` | "A **future** `translate_sun` (step 4) will fold…" | forward-looking |
| `ui.md:376` | "There is no bespoke `update_ui_texture` entry point" | never-built |
| `ecs.md:210` | "There is no `query_3_mut`/`query_4_mut`" | never-built (×2) |
| `game-loop.md:173` | "There is **no standalone `input_system`**" | never-built |
| `starfield-esm-roadmap.md:122` | "New test `parse_cydonia_cell` … **proposed, does not exist yet**" | forward-looking (self-declared!) |
| `cxx-interop.md:12` | "The unused Rust→C++ `EngineInfo` export **was removed**" | deleted |
| `nifal.md:486` | "`PrecombineMaterial` subset and field-by-field patch operation **were removed**" | deleted |
| `watal.md:567` | "does not introduce a second `WaterLod` material representation" | never-built |
| `sandboxed-linked-mods.md:292` | "The eventual schema name is deliberately left open. It represents a `ModManifest`" | forward-looking (lower confidence — sentence self-qualifies) |

**One is content rot, not just convention.** `SunModel` exists in no `.rs`
file (`grep -rn SunModel crates/ byroredux/ --include='*.rs'` → 0 hits), yet
`exal.md` names it **both ways**:

```
exal.md:240   the canonical `WeatherDataRes` + `SunModel`, not a translate site.   ← asserts it EXISTS
exal.md:614   No separate `translate_sun`/`SunModel`                               ← asserts it does NOT
```

`WeatherDataRes` does exist (`byroredux/src/env_translate.rs`, `cornell.rs`,
`boot.rs`). So `:240` pairs a real canonical resource with a phantom one and
presents the pair as the thing `weather_system` samples.

Excluded as false positives after checking context: `MyActorScript`
(`scripting.md:768` — a pedagogical placeholder in a worked example, not a repo
claim), `GreaterThan` (Papyrus event vocabulary), and the ~225 remaining
advisory entries, which are the documented noise floor (GMST/perk/actor-value
rosters, nif.xml field names, on-disk format fields, Vulkan entry points).

**Impact**
Each violation is a name an auditor can `grep` for, fail to find, and file as
missing/deleted — the exact false-finding loop the convention exists to stop.
`exal.md:240` is worse: it can produce a finding that the "canonical `SunModel`"
is unimplemented, against a design that never intended one.

**Related**
#3197 (CLOSED — the advisory's two prior structural blind spots; this is the
first triage pass since the advisory started reporting a non-zero docs list).
#3052 (CLOSED — an audit skill naming a backticked symbol that exists nowhere,
same defect one tier up). Dim 3 of this audit spot-checked these and routed the
triage here rather than filing; no overlap with TD3-2026-09-05-01…06.

**Suggested Fix**
Italicise all of them (`*translate_node*`, `*translate_sun*`, …) per the
convention. Fix `exal.md:240` separately — it is a content error, not a
formatting one: either drop `SunModel` from the sentence or mark it as the
not-yet-built model `:614` says it is.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
