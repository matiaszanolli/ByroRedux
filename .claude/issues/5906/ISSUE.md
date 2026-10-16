# #5906: PAR-D4-2026-10-15-06: `docs/feature-matrix.md` and ROADMAP's crate table still say FO4 `.hkx` is unread and that there is no behavior-graph loading

- **Labels**: low,documentation,doc-rot,animation,game:fo4
- **Filed from**: `docs/audits/AUDIT_PARSERS_2026-10-15.md`

- **Severity**: LOW
- **Dimension**: Corpus Gates (doc claims)
- **Location**:
  - `docs/feature-matrix.md:82-87`: "awaits a Havok `.hkx` loader for FO4's packfile layout … covers Skyrim only".
  - `docs/feature-matrix.md:132-142`: row `~` "Skyrim LE + SE", "no behavior-graph loading or execution … FO4 and Starfield `.hkx` remain unread".
  - `docs/feature-matrix.md:327`, `:341`: the gaps table lists the "Havok `.hkx` loader (FO4 / Starfield layouts)".
  - `docs/feature-matrix.md:98`: "FO4+ still slides".
  - `ROADMAP.md:789`: `byroredux-hkx` described as "(Skyrim LE/SE rigs and clips)".
- **Status**: NEW. The file was last touched at ce130a7cf (2026-10-15), after a7bbba273 / 60082cc55 / 49529db7c (2026-10-14) shipped the FO4 Havok 2014 clip, skeleton and behavior-graph readers. `ROADMAP.md:102` is already correct ("FO4's run through its Havok 2014 behavior graphs").
- **Trigger Input**: n/a.
- **Description**:
  - No BGSM/BA2/CDB/HKX *rate* claim without a date was found. The only numeric rates in `ROADMAP.md:218-220` are NIF-level and dated.
  - The capability claims for HKX, however, contradict the code and `ROADMAP.md:102`.
- **Evidence**: `git log -1 -- docs/feature-matrix.md` returns ce130a7cf, dated 2026-10-15. The `.hkx` rows still predate the FO4 commits.
- **Impact**: doc accuracy; readers of the feature matrix believe FO4 actors cannot animate from `.hkx`.
- **Related**: PAR-D4-2026-10-15-40.
- **Suggested Fix**: update the four feature-matrix spots and the ROADMAP crate row. Cite the FO4 census (D4-40) once it lands.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
