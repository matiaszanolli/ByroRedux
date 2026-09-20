# EXT-D7-2026-09-19-08: missing textures / failed NIFs are WARN-only while a chrome frame satisfies every hard gate

- **ID**: EXT-D7-2026-09-19-08
- **Labels**: low,tech-debt,bug
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4508

**Severity**: LOW · **Dimension**: Acceptance harness · **Game Affected**: all
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D7-2026-09-19-08)

**Location**: `docs/smoke-tests/m-exteriors.sh:361-369,764-769`; protocol at workspace AGENTS.md ("Chrome … run `tex.missing` first")

**Description**
The documented soft-split (missing textures / failed NIFs → WARN) is deliberate, but the interaction matters: a checkerboard-placeholder × normal-map exterior — the project's own named "chrome" failure mode — has sd far above `image_health`'s 0.005 floor and full population, so it passes every hard gate with a WARN. The FNV static baseline sd is itself only 0.0160 (≈4/255), already near the washout regime, so the hard image gate's headroom over "washed/chrome" is thin.

**Impact**
A texture-pipeline regression ships green from the exterior matrix; only a human reading `debug.log` classifies it.

**Suggested Fix**
Keep content-drift WARNs, but add a ceiling — hard-fail when `missing_textures` ≥ 2× the profile's calibrated baseline.

## Completeness Checks
- [ ] **TESTS**: A seeded over-baseline `tex.missing` run fails the script
