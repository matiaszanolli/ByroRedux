# Starfield ESM Parser Action Plan

**Status**: planning doc — 2026-05-28.
**Trigger**: user wants to render Cydonia. Cell rendering needs an ESM parser; Starfield ESM is currently unimplemented.
**Audience**: ByroRedux engine devs.

## The surprise upfront

A lot of infrastructure is already Starfield-aware:

- **`GameKind::Starfield`** exists (`crates/plugin/src/esm/reader.rs:99-100`) and is auto-detected from HEDR `0.96` (`reader.rs:140`).
- **TES4 detection** works against real `Constellation.esm` — parser returns `Ok(0 interior cells)` without panic (smoke-tested 2026-05-28).
- **12+ per-record dispatch sites** already branch on `GameKind::Starfield` (equipment, climate, outfit, items, scol, weather, actor records).
- **Form-id width** is already `u64` (`crates/core/src/form_id.rs:43`) — no struct rework needed.
- **GRUP walker** is per-game agnostic.

The work isn't "build from scratch." It's "fill in the SF-specific record types, validate per-game CELL/REFR subrecord layouts, and stand the parser up against the actual 1.4 GB `Starfield.esm` corpus."

## Reference materials

- **Gibbed.Starfield** at `/mnt/data/src/reference/Gibbed.Starfield/projects/Gibbed.Starfield.PluginFormats/` — `FormType.cs` is the canonical 214-record-type enum with FourCC, ID, and C++ class name for every Starfield form type. **This is the single most valuable reference.**
- **xEdit / xelib** — partial Starfield record-schema reverse engineering (not cloned locally; check `https://github.com/TES5Edit/TES5Edit` for the Starfield branch).
- **Bethesda's own debug strings** in `Starfield.exe` carry many subrecord FourCCs and field-name hints.
- **Material-Editor** at `/mnt/data/src/reference/Material-Editor/` — CK material editor; useful for cross-checking record layouts but limited to materials.

## Scope decision: what counts as "ESM parser complete"

Three concentric scopes:

1. **Minimum**: parse the ESM without panics, extract CELL + REFR data, render Cydonia interior. ~3-4 weeks of focused work.
2. **Practical**: + NPC + ARMA + outfit so NPCs spawn populated. ~2-3 more weeks.
3. **Full**: + planets + biomes + procedural surfaces + space cells + ship assembly. **Months**, much of it gated on rendering work (M64-tier procedural exteriors).

**This roadmap targets the minimum scope.** Practical scope is a Phase 6 follow-up; full scope is deferred to roadmap milestones we haven't started.

## Phase 0 — Baseline smoke test (1-2 sessions)

Establish the existing parser's actual behavior against real Starfield data. Without this baseline, every later phase is guessing what's broken vs already fine.

**Deliverables**:
- New `cargo run -p byroredux-plugin --example sf_smoke <ESM_PATH>` tool that parses an ESM and reports:
  - HEDR detection result (game kind, version, master list)
  - Total GRUP count by top-level type FourCC
  - Per-FourCC record count
  - Number of records that hit the dispatch's `_unhandled` arm (i.e. silently skipped)
  - Number of records that the dispatch claims to handle (passed to a per-record decoder)
  - Any panics / `?`-bailout errors with byte offset
- Baseline TSV at `.claude/audit-baselines/sf-esm/` per ESM file (Constellation, Starfield, BlueprintShips-*, ShatteredSpace, OldMars).
- 1-page summary doc: "what works today, what's the actual coverage gap."

**Why this matters**: Constellation.esm parsed to 0 interior cells with no error. Is that because Constellation has zero interior cells in its content (plausible — it's a DLC patch), or because the CELL dispatch silently dropped them? We don't know until we measure. **Do not write the plan beyond Phase 0 before this measurement lands.**

**Exit criteria**: a number for "% of Starfield.esm records the dispatch actually handles" with per-FourCC breakdown.

## Phase 1 — 214-FourCC dispatch table + warned-skip pattern (1-2 sessions)

Goal: every Starfield record FourCC is recognized; unhandled records emit a one-shot warn (mirroring existing `warned_scol` / `warned_movs` / `warned_pkin` / `warned_mswp` pattern at `crates/plugin/src/esm/records/mod.rs:190-193`) instead of getting silently skipped or — worse — mis-decoded as a similar-FourCC type.

**Deliverables**:
- Generated Rust source from `Gibbed.Starfield/FormType.cs` listing all 214 FourCCs as `pub const SF_RECORD_*: [u8; 4]`.
- Extended `parse_top_level_records` (or sibling) with `is_starfield_only` predicate (mirrors `is_fo4_plus`) and per-record `warned_*` bool gating. Only fire warns when `game_kind == Starfield` (FO4/Skyrim plugins legitimately don't have these types).
- Integration test: `BYROREDUX_STARFIELD_DATA=... cargo test -p byroredux-plugin --test sf_full_parse -- --ignored` walks every Starfield ESM and asserts:
  - Zero panics
  - Zero `?`-bailouts on unhandled FourCCs (graceful skip, not error)
  - Warning-rate baseline pinned (Phase 0's TSV becomes the reference)

**Why this isn't "do nothing"**: the existing dispatch silently falls through on unknown FourCCs in some paths and bails out in others. We need a uniform "recognized but unimplemented" status for the 200 Starfield-only types we don't yet decode, so Phase 0's "% handled" number is meaningful.

**Exit criteria**: full Starfield ESM corpus parses with zero panics and zero `?`-bailouts; Phase 0 TSV becomes a regression gate.

## Phase 2 — TES4 + load order validation (1 session)

Verify the existing TES4 + master-list infrastructure handles Starfield's edge cases:

- **`BlueprintShips-*.esm`** — these are content patches that depend on Starfield.esm. Verify multi-master load is correct (form-id remap, override semantics).
- **Localized strings** — Starfield's STRINGS / DLSTRINGS / ILSTRINGS files live alongside the ESMs; verify the existing `strings_table.rs` handles SF's encoding (probably UTF-8, same as FO4+).
- **HEDR-version sub-band detection** — Constellation, Shattered Space, BlueprintShips might use bumped HEDR sub-versions (e.g. 0.96 vs 0.96.1) that need distinct sub-arms. Phase 0 measurement reveals if this matters.
- **DLC ordering** — Shattered Space depends on Starfield.esm; the existing load-order infra (`crates/plugin/src/legacy/`) needs validation that ordering works for SF.

**Deliverables**: pass tests against a real multi-master Starfield load (Starfield.esm + Constellation.esm + ShatteredSpace.esm) with form-id resolution correct.

## Phase 3 — STAT / MSTT / TXST + GBFM/GBFT base records (1-2 sessions)

Cydonia interior architecture is mostly `STAT` records (static meshes) with `TXST` material refs. Without these, every REFR has no base form to link to and renders as a 3D-unit-cube placeholder.

**Deliverables**:
- `STAT` parser — model path (`MODL`), object bounds (`OBND`), texture set ref (`MNAM`), keywords (`KSIZ` / `KWDA`). Mostly FO4-baseline reusable.
- `MSTT` parser (Movable Static) — same as STAT plus motion subrecords.
- `TXST` parser — 8 texture slots (`TX00` … `TX07`); FO4-compatible plus any SF-specific additions Phase 0 / Gibbed flagged.
- **`GBFM` / `GBFT` (Generic Base Form / Template)** — these are *new* in Starfield. Bethesda introduced a meta-record that lets one form reference another as a "template" with parameter overrides. This is non-trivial; Phase 0 measurement reveals frequency. If common in Cydonia content, this is a hard blocker.
- Tests: parse a known STAT + TXST + GBFM out of Starfield.esm by form-id, assert expected field values.

## Phase 4 — CELL + REFR with SF-specific subrecord variants (2-3 sessions)

The existing CELL walker (`crates/plugin/src/esm/cell/walkers.rs`) handles FO3 → FO76 cell-block-and-sub-block traversal. Starfield uses the same outer GRUP shape but may have:

- **New XCLL subrecord layout** — interior lighting structure may be a different size (`xcll_canonical_sizes` table at `cell/walkers.rs` may need a `Starfield => N` arm; Phase 0 measurement tells us N).
- **New REFR subrecords** — Bethesda may have added Starfield-specific REFR-flags / data fields. Per-game variants in `cell/walkers.rs` already exist for FO76; SF probably needs its own arm.
- **Persistent vs temporary REFR split** — historically consistent, but SF-specific paths need verification.
- **XEZN / XCWT / XCLR** — encounter zone, water type, climate ref. Usually reusable; verify against real SF data.

**Critical**: do NOT touch the existing FO3 / FNV / Skyrim / FO4 CELL/REFR paths. Add Starfield arms; don't refactor the working paths.

**Deliverables**:
- `parse_cell_group(..., GameKind::Starfield)` walks Cydonia's CELL without panic and extracts all REFR positions / rotations / scales / form-id refs.
- New test `parse_cydonia_cell` in `crates/plugin/src/esm/cell/tests/` (likely new `tests/starfield.rs` sibling).
- TSV regression baseline for Cydonia CELL: entity count, REFR count, lighting struct values.

## Phase 5 — Render Cydonia interior (validation milestone)

The visible-progress moment. With Phases 1-4 landed:

- `cargo run -- --esm Starfield.esm --cell Cydonia<interior_id> --bsa "Starfield - Meshes01.ba2" --textures-ba2 "Starfield - Textures01.ba2" --materials-ba2 "Starfield - Materials.ba2"` should:
  - Find the cell via probe_cells
  - Spawn every architectural REFR
  - Resolve STAT base meshes via BA2 extraction (already works per existing infra)
  - Render with Disney BSDF on Starfield content (already wired per #1289 Phase 1)
- Probably won't be pretty — no NPCs, no skybox, no proper lighting. But it'll be *the first Starfield cell on screen*.

**Deliverables**: screenshot in `docs/audits/SF_FIRST_RENDER_<DATE>.md` + a `byro-dbg` `stats` snapshot.

**Acceptance criteria**: at least 50 REFRs render, no panic, FPS > 30, `tex.missing` count < 20% of total. Looser than other-game cells because we're not chasing perfection — this is the proof-of-life.

## Phase 6 — NPC + ARMA + outfit (3-4 sessions)

Populates cells with NPCs. SF inhabitants are humanoid (FO4 ARMA-style biped); the rig should mostly mirror FO4. Open questions:

- New NPC subrecords (probably; SF added perks / traits / backgrounds)
- Outfit (OTFT) records — same as Skyrim+
- ARMA (armor addon) — biped definitions likely FO4-baseline
- Face genetics — SF uses a different head-mesh system (probably distinct from FO4 FaceGen; needs research)

Defer face genetics to a follow-up; render NPCs with neutral default heads for Phase 6.

## Phase 7+ — Long-tail (months, decoupled from this plan)

- **Planets** (PNDT) — out of scope for ground rendering; needed for star-map UI.
- **Stars** (STDT) + sun presets (SUNP) — star-map UI; pretty but optional.
- **Biomes** (BIOM) + surface patterns (SFBK/SFPT/SFTR) — drive procedural exterior generation (M64-tier engine work).
- **Procedural planet content managers** (PCMT/PCBN/PCCN) — Bethesda's procgen engine; very deep.
- **Layered material swaps** (LMSW) + material paths (MTPT) — material system enhancements; ride on the deferred Phase 2 of #1289 (CDB per-field extraction).
- **Ship assembly** (BlueprintShips, MOD attachments) — gameplay-driven; out of scope without a form-linker for runtime entity composition.

## Decision points before starting

Three questions to answer before Phase 0:

1. **Form-id remap policy for Starfield ESMs**: SF's multi-master load with BlueprintShips DLCs may have edge cases the existing `FormIdRemap` infra doesn't cover. Worth a 1-hour spike to verify.
2. **Strings file encoding**: confirm SF strings files are UTF-8 (probably) vs the legacy Windows-1252 some older games use.
3. **`GBFM` (Generic Base Form) frequency**: Phase 0 measurement will reveal whether GBFM is rare (defer to Phase 7) or common (must close in Phase 3). The whole shape of Phase 3 changes based on this number.

## Sequencing decision

Two ways to attack this:

**(A) Build serially** — Phase 0 → 1 → 2 → 3 → 4 → 5. Each phase produces a regression test that gates the next. ~3-4 weeks for Phase 5 visible-progress milestone.

**(B) Build by visible-result slice** — measure (Phase 0), then implement a single thin path through every layer just enough to render ONE Cydonia REFR with ONE STAT base, then expand. ~1 week to first pixel; ~4 weeks to "the cell renders properly."

**Recommendation: (B).** Bethesda has 214 record types but Cydonia only needs ~30. Optimize for the visible moment, not for parser-spec completeness. Phase 0 is non-negotiable either way.

## Out of scope explicitly

To keep this plan honest:

- Per-field CDB extraction for Starfield materials (separate issue, Phase 2 follow-up to #1289)
- Procedural-planet surface generation (M64-tier engine work; orthogonal)
- Star-map UI (PNDT/STDT/SUNP records; needs the planet/star-map renderer first)
- Ship assembly (form-linker for runtime entity composition; gameplay-driven)
- Save game compatibility (SF saves are a separate format; not blocking)
- Modded ESM tolerance (Creation Kit / xEdit-edited ESMs may have edge cases; defer)

## Effort estimate (focused implementation time)

| Phase | Estimate | Visible deliverable |
|------:|---------:|---------------------|
| 0 | 1-2 sessions | sf_smoke tool + baseline TSV |
| 1 | 1-2 sessions | All FourCCs dispatched; zero-panic corpus parse |
| 2 | 1 session | Multi-master Starfield+Constellation+ShatteredSpace load |
| 3 | 1-2 sessions | STAT/MSTT/TXST/GBFM decoded for Cydonia content |
| 4 | 2-3 sessions | Cydonia CELL + REFRs extracted |
| 5 | 1 session | Cydonia interior renders on screen |
| 6 (deferred) | 3-4 sessions | NPCs populate the cell |
| 7+ | months | Procedural / space cells / etc. |

**Total minimum-scope effort**: 7-11 sessions to "Cydonia interior renders." Roughly 2-3 weeks calendar time.

## References

- Gibbed FormType: `/mnt/data/src/reference/Gibbed.Starfield/projects/Gibbed.Starfield.PluginFormats/FormType.cs`
- Existing ESM reader: `crates/plugin/src/esm/reader.rs` (1247 LOC, already SF-aware)
- Existing CELL walker: `crates/plugin/src/esm/cell/` (3000+ LOC, FO3-FO76 coverage)
- Existing record dispatch: `crates/plugin/src/esm/records/mod.rs` (200 LOC main + per-record-type sibs)
- Audit findings that triggered this: `docs/audits/AUDIT_STARFIELD_2026-05-28.md` Dim 6 forward-blockers
- Sibling Phase 1 (CDB consumer wiring): #1289 (closed 2026-05-28, commit 6bd510ba)
