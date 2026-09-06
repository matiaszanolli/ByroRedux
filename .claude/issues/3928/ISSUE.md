# #3928: FO4-2026-09-05b-D9-01: nothing in the repository gates the lit palette path against real FO4 content, so `#3897`'s activation and D5-01's miscolouring are both invisible to `cargo test`

Filed from `docs/audits/AUDIT_FO4_2026-09-05b.md` (FO4-2026-09-05b-D9-01) via `/audit-publish`, 2026-09-05 (`/audit-suite --preset per-game-all`). Labels: `medium,game:fo4,legacy-compat,shaders,test-gap,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3928 --json state`.

---

**Source**: `docs/audits/AUDIT_FO4_2026-09-05b.md` (FO4-2026-09-05b-D9-01), `/audit-suite --preset per-game-all`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: 9 (real-data validation) / test infrastructure
- **Location**: `crates/nif/src/import/material/fo4_shader_flag_tests.rs`
  (the `#3897` cases) · `byroredux/src/asset_provider/tests/bgsm_merge.rs`
  (the `#3898` cases) · `byroredux/src/cell_loader.rs`
  (`pack_imported_material_flags_tests`) · `crates/renderer/shaders/triangle.frag`
- **Status**: NEW. Adjacent to but distinct from `#3850` (ignored real-data
  tests that report green when data is absent) and from
  `AUDIT_FO3_2026-09-05.md` `FO3-2026-09-05-D2-02` (CI never runs `--ignored`
  at all). Those are about *existing* gates not running; this is about a
  visible-output path that has **no** gate of any kind.
- **Description**: `79194306` added synthetic unit tests at three tiers — the
  NIF flag capture, the BGSM enable-bit merge, and the flag packer — and all
  three pass. None of them observes a colour. The change they collectively
  guard is "30 166 vanilla properties begin taking a shader branch that
  replaces albedo", and the repository has no assertion, corpus census, or
  golden frame that would move if that branch produced the wrong colour, the
  wrong row, or nothing at all. `byroredux/tests/golden_frames.rs` renders the
  cube demo, which has no BGSM and no LUT. The consequence is concrete: D5-01
  is a wrong-albedo defect on tens of thousands of surfaces that ships entirely
  green.
- **Evidence**: the three test sites above assert flag bits and `ImportedMaterial`
  fields only. `golden_frames.rs` is the cube-demo frame-60 PNG. No test in the
  workspace references `greyscaleLutIndex`, `bricks01grad01`, or any palette LUT
  path. The earlier pass reached the same branch and classified it "dead code"
  from source reading alone — correct at the time, and equally unfalsifiable by
  the test suite in either direction.
- **Impact**: any future change to either palette branch — including the D5-01
  fix — lands unverified. The FO4 bench cell (`MedTekResearch01`) is
  hi-tech-panel-heavy, i.e. it is exactly the content
  `hittechmetalpanel_01lgrad.dds`'s 48 materials / 12 rows sit on, so the
  material is available; only the assertion is missing.
- **Related**: `#3897`, `#3898`, `#3850`, `AUDIT_FO3_2026-09-05.md`
  `FO3-2026-09-05-D2-02`, D5-01 above.
- **Suggested Fix**: cheapest useful gate is a corpus census assertion, not a
  frame: an `#[ignore]`d test that walks the FO4 material archives and pins
  "477 palette-enabled BGSMs, N distinct scales over M shared LUTs", so a
  translation change that collapses the parameter shows up as a count moving.
  A screenshot gate on the bench cell is the stronger form and can follow.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other block parsers, other games)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `translate_material` / `Material::resolve_pbr` / the emitter params, per-game logic stays at the NIFAL parser→`Material` boundary
