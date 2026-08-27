# FNV-2026-08-26-D8-01

**Issue**: #3331
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: MEDIUM
**Dimension**: 8 — Real-Data Validation & Bench
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `.claude/commands/audit-fnv/SKILL.md:143`

**Premise verified**: the command as written is

```
cargo run --release -- --esm FalloutNV.esm --cell GSProspectorSaloonInterior \
  --bsa Meshes.bsa --textures-bsa Textures.bsa --textures-bsa Textures2.bsa \
  --bench-frames 300 --bench-hold
```

A vanilla FNV `Data/` directory contains no `Meshes.bsa`, no `Textures.bsa` and no
`Textures2.bsa`. Directory listing of
`/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data/`:

```
Fallout - Meshes.bsa
Fallout - Misc.bsa
Fallout - Sound.bsa
Fallout - Textures.bsa
Fallout - Textures2.bsa
```

`Archive::open` (`byroredux/src/asset_provider/archive.rs:21`) opens the **literal**
path — there is no fuzzy/stem matching — so all three `--bsa`/`--textures-bsa` flags
fail to open, and the run lands in exactly the near-empty-scene failure the skill
warns about one line *above* the command (`SKILL.md:141`).

Two further defects in the same block:

1. **No `--upscaler` flag.** `byroredux/src/cli_args.rs:70` —
   `option("--upscaler")?.unwrap_or("fsr3")` — so the bare command measures **FSR 3.1
   Quality**, while the skill instructs the auditor to "compare … against the ROADMAP
   FNV row", whose headline column is **TAA (native)**. This is the trap ROADMAP
   line 1332 already documents as #2560 / FNV-D8-01 (~254 FPS FSR vs ~145 FPS TAA);
   the annotation lives in ROADMAP but never propagated into the skill.
2. The third `--textures-bsa Textures2…` is **redundant** — see FNV-2026-08-26-D8-03.

Every *real* harness in the tree gets the names right, which is what makes the skill
the sole live defect: `scripts/fsr-bench-matrix.sh:89-93`,
`scripts/renderer-eval-fnv.sh:20-22,56-58`,
`docs/smoke-tests/r6a_stale_15_bench.sh:241-243`, and
`assets/debug_profiles.toml:47-48` all use `"Fallout - Meshes.bsa"` /
`"Fallout - Textures.bsa"`.

**Impact**: the audit dimension whose entire job is to validate the bench-of-record
ships a repro command that produces a 36-entity scene and a spurious FPS number.
An auditor who copy-pastes it and compares the result to the ROADMAP row is comparing
an empty scene (or, if they fix only the names, an FSR figure against a TAA row).

**Fix sketch**: replace the command in `SKILL.md:143` with the CWD-immune profile form
`cargo run --release -- --game fnv --cell GSProspectorSaloonInterior --upscaler taa
--bench-frames 300 --bench-hold` (see FNV-2026-08-26-D8-02), or at minimum quote the
real archive names and add `--upscaler taa`; point the reader at
`scripts/fsr-bench-matrix.sh 3 300` as the authoritative form.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
