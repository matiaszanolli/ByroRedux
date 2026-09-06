# #3914: FNV-2026-09-05-D4-01: `SOUN.FNAM` is a *folder* on 1 620 of FalloutNV.esm's 3 189 sound records (50.8 %), but `SounRecord.sound_path` and `sound_archive_path` are both documented and implemented as a file path

Filed from `docs/audits/AUDIT_FNV_2026-09-05.md` (FNV-2026-09-05-D4-01) via `/audit-publish`, 2026-09-05 (`/audit-suite --preset per-game-all`). Labels: `medium,game:fnv,legacy-compat,audio,esm-plugin,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3914 --json state`.

---

**Source**: `docs/audits/AUDIT_FNV_2026-09-05.md` (FNV-2026-09-05-D4-01), `/audit-suite --preset per-game-all`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: 4 — ESM Record Parser (this game's data through the parser)
- **Location**:
  - `crates/plugin/src/esm/records/soun.rs` — `SounRecord::sound_path` and its
    module doc ("`FNAM` — the sound's file path")
  - `byroredux/src/asset_provider/audio.rs` — `sound_archive_path`,
    `resolve_sound_path`
- **Status**: NEW
- **Description**: `sound_archive_path` normalises `FNAM` to
  `sound\<fnam>` and hands the result straight to `Archive::extract`. That is
  correct for a file. But **1 620 of the 3 189 FNAM-bearing FNV `SOUN` records
  (50.8 %) author a directory** — the value ends in `\` and names a folder of
  variant `.wav`s that the retail engine picks from at random. Every one of
  those is a guaranteed `extract` miss, because a folder is not an archive
  entry key.

  Measured variant counts inside the named folders (so this is the real
  convention, not a data typo):

  | Files in the named folder | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8+ |
  |---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
  | SOUN records | 55 | 51 | 272 | 351 | 253 | 201 | 199 | 103 | 135 |

  Worked examples: `EMTCeilingCrumble` → `fx\amb\ceilingcrumble\` (4 files),
  `FXExplosionArtilleryHoover` → `fx\fx\explosion\artillery\incoming\` (3),
  `WPNShotgunSawFire2D` → `fx\wpn\shotgunsawed\fire_2d\` (2). **103 of the
  folder-form records are footstep sets** (`FSTConcSolidJump` →
  `fx\fst\conc_broken\jump\`), i.e. exactly the corpus M44 Phase 3.5b
  ("FOOT-record-driven per-material lookup") is aimed at.

  This also reframes the profile's own resolution figure. `#3788` says
  "2,700/3,189 (84.7 %) of FNV SOUN.FNAM paths resolve inside this one
  archive"; independently measured the same way, **2 692 (84.4 %)** — but
  **1 565 of those 2 692 (58 %) resolve only as a folder**, and just **1 127
  (35.3 % of all SOUN) resolve as an extractable file.**
- **Evidence**: independent ESM walk of every `SOUN` record's `FNAM` in
  `FalloutNV.esm`, cross-indexed against a full listing of
  `Fallout - Sound.bsa`. `sound_archive_path`'s own doc calls `FNAM` "file path
  relative to `Data\Sound\` … the same convention as `MODL` being relative to
  `Meshes\`" — the analogy is what is wrong: `MODL` is always a file, `FNAM`
  is not.
- **Impact**: Bounded **today** — the only live `resolve_sound_path` consumer is
  `dispatch_region_ambient_music`, which is already a no-op on FNV for the
  unrelated MSET reason (closed #3787). But this is the premise the entire
  SOUN → audio path is built on, and the next consumer (per-material footsteps,
  weapon fire, ambient loops) silently loses half the reference title's sound
  library the day it lands, with no error — the same failure shape as
  FNV-2026-09-05-D8-01, one layer up. Scored MEDIUM on that basis, not on
  today's blast radius.
- **Related**: #3788 (the 84.7 % figure), #3301 (REGN ambient loops — a future
  consumer that would hit this directly), FNV-2026-09-05-D8-01.
- **Suggested Fix**: Have `SounRecord` carry the distinction rather than a bare
  `String` — a `sound_path` plus a `is_folder` flag, or an enum — and give the
  provider a `extract_any_in(folder)` sibling that picks one entry. Random
  selection needs a policy decision (deterministic-per-emitter vs. per-play),
  so this is real design work, not a normalisation tweak; the minimum
  non-guessing step is to stop *documenting* `FNAM` as a file path and to log
  the folder form distinctly from a genuine miss.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other block parsers, other games)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `translate_material` / `Material::resolve_pbr` / the emitter params, per-game logic stays at the NIFAL parser→`Material` boundary
