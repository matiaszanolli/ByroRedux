# Legacy Compatibility Audit — 2026-08-24

**Base:** `048a8bd8` · **Type:** full `/audit-legacy-compat` sweep, all 7 dimensions,
run solo (no sub-agent fan-out) per explicit dispatch instruction.

## Scope

All seven dimensions: coordinate-system correctness (Z-up→Y-up), NIFAL
cross-layer mapping shape, the material translation boundary, PHYSAL's source
axis, EXAL/WATAL, per-game translation-survey patterns (A/B/C), and subsystem
coverage vs the legacy engines.

**Delta weighting.** 193 commits since the prior sweep
(`docs/audits/AUDIT_LEGACY_COMPAT_2026-08-20.md`, base `bb0b92f2`), the large
majority continuing the session-70/71 WATAL water push plus a same-day
(2026-08-24) grab-bag covering the actor-value key space, RACE.DATA
Magicka/Stamina, the scheduler's WindField access declaration, and save/load
notifications. This sweep's first task was therefore verification: re-trace
every one of the prior sweep's six findings against HEAD, then look for what
193 commits of continuous water/character work might have newly broken or
newly left undocumented.

**Source-availability statement.**

| Reference | Status |
|---|---|
| Gamebryo 2.3 source (`/media/matias/Respaldo 2TB/…/Gamebryo_2.3/`) | Not consulted this sweep — no finding below turned on runtime semantics only the 2.3 source could settle. |
| `/mnt/data/src/reference/nifxml/nif.xml` | Not needed this sweep (no new NIF-field question arose). |
| Vanilla masters (`Oblivion.esm`, `Fallout3.esm`, `FalloutNV.esm`, `Skyrim.esm`) | Referenced via the prior sweep's already-captured byte census and today's in-tree byte-verified doc comments (`crates/plugin/src/esm/records/actor/mod.rs:1206-1211`, "verified byte-for-byte against vanilla `Skyrim.esm` … 2026-08-12"); not independently re-scanned this sweep. |

**Method.** Every claimed single-boundary contract was re-traced to its
callers with fresh greps against HEAD (not assumed from the prior report).
Each of the prior sweep's six findings was individually re-verified by reading
the current code at its cited location. Deduplicated against 200 issues
cached at `/tmp/audit/issues.json` and against `docs/audits/`. No `cargo`
command was run (workspace build is blocked by an unrelated E0004 in
`crates/scripting/examples/fragment_coverage.rs`, owned by `/audit-scripting`
per the dispatch note). No source file, game file, or GitHub issue was
modified.

## Executive Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 1 |
| **Total** | **1** |

**All six findings from the 2026-08-20 sweep are fixed and verified still
fixed at HEAD.** This is an unusually clean result for a "full comprehensive"
run, and it is not because the dimensions went unchecked — every claimed
boundary was re-traced, not carried over from the prior report's word. The
codebase's fix velocity on this specific area has been remarkable: three of
the six findings (LC-D5-01, LC-D6-01, LC-D6-03) were fixed same-week, and two
more (LC-D2-01, LC-D5-02) were fixed with an explicit contract update to
`docs/engine/watal.md` §3 rather than a patch that leaves the spec wrong. The
one new finding is a **documentation** issue, not a code issue: the
per-game-translation-survey's headline example and one of its concrete
per-record claims have gone stale relative to fixes that landed after it was
written, and Dimension 6 of this very skill directs auditors to trust that
document as the reference for cross-game pattern findings.

### Per-dimension finding counts (every dimension enumerated)

| Dimension | CRIT | HIGH | MED | LOW | Findings |
|---|---:|---:|---:|---:|---|
| 1. Coordinate-system correctness (Z-up→Y-up) | 0 | 0 | 0 | 0 | **none — clean** |
| 2. NIFAL — canonical NIF→ECS mapping shape | 0 | 0 | 0 | 0 | **none — clean** (LC-D2-01 verified fixed) |
| 3. Material translation boundary | 0 | 0 | 0 | 0 | **none — clean** |
| 4. PHYSAL — per-game Havok → solver (source axis) | 0 | 0 | 0 | 0 | **none — clean** |
| 5. EXAL / WATAL — exterior + water → renderer & solver | 0 | 0 | 0 | 0 | **none — clean** (LC-D5-01, LC-D5-02 verified fixed) |
| 6. Per-game translation-survey gaps (Pattern A/B/C) | 0 | 0 | 0 | 1 | LC-D6-2026-08-24-01 (LC-D6-01 verified fixed) |
| 7. Subsystem coverage vs legacy | 0 | 0 | 0 | 0 | **none — clean** |

---

## Dimension 1: Coordinate-system correctness (Z-up → Y-up)

**Findings: 0.**

- **Single `(x, z, -y)` producer, re-verified.** A fresh grep for the swizzle
  comment/production sites across `crates/` and `byroredux/` returns the
  single production site (`crates/core/src/math/coord.rs::zup_to_yup_pos` /
  `zup_to_yup_quat_wxyz`) plus its typed NIF/collision/particle/SpeedTree
  wrappers, all of which are documented delegations to the same SoT (e.g.
  `crates/nif/src/import/collision/mod.rs:540`, `byroredux/src/systems/particle.rs:103`,
  `byroredux/src/cell_loader/placement_lod.rs:844`). No independent inline
  `(x, z, -y)` swizzle exists outside `coord.rs`.
- **No new bare `4096.0` cell math.** Every production `4096.0` hit resolves
  to `EXTERIOR_CELL_UNITS` (`crates/core/src/ecs/components/camera.rs:214`),
  an unrelated quantity (a UV epsilon in `crates/physics/src/water.rs:330`,
  `LOCOMOTION_GROUND_RAY_MAX_DISTANCE` in `byroredux/src/systems/locomotion.rs:39`,
  `FOG_HEIGHT_REFERENCE_RAY_MAX_DISTANCE` in `byroredux/src/render/mod.rs:21`,
  the combustion-light scale, a scroll multiplier in `systems/cinematic.rs`),
  or `#[cfg(test)]` fixtures. No new collapse candidate.

Neither of the two axes this dimension guards moved in the last 193 commits.

---

## Dimension 2: NIFAL — canonical NIF→ECS translation contract (mapping shape)

**Findings: 0.**

- **Downstream per-game-branch scan re-run clean.** `grep -rn "GameKind"` over
  `crates/renderer/src`, `crates/physics/src`, `crates/core/src` now returns
  four files instead of the prior sweep's three: the same shader-hygiene
  test-string hit (`crates/renderer/src/vulkan/volumetrics.rs`) plus three new
  hits in `crates/core/src/character/{mod,profile}.rs` and
  `crates/core/src/ecs/components/water.rs`. Checked each: the two
  `character/` hits are CHARAL's own per-game ruleset seam (owned by
  `/audit-character`, not a NIFAL/material-boundary leak — CHARAL's whole
  contract *is* a `GameKind`-keyed ruleset selection, at a different boundary
  than `translate_material`). The `water.rs` hit is a doc-comment
  cross-reference to `GameKind`, not a branch. Zero actual code branches
  downstream of any `translate()` boundary.
- **Pattern A re-run clean.** `grep -rnE "bs_version\s*(>=|<=|==|>|<)\s*[0-9]+"`
  over `crates` + `byroredux` returns 0 non-test hits, same as last sweep.

### LC-D2-01 (2026-08-20) — verified FIXED

`water_material_from_mesh`'s bit-16 `blend_normals` gate is gone.
`byroredux/src/material_translate.rs:145-157` now decides the optical-response
subset (`REFLECTIONS`/`REFRACTIONS`, bits 6/7 of the real
`WaterShaderPropertyFlags` enum) directly against the spec bitfield with named
constants, and `blend_normals` is set from the WATR `FNAM` byte on the Skyrim
arm (`crates/plugin/src/esm/records/misc/water.rs:1376-1378`,
`sub.data[0] & 0x10 != 0`) rather than from a bit the file format never
defines. `material_translate.rs:889,909,916` assert `water.blend_normals` is
`true` for mesh-water — the inverted-default bug is closed. Not re-filed.

---

## Dimension 3: Material translation boundary (NIFAL reference slice)

**Findings: 0.**

- `byroredux/src/material_translate.rs::translate_material` remains the sole
  populated-`Material` producer. Grepping `"Material {"` across `byroredux/src`
  and `crates/core/src` finds three production call sites now instead of two
  (`byroredux/src/scene/nif_loader.rs:1020`,
  `byroredux/src/cell_loader/placement_lod.rs:514`,
  `byroredux/src/cell_loader/spawn/mesh_instance.rs:794` — the last is the
  post-refactor home of the former `cell_loader/spawn.rs` call site, a rename
  not a new producer) — all three call `translate_material`, none construct a
  `Material` literal with scalar fields directly. The only literal
  `Material {` constructors are `cornell.rs` (self-contained `--cornell`
  harness, no game data) and `#[cfg(test)]` blocks.
- The deleted `Option`-override + render-time `classify_pbr` path has not
  reappeared. `EmissiveSource` still shares one scale; `NiFogProperty` remains
  the documented deliberate skip.

---

## Dimension 4: PHYSAL — per-game Havok articulation → solver (source axis)

**Findings: 0.**

- `grep -rn "GameKind|game ==" crates/nif/src/import/collision/` returns zero
  hits. `extract_ragdoll` (`crates/nif/src/import/collision/ragdoll.rs:31`)
  still switches only on constraint-block presence
  (`BhkConstraint`/`BhkBreakableConstraint`), never on game.
- The per-game seam is still confined to the constraint `CInfo` decode in
  `crates/nif/src/blocks/collision/constraints.rs`.
- `docs/engine/physal.md` was touched this window (`fdebb508 fix(physal):
  report blocked ragdolls, pin remove/damping, correct §3 seam count`) —
  read the current §3; it still states the seam count correctly against the
  code re-checked above. No drift.

---

## Dimension 5: EXAL / WATAL — per-game exterior environment → renderer & solver

**Findings: 0.**

**Boundary shape re-verified.** `resolve_water_material` still has exactly
two production callers (cell plane + worldspace LOD plane in
`byroredux/src/cell_loader/water.rs`); `default_water_for_worldspace` still
has exactly one (`byroredux/src/cell_loader/exterior.rs`). No third
`SkyParamsRes`/`WeatherDataRes` construction site appeared.

### LC-D5-01 (2026-08-20, Oblivion water damage) — verified FIXED

`crates/plugin/src/esm/records/misc/water.rs:1360-1379`'s `FNAM` arm's
`matches!` gate now includes `GameKind::Oblivion` alongside the other five
games, and sets `legacy_flags` for it
(`if matches!(game, GameKind::Oblivion | GameKind::Fallout3NV) { out.legacy_flags = Some(sub.data[0]); }`).
The `DATA` arm's tail-`u16` capture is likewise no longer Oblivion-excluded
(`:1421-1424`: `if matches!(game, GameKind::Oblivion) && sub.data.len() >= 2 { … legacy_damage = Some(…) }`).
Oblivion's five lava/oil `FNAM=0x01` records now reach the canonical
`WaterPlane::damage_per_second` path the same way FO3/FNV's do. Not re-filed.

### LC-D5-02 (2026-08-20, undeclared second `WaterMaterial` producer) — verified FIXED

`docs/engine/watal.md` §3's `translate()` boundary row now explicitly names
**both** producers — `byroredux/src/env_translate.rs::resolve_water_material`
for ESM WATR/worldspace records, and
`byroredux/src/material_translate.rs::attach_mesh_water` (plus its pure
helpers) for NIF mesh-water shader properties — with "no consumer-side
construction" as the (now accurate) invariant. The `WaterKind` token
divergence that was the concrete symptom is also gone: both producers now
call the same `water_kind_from_name` helper
(`byroredux/src/material_translate.rs:189-204`, doc-commented as "the
**single** name-token → `WaterKind` table, shared by both water producers"),
which includes `canal` and the additionally-discovered `creek` token — the
in-code census cites `creek` as present in 5 `FalloutNV.esm` records, 2
`Fallout3.esm` records, and four Skyrim EDIDs, without which "all 78 vanilla
FNV records classified `Calm`" (per the function's own doc comment).
`grep -rn "WaterMaterial {"` confirms only these two production constructors
remain; everything else is `#[cfg(test)]`. Not re-filed.

---

## Dimension 6: Per-game translation-survey gaps (Pattern A/B/C)

**Findings: 1 (LOW).**

Patterns A and B re-checked clean (see Dimension 2). The one finding is
Pattern-adjacent but about the survey document itself, not new code.

### LC-D6-01 (2026-08-20, WATR simulator-block misalignment) — verified FIXED

All three fixes landed:
`crates/plugin/src/esm/records/misc/water.rs:592-606` (Oblivion),
`:699-719` (FO3/FNV), and `:938-972` (Skyrim) now read the displacement
block's starting size at its own `+16` offset from the block's force field
rather than from the rain block's last slot, and `rain_start_size` at the
rain block's own `+16`. The damaging assignment —
`apply_skyrim_dnam_tail`'s `normal_magnitude = read_f32_at(data, 92)` — is
gone; `:968-972`'s comment states plainly that offset 92 previously fed
`normal_magnitude`, that the constant `0.05` scaled every canonical noise
amplitude, and that the fix removes the assignment. Test pins at
`water.rs:2008,2019` now assert `rain_start_size == 0.01` and
`normal_magnitude == 1.0` (the neutral sentinel) rather than the previous
tautological `0.05` pin. Tracked by issue #3205 (still shown OPEN in `gh` —
the fix commit `4e1afcbe` did not carry a `Fix #3205` trailer — but the code
at HEAD is the fix; not re-filed as a code gap).

### LC-D6-02 (2026-08-20, `decode_data`'s unreachable 144–220 tail) — still open, not re-filed

Re-checked at `crates/plugin/src/esm/records/misc/water.rs:417-483`. The
structural claim is unchanged: `decode_data` delegates to
`decode_data_fo3nv` for any `data.len() >= 186`, then goes on to read offsets
144 through 220 via bounds-checked `read_f32_at` — reachable only for a
`DATA` payload of 148–185 bytes, a window no supported game's vanilla corpus
emits (per the prior sweep's census: Oblivion/FO3/FNV/Skyrim/FO4 all cluster
at 2/42/62/86/102/186/0 bytes, never 148–185). It is bounds-checked so it is
harmless, not a bug — the finding was always LOW/dead-code, not a defect.
Already tracked as **#3146 OPEN**; carried forward, not re-filed.

### LC-D6-03 (2026-08-20, watal.md §4 payload-table naming) — verified FIXED

`docs/engine/watal.md:507` now reads "DNAM 196/184 B (majority) or DATA 186 B
plus 2 B damage" for the FO3/FNV column (naming DNAM explicitly, the fix the
finding asked for) and lists FO76 separately at 148 B rather than folded into
the FO4 cell. The §9 byte census (`docs/engine/watal.md:701-705`) matches the
prior sweep's own scan numbers exactly. Not re-filed.

### LC-D6-2026-08-24-01: `per-game-translation-survey.md` is 3 months stale and its headline example now describes a fixed bug

- **Severity**: LOW
- **Dimension**: Per-game translation-survey gaps — spec vs. codebase currency
- **Location**: `docs/engine/per-game-translation-survey.md:1-52` (header + §2 "the bad news"), `:215-218` (§4.3 "RACE DATA" bullet)
- **Status**: NEW
- **Description**: This audit's own Dimension 6 explicitly names
  `per-game-translation-survey.md` as the reference for cross-game pattern
  findings ("Reference: `docs/engine/per-game-translation-survey.md` (§4
  findings by layer, §5 cross-cutting patterns, §7 …)"). The document's
  `Status:` line says "generated 2026-05-28" and it has not been touched
  since (`git log -1` on the file returns `7220a8d4`, 2026-05-28; HEAD is
  `048a8bd8`, 2026-08-24 — a 3-month, 193+-commit gap on the file the skill
  tells auditors to trust). Two concrete claims inside it are now false:
  1. **§2's headline example**, the document's own stated reason "Fallout
     looks broken": *"the same `Material` slot that holds `metalness 0.79 /
     roughness 0.04` when the input is an FO4 BGSM holds `metalness 0.00 /
     roughness 0.80` when the input is a FNV `BSShaderPPLightingProperty`
     (because `classify_pbr_keyword` collapses every non-glass surface to the
     matte default …)."* That description matches the pre-fix behaviour of
     `classify_pbr_keyword` before issue #1873 (commit `634873db`, "gate PBR
     env-map metalness lift on authored specular, not the struct default"),
     which the project's own memory notes record as **FIXED**. The current
     `classify_pbr_keyword` (`crates/core/src/ecs/components/material.rs:663-`)
     no longer "collapses every non-glass surface to the matte default" — it
     runs an extensive, evidence-cited keyword classifier (metal/precious
     metal/glass/wood/stone/fabric/skin/… arms, each with its own roughness
     history documented inline, e.g. the 2026-06-03 metal roughness
     0.3→0.55 revision at `:676-687`). The document's central "why Fallout
     looks broken" thesis is built on a bug that no longer exists.
  2. **§4.3's `RACE DATA` bullet**: *"size gate ≥ 36 covers Oblivion/FO3/FNV;
     Skyrim is 128+ bytes with a different layout, **no Skyrim arm exists**,
     Skyrim RACE silently parses with the wrong schema."* This is directly
     contradicted by the code at HEAD:
     `crates/plugin/src/esm/records/actor/mod.rs:1219` has a dedicated
     `b"DATA" if matches!(game, GameKind::Skyrim) && matches!(sub.data.len(), 128 | 164)`
     arm, whose own doc comment (`:1198-1211`) states it was "verified
     byte-for-byte against vanilla `Skyrim.esm` (2026-08-12), which ships
     164-byte DATA for all 99 of its RACE records" and cites specific decoded
     values (`NordRace` skill bonuses, `ElderRace`'s seven `Skill_None`
     slots) as evidence. A same-day (2026-08-24) follow-up extended that arm
     to also capture `starting_magicka`/`starting_stamina`
     (`:1244-1254`, closing issue #3219 — still shown `OPEN` in `gh` because
     the landing commit lacks a `Fix #3219` trailer, but the code is the
     fix).
- **Evidence**: See file/line citations above; both claims were checked
  against the live tree, not against the survey's own text.
- **Impact**: Documentation-only — no runtime behavior is affected, and this
  is exactly the doc-rot class the prior sweep's LC-D6-03 finding covered for
  a different file. The reason it is worth flagging rather than silently
  fixing is procedural: this skill's own Dimension 6 tells every future
  auditor to use this document as the reference for "why Fallout is worse
  than Skyrim" and cross-game pattern gaps. An auditor who trusted §2's
  worked example verbatim would misdiagnose the current material boundary as
  broken (Dimension 3 above independently re-confirms it is not) and could
  waste effort re-filing #1873's closed bug. The `~70+ per-game branches`
  headline count in the TL;DR is unverified against the current tree by this
  sweep (a full recount was out of scope) and should be treated as
  unreliable until the document is regenerated — some of the counted classes
  (e.g. `SCOL`/`PKIN`/`MOVS`/`MSWP` "no game gate", re-checked this sweep and
  still true; FO4 collision blocked on `BhkSystemBinary`, still a documented
  PHYSAL limitation) remain accurate, so the document is **partially**, not
  wholesale, stale — a targeted correction is more appropriate than a
  full rewrite.
- **Related**: LC-D6-03 (2026-08-20) — the same doc-rot class, different file
  (`watal.md`). Issues #1873 (closed, the fixed bug §2 still describes) and
  #3219 (open in `gh`, but its code fix is what falsifies §4.3's claim).
- **Suggested Fix**: Regenerate or hand-correct at minimum the two cited
  passages: replace §2's worked `classify_pbr_keyword` example with a current
  one (or drop the specific numbers and point at `material-abstraction.md`'s
  live examples instead), and delete/replace the §4.3 `RACE DATA` bullet with
  the current per-game-arm state (Oblivion/FO3/FNV size-gated, Skyrim
  128/164-byte gated arm exists, FO4/FO76 200/216-byte arm — check
  `actor/mod.rs` for the current FO4+ line count too before publishing the
  correction). Bump the `Status:` date so a future audit can tell at a glance
  whether it predates or postdates recent fixes, the same convention
  `watal.md` already uses.

---

## Dimension 7: Subsystem coverage vs legacy

**Findings: 0.**

- **Scene-graph decomposition, transform model, property→pipeline mapping,
  animation model**: unchanged since the last sweep; no code in this window
  touched `NiAVObject` field mapping, `Transform` composition, or the
  `NiProperty`→pipeline-state table. `NiFogProperty` remains the one
  documented skip.
- **String interning**: `crates/core/src/string/` untouched this window;
  bone-name → entity resolution path (load-bearing for skinning + PHYSAL)
  unchanged.
- All three previously-closed SUBSYS findings (weapon reach/speed,
  `NiTimeController` envelope, REFR `XLOC`) remain in place — re-spot-checked
  at their cited locations, no regressions.

No new subsystem-coverage gap surfaced. The 2026-08-24 grab-bag's actor-value
and RACE.DATA work (see LC-D6-2026-08-24-01 above) is a genuine, verified
subsystem-coverage *improvement* — TES5 Magicka/Stamina actor-value
population, previously entirely absent, now exists — not a gap.

---

## Deduplication

`/tmp/audit/issues.json` (200 issues) was keyword-scanned for every
candidate: `water|watr|watal|blend.normals|WaterMaterial|WaterKind`,
`oblivion|lava|damage|fnam`, `normal.magnitude|displacement|rain`,
`decode_data|dnam`, `race data|skyrim race|magicka|stamina`, `translation-survey`,
`classify_pbr|pbr classifier`, plus the Dimension 1/4/7 keyword sets
(`coordinate|euler|4096|axis`, `ragdoll|havok|constraint`,
`xloc|controller envelope|weapon reach`). `docs/audits/` was scanned for
prior write-ups.

| Finding | Nearest existing | Verdict |
|---|---|---|
| LC-D2-01 | — | Verified FIXED, not re-filed |
| LC-D5-01 | — | Verified FIXED, not re-filed |
| LC-D5-02 | — | Verified FIXED, not re-filed |
| LC-D6-01 | #3205 (OPEN in `gh`, fix already in code) | Verified FIXED at HEAD, not re-filed |
| LC-D6-02 | #3146 (OPEN) | Still valid, still open, carried forward — not re-filed |
| LC-D6-03 | — | Verified FIXED, not re-filed |
| LC-D6-2026-08-24-01 | No issue mentions `per-game-translation-survey.md`, "classify_pbr_keyword" staleness, or the RACE DATA Skyrim claim's obsolescence | **NEW** |

Skipped as already OPEN and owned elsewhere (not duplicated here): #3146
(ESM/legacy-compat shared, carried above), #3189 (audio boot-time double
archive scan — `/audit-audio` territory), #3219 (RACE.DATA Magicka/Stamina —
its code fix is cited as evidence for LC-D6-2026-08-24-01 but the issue
itself belongs to whoever filed it, not re-opened or re-filed here), the full
NIFAL-D* backlog (#2423, #2490, #2532, #2533, #2571, #2697, #3072–#3075,
#3187, #3232–#3235 — all `/audit-nifal`'s per-slice contents, out of this
audit's mapping-shape scope per the skill's own dimension-2 framing).

## Verification

Read-only source review of the current tree at `048a8bd8`, cross-referenced
against the prior sweep's report and against `git log`/`git show` for the 193
commits since `bb0b92f2`. No vanilla master was independently re-scanned this
sweep (the prior sweep's byte census was re-used as ground truth where cited;
new claims were checked against in-tree byte-verified doc comments instead).
No `cargo` command was run — `cargo test --workspace` is blocked by the
unrelated `crates/scripting/examples/fragment_coverage.rs` E0004, already
flagged to `/audit-scripting`; this audit did not attempt a per-crate
`cargo check` since no finding here turned on a compile-time question. No
source file, game file, or GitHub issue was modified.

## Summary

- **Findings:** 1 (NEW) — 0 CRITICAL, 0 HIGH, 0 MEDIUM, 1 LOW.
- **Prior sweep:** 6/6 findings verified fixed at HEAD (not just closed —
  independently re-traced against current code, not taken on the prior
  report's word). No regressions anywhere in the audit's scope.
- **Boundary health:** NIFAL / EXAL / PHYSAL / WATAL all structurally intact
  and, unlike the last sweep, also *content*-clean this time — the water
  fidelity gaps that were this audit's yield twice running (2026-08-16,
  2026-08-20) are now closed. Dimensions 1, 2, 3, 4, 5, 7 are clean; only
  Dimension 6 has a finding, and it is about a reference document, not code.
- **Where the one gap lives:** not in the engine but in the audit
  infrastructure's own reference material.
  `docs/engine/per-game-translation-survey.md` was generated once
  (2026-05-28) and never regenerated across three months and 193+ commits of
  exactly the kind of fix activity it was written to motivate; two of its
  concrete claims are now false because the bugs they describe were fixed.
  This matters more than an ordinary stale doc because this skill's
  Dimension 6 names it as the authority a future auditor should trust.
- **Highest-value fix:** correct or regenerate
  `per-game-translation-survey.md`'s §2 example and §4.3 RACE DATA bullet
  before the next sweep leans on it.

Suggested next step:
```
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-08-24.md
```

TALLY: CRITICAL=0 HIGH=0 MEDIUM=0 LOW=1
