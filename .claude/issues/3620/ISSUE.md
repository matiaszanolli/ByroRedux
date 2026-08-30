# #3620 — REN-2026-08-30-D18-03: `environmentSky`'s doc cites a `triangle.frag` line `#3323` renamed, and mislabels the window-portal escape as a "background write"

**Labels**: `low,renderer,shaders,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3620 --json state`.

---

- **Severity**: LOW
- **Dimension**: Sky / weather / exterior lighting
- **Location**: `crates/renderer/shaders/include/lighting.glsl:345-347`
- **Status**: NEW
- **Description**: The `#3162` irradiance-units comment justifying why `skyTint` is
  left untouched by the `1/PI` conversion says: *"`skyTint` is already rendered sky
  radiance (see `triangle.frag`'s `skyColor = skyTint.rgb` background write)"*. No
  such line exists at HEAD. `#3323` (commit `19813460`'s predecessor set) rewrote it to
  `vec3 skyColor = exteriorSkyTint.rgb;` (`triangle.frag:1685`), and it is not a
  background write at all — it is the glass window-portal escape branch, which the
  same `#3323` comment block explicitly warns must **not** be generalised
  (*"Do not swap the rest of this shader onto it: everything else reading a stale
  exterior sky from inside is the `#2226` leak"*).
- **Evidence**: `grep -n "skyTint" crates/renderer/shaders/triangle.frag` returns no
  assignment of `skyColor` from `skyTint`; the live `skyTint` consumers are the two
  RT-miss blends (`triangle.frag:1850`, `:2188`), the two `sunAngularRadius` reads
  (`:3336`, `:3514`), and `include/raytrace.glsl:46`.
- **Impact**: The unit-space argument that keeps the Skyrim-DALC and FO3/FNV/Oblivion
  escape paths at parity now points at a line that does not exist, in a branch with
  the opposite interior/exterior contract. A reader following the citation to confirm
  the units invariant lands in the one place the codebase says is a special case.
- **Suggested Fix**: Re-point at the actual radiance-space evidence — the RT-miss
  blend at `triangle.frag:1850` (`skyTint.xyz * 0.5 + sceneFlags.yzw * 0.5`) and its
  `raytrace.glsl:46` twin — and drop the "background write" wording.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D18-03

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
