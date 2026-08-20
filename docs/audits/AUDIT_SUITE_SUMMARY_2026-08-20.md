# Audit Suite Summary — comprehensive — 2026-08-20

24/25 audits complete, plus one arbitration pass. **0 CRITICAL · 20 HIGH · 63 MEDIUM · 59 LOW** (142 findings).

`/audit-runtime --game all` **did not run** — see *Coverage gaps* below. This is a
24/25 sweep, not a full one.

Severity counts are grepped from the reports' own `TALLY:` lines, not relayed
from agent messages. The arbitration pass's tally is **not** added to the total:
its HIGH is `LC-D6-01` upheld, not a new defect.

Delta audited: **335 commits since the 2026-08-16 sweep** (`bb0b92f2` at start),
near-monothematic session-70 WATAL water work plus terrain-LOD streaming,
volumetrics, SpeedTree wind sharing and CHARAL wiring.

| Audit | C | H | M | L | Report |
|---|---|---|---|---|---|
| Physics | 0 | 3 | 3 | 3 | `AUDIT_PHYSICS_2026-08-20.md` |
| ESM | 0 | 2 | 4 | 3 | `AUDIT_ESM_2026-08-20.md` |
| Save | 0 | 2 | 3 | 3 | `AUDIT_SAVE_2026-08-20.md` |
| Skyrim | 0 | 2 | 2 | 1 | `AUDIT_SKYRIM_2026-08-20.md` |
| Audio | 0 | 1 | 1 | 4 | `AUDIT_AUDIO_2026-08-20.md` |
| Concurrency | 0 | 1 | 1 | 1 | `AUDIT_CONCURRENCY_2026-08-20.md` |
| ECS | 0 | 1 | 4 | 2 | `AUDIT_ECS_2026-08-20.md` |
| FNV | 0 | 1 | 2 | 0 | `AUDIT_FNV_2026-08-20.md` |
| FO4 | 0 | 1 | 2 | 0 | `AUDIT_FO4_2026-08-20.md` |
| Legacy-compat | 0 | 1 | 2 | 3 | `AUDIT_LEGACY_COMPAT_2026-08-20.md` |
| Oblivion | 0 | 1 | 2 | 2 | `AUDIT_OBLIVION_2026-08-20.md` |
| Performance | 0 | 1 | 3 | 5 | `AUDIT_PERFORMANCE_2026-08-20.md` |
| Renderer | 0 | 1 | 2 | 2 | `AUDIT_RENDERER_2026-08-20.md` |
| Starfield | 0 | 1 | 1 | 4 | `AUDIT_STARFIELD_2026-08-20.md` |
| UI | 0 | 1 | 3 | 3 | `AUDIT_UI_2026-08-20.md` |
| Character | 0 | 0 | 4 | 1 | `AUDIT_CHARACTER_2026-08-20.md` |
| FO3 | 0 | 0 | 2 | 3 | `AUDIT_FO3_2026-08-20.md` |
| NIF | 0 | 0 | 2 | 2 | `AUDIT_NIF_2026-08-20.md` |
| NIFAL | 0 | 0 | 4 | 2 | `AUDIT_NIFAL_2026-08-20.md` |
| Regression | 0 | 0 | 3 | 3 | `AUDIT_REGRESSION_2026-08-20.md` |
| Safety | 0 | 0 | 4 | 1 | `AUDIT_SAFETY_2026-08-20.md` |
| Scripting | 0 | 0 | 3 | 1 | `AUDIT_SCRIPTING_2026-08-20.md` |
| SpeedTree | 0 | 0 | 3 | 4 | `AUDIT_SPEEDTREE_2026-08-20.md` |
| Tech-debt | 0 | 0 | 3 | 6 | `AUDIT_TECH_DEBT_2026-08-20.md` |
| *(arbitration)* | 0 | *1* | *3* | *3* | `AUDIT_WATR_ARBITRATION_2026-08-20.md` |

---

## The water sprint shipped onto a decoder reading the wrong bytes

335 commits of WATAL work landed in four days. Eight independent audits
converged on the same conclusion: the feature work is sound, the **data layer
beneath it is not**, so much of the sprint tuned against values that were never
what the games authored.

The decisive result came from an arbitration pass run after `/audit-fo4`
contradicted a finding four other audits had accepted.
`AUDIT_WATR_ARBITRATION_2026-08-20.md` settles it against shipped bytes from all
seven vanilla masters (no xEdit `*Records.pas` exists on disk, so neither
audit's "verified against xEdit dev-4.1.6" was re-checkable at its stated
source):

- The GECK/CK ships one default simulator tuple — Rain `0.1/0.6/0.985/2.0/0.01`,
  Displacement `0.4/0.6/0.985/10.0/0.05` — appearing verbatim in Oblivion, FO3,
  FNV, Skyrim and FO4 bytes. It pins the field identities unambiguously.
- **Oblivion, FO3/FNV and Skyrim all commit the same one-field swap.** Skyrim's
  `normal_magnitude` really is the displacement starting size, `0.05` on
  **34/34** records.
- **Both original claims were wrong about FO76**, whose 148-byte DNAM is a
  Starfield-family layout, not an FO4 one — its decoder feeds wind-direction
  *degrees* into `displacement`.
- The real Skyrim outcome is **worse** than first reported: `water.frag`'s
  `max(detail, 0.05)` floor means 33/34 records land at or below it, so every
  Skyrim water renders at exactly `0.05` with all authored variation
  (0.0725–1.0) destroyed. Ratio 13.9×–18.6×, not a flat 20×.
- Larger defect both claims missed: the **majority** FO3/FNV path
  (`decode_dnam_pre_fo4`) decodes **nothing past byte 52** — 79% of FO3 and 90%
  of FNV records.

Amendments required: `LC-D6-01` **upheld** (correct 31/31→34/34, strike "FO76 is
correct"); `FO4-D6-2026-08-20-02` **partially withdrawn** — publish its Oblivion
half, strike its "FNV/Skyrim/FO76 are correct, do not fix them" collateral.

### Do not fix by analogy

Two different bits in two different formats, and conflating them would break
working code:

- **WATR-side `FNAM` bit `0x10` is empirically CORRECT** (`DefaultWaterFlow`
  0x08 vs `DefaultWaterFlowBlend` 0x18). Leave it alone.
- **NIF-side `blend_normals` bit 16** is the undefined one (`LC-D2-01`).

---

## The verification layer is still green by construction

The 2026-08-16 sweep's meta-finding was that this codebase's checks are
"systematically green by construction". That has not improved — this sweep found
**16 more instances**, several created *inside this delta*:

| # | Where | The green signal | What it cannot see |
|---|---|---|---|
| 1 | Safety | Water-UBO guard asserts a 64 KiB spec floor | Real floor is 16 KiB — the guard cannot fail |
| 2 | ECS / Concurrency | `known_conflict_count() == 0` | Falsely 0 *because* the `WindField` declaration is missing |
| 3 | NIF | per-block baselines not regenerated after #2562 | Will now report `ParsedShrank` on every game |
| 4 | Performance | #2923 Fx-hashing guard | Only the collections it names; new water/vegetation maps skipped the rule |
| 5 | Physics | Buoyancy quiesced-fast-path test | Passes only because the test world omits `TotalTime` |
| 6 | Physics | `swim_vertical_velocity` tests | All use `dt=1/60`, hiding a ~4.8× 30→144 fps spread |
| 7 | Renderer | `GpuWaterParams` | 3 declaration sites, no lockstep guard at all |
| 8 | Character | #3095 real-data test | 1 roster of 5, no derived-row key — how the Skyrim Illusion key survived |
| 9 | UI | `KNOWN_MISSING_ON_DESTROY_TRAIT` removed | Traded a hard failure for a silent one |
| 10 | UI | Corpus gate | Consumes the output of the scan it audits |
| 11 | Scripting | `m47-triggers.sh` | No exit-code assertion — deleting `attach_vmad_scripts` leaves it green |
| 12 | Tech-debt | `_audit-validate.sh` symbol advisory | Lowercase-anchored regex skips all 157 SCREAMING_SNAKE_CASE symbols |
| 13 | Tech-debt | `audit-physics/SKILL.md:290` | *Instructs* auditors that swimming/drowning are unbuilt — nearly suppressed 2 real findings |
| 14 | FO3 | `installed_masters_water_fields_…` FO3 row | Assertions satisfiable by `WaterParams::default()` |
| 15 | Skyrim / Arbitration | `water.rs:1788` | Pins the **wrong** `0.05`; the FO4 test 20 lines below asserts the opposite correctly |
| 16 | Starfield | Absorption tests | Synthetic fixtures encode the distance hypothesis; cannot falsify it |

**The worst one is not a test.** `crates/plugin/src/esm/records/tests.rs` has
carried 3 raw NUL bytes since `09682c71`, so `grep -r` treats it as binary and
skips it — hiding **40 guards citing 31 issues** from the discovery recipe every
one of the 27 audit skills prescribes (`REG-D1-01`). A green-by-construction
*tool*, invisible to all audits simultaneously.

Related: **43 of 134 issues closed in this window (32%) have no commit citing
them**; 14 have no citation anywhere. All 14 hand-verified as genuinely fixed,
but the fix→issue link `/audit-regression` depends on is broken for a third of
the window.

---

## Delta-introduced regressions (shipped in the last four days)

| Finding | Effect |
|---|---|
| `FNV-D1-01` (H) | New `FNAM & 0x02` gate zeroes reflectivity on 36/78 FNV WATR / 67 cells. Vanilla disproves the premise: `NVCleanWater` and `NVCleanWaterNoReflect` have byte-identical DNAMs, `FNAM=0x02` on **both** |
| `SKY-D6-01` (H) | New `water.frag` blend-normals gate discards every authored normal layer but the first on 34/34 Skyrim records |
| `SPT-D2-02` (M) | #1374 camera-parked early-out now bypassed every frame in any windy exterior; re-dirties `GlobalTransform` for *every* `Billboard` |
| `SAVE` (M) | `LodCoverageStats` landed unclassified in the registry this delta |
| `FO3-D…` (—) | The FO3 water assertion added this delta is satisfiable by `WaterParams::default()` |

## Latent bugs the water sprint finally exposed

- `AUD-D2-01` (H) — spatial attenuation authored in **metres**, fed **Bethesda
  units** (70 BU/m). Everything not co-located with the listener is inaudible
  past ~17–43 cm. Survived **eleven cycles** because `footstep_system` emits at
  the listener's own entity; the new water splash is the first offset emitter.
- `PHYS-D5-02` (H) — the 08-16 report's explicitly **"Disproved Candidate"**
  went live: sensor colliders are excluded only by `cast_ray`, so
  non-collidable Havok bodies are solid walls to the player and false floors to
  the spawn probe.

## Dead or unreachable paths (the recurring shape this sweep)

- `UI-D5-02` (H) — the archive-backed menu path (911 lines of `navigator.rs` +
  BSA/BA2 providers) has **no engine caller**; the shipped binary cannot open a
  vanilla Bethesda menu. Documented under *Status*, not *Pending*.
- `FO4-D2-01` (H) — resolved MSWP swap table has **zero** consumers → 239,573
  vanilla REFRs render unswapped. Existing **#973**, closed as `low`, never
  fixed. Reopen at `high`.
- `SCR-D6-01` (M) — #2940's `HasPerk` fix reads `Perks`, whose only writer is
  FO4+-gated and never applied to the player → structurally `0.0` on
  Skyrim/FO3/FNV and for the player in every game.
- `FNV` (M) — all 78 FNV WATR classify `WaterKind::Calm` (Skyrim vocabulary;
  FNV names its moving water `Creek*`/`Potomac*`), so `WaterFlow` and the whole
  WATAL current half are **unreachable on the reference title**.
- `SPT` (L) — the geometry-wind branch added by `6096f19f` is unreachable.
- `SF` (M) — Starfield concentration controls: 41/60 values clamped away, shader
  density saturates to 1.0 on 12/15 records — feature conveys no information.
- `FO3-D4-01` (M) — `LodBandLadder::for_game` returns `None` for `Fallout3NV`:
  567 meshes + 832 textures unreachable.

## Cross-cutting: VRAM

`REN-D16-01` and `PERF-D3-01` independently: `docs/engine/memory-budget.md`
models the volumetrics froxel grid at 2 RGBA16F volumes/slot; the code allocates
6 (44 B/froxel). Real ~730 MB @1080p / **2.92 GB @4K** vs 265 MB / 1.06 GB
documented — and the doc's summary ledger still carries the pre-Session-62
29.5/118 MB figure, contradicting its own section by 9×. Breaks the <4 GB target
on the 12 GB dev GPU.

---

## What is confirmed healthy

Verified rather than assumed, and worth as much as the defects:

- **PHYSAL doctrine holds** — no game/version symbol anywhere in the solver path.
- **`translate_material` is clean** — NIFAL's single boundary intact, 3 callers.
- **All GPU-struct mirrors in lockstep at HEAD**, verified field-by-field
  (`GpuInstance` ×5, `GpuMaterial` 87 fields, `CameraUBO` ×5, `GpuWaterParams` ×3).
- **20/21 shaders byte-identical** to a fresh recompile (the 21st is
  `REN-D3-01`: stale `triangle.frag.spv`, `RENDER_DEBUG_MODE_MAX` 7→8).
- **Oblivion NIF parsing 100% clean** — 8,032/8,032, zero truncations.
- **FO4 parse census 100.00%** — 159,866/159,866.
- **Zero code regressions** — 47/47 traceable closures present; 18/18
  water-adjacent older fixes survived the rewrite.
- **CHARAL's "wiring gap closed" is genuine**, proved with an independent binary
  parser: 1 game / 3 derived rows → **3 games / 19**.
- **Untrusted-input verdict clean** for `.pex`, `.psc` and `.hkx`.
- **#3076 billboards genuinely face the camera** — the carry-over hypothesis was
  disproved, not confirmed.

## Issue hygiene

- **Close**: #3070 (premise removed by #3036), #2888 (resolved by `4c383433`),
  #2767 (fixed via `include/mesh_id.glsl`).
- **Reopen at `high`**: #973 (closed `low`, never fixed).
- **#3069** closed but its `0x02` half is live — Skyrim LVLI over-expansion,
  1,491/5,118 NPCs, worst case 1,612 items on one NPC (`SKY-D3-01`).
- **#2564** half-resolved; ROADMAP row still stale and wrong **by 6, not 5**.
- **#3082** closed with half its fix; set-equality half vacuous.
- Premises still valid: #2424, #2425, #2597.

## Coverage gaps — this sweep does NOT bless a release

1. **`/audit-runtime --game all` did not run.** A concurrent `cargo test` held
   the target lock for the sweep's duration, and `target/release/byroredux`
   predates most of the delta — telemetry from it would diff stale code against
   the baselines. Run separately once the tree is quiet.
2. **No cargo, no engine, no GPU** in any of the 24 static audits (deliberate:
   25 agents contending on one target lock). Every barrier/layout/VRAM verdict
   is source-read only. Several agents wrote from-scratch Python ESM/BA2/NIF
   readers instead, which cross-check the Rust rather than restate it.
3. **`BYRO_VALIDATION=1` never ran** — safety dim 5's validation channel is
   uncovered. Owed: `--cornell` plus a real water cell.
4. **Gamebryo 2.3 source tree not mounted** (same as 08-16). Legacy-compat dim 7
   ran against `docs/legacy/` + `nif.xml`, which take precedence anyway.
5. **Un-owned subsystems**: `crates/facegen`, `crates/debug-server`,
   `crates/debug-protocol` — no owner, not covered. The **P2 gameplay slice**
   (`combat.rs`, `inventory.rs`, `settings_io.rs`, action half of
   `interaction.rs`) was folded into `/audit-ecs` for this run and produced
   `ECS-2026-08-20-03` (weapon-slot collision); it still has no standing owner.
6. `/tmp/audit/issues.json` spans only #2671–#3103 (400-issue fetch limit), so
   older numbers were carried on prior reports' word.

## Suggested publish order

```
/audit-publish docs/audits/AUDIT_WATR_ARBITRATION_2026-08-20.md   # settles the layout first
/audit-publish docs/audits/AUDIT_PHYSICS_2026-08-20.md
/audit-publish docs/audits/AUDIT_ESM_2026-08-20.md
/audit-publish docs/audits/AUDIT_SAVE_2026-08-20.md
/audit-publish docs/audits/AUDIT_SKYRIM_2026-08-20.md
```
…then the remaining 19 reports with findings.

**Publish the arbitration first.** Five reports carry the pre-arbitration
framing; publishing them ahead of it would file one withdrawn claim and several
wrong counts.
