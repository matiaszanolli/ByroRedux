# #6966: TD3-2026-11-15-01: CLAUDE.md and game-loop.md state `--radius 1..=7` / "default 3"; the code clamps 1..=12 with default 12

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6966
- **Labels**: low,terrain-exterior,documentation,doc-rot,tech-debt
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW. **Dimension**: 3. **Effort**: trivial. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none (`docs/engine/exterior-grid-streaming.md:28-30` already calls both notes stale, nobody fixed them).
- **Age**: the clamp became `1..=12` in 5916a9d99 (2026-05-30). `docs/engine/game-loop.md:38` carries the old text since 6f73eadb4f (2026-05-28). CLAUDE.md:45 was rewritten on 2026-11-13 (0d17432bd8) and kept the stale range.
- **Evidence**: `byroredux/src/scene.rs:55-58` (`parse_exterior_radius` -> `r.clamp(1, 12)`), `byroredux/src/scene.rs:39` (`DEFAULT_EXTERIOR_RADIUS: i32 = 12`). Stale: `CLAUDE.md:45` ("radius 1..=7"), `docs/engine/game-loop.md:38` ("radius 1..=7, default 3").
- **Also**: `docs/engine/exterior-grid-streaming.md:25-28` cites `scene.rs:75`, `scene.rs:228-263`, `scene.rs:50-55`; those are in range (so `stale_line_references` stays green) but now mean something else (`setup_scene` is `scene.rs:224`, `parse_exterior_radius` `scene.rs:55`, `--grid` is read at `scene.rs:487`). Name the symbols instead of lines.
- **Fix**: CLAUDE.md:45 -> "(radius clamped 1..=12, default 12)", game-loop.md:38 likewise, and replace the three line refs in exterior-grid-streaming.md with `setup_scene` / `parse_exterior_radius`. CLAUDE.md is the file every agent reads first, so do not leave it to the next sweep.
- **Also (orchestrator check)**: the comment above the parse at `byroredux/src/scene.rs:505-506` says "Defaults to 5 ... raised from 3", and the constant it describes (`DEFAULT_EXTERIOR_RADIUS`, `scene.rs:39`) is 12. Fix it in the same commit.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
