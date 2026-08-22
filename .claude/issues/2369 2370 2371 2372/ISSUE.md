# Issues #2369, #2370, #2371, #2372 — EXAL Exterior Readiness Epic (Tranches C & D)

All four are milestone-sized "Plan: EX-XX" tickets from the exterior epic
#2377, tracked in `docs/engine/exterior-readiness-plan.md`. None are scoped
bugs — no Location/Evidence/Suggested-Fix sections, just acceptance criteria.
User explicitly requested implementation plans (not code) for all four.

Master plan status (from docs/engine/exterior-readiness-plan.md):
- Tranche A (make failures reproducible) — DONE.
- Tranche B (make entry/traversal safe) — DONE except item 3 (deadline
  budgeting still open, tracked separately as EX-06/07 / #2376, OPEN, left
  from a prior session).
- **Tranche C (close visual continuity) — EX-10 through EX-15 — only has a
  one-line exit criterion in the doc, no execution plan yet.** Covers #2371
  (EX-10/11) and #2369 (EX-14/15).
- **Tranche D (make the exterior a world, not a render demo) — EX-09, EX-16,
  EX-17 — same: exit criterion only, no execution plan.** Covers #2370
  (EX-09/17) and #2372 (EX-16).

Issue-number-to-EX-ID mapping (confirmed via `gh issue view` against the
doc's "Issue slate" list):
- EX-01/05 → #2368 · EX-02/04 → #2375 (closed) · EX-06/07 → #2376 (open)
- EX-08 → #2374 (closed) · EX-09/17 → **#2370** · EX-10/11 → **#2371**
- EX-12/13 → #2373 (closed) · EX-14/15 → **#2369** · EX-16 → **#2372**

## #2369 — EX-14/15: ground cover/trees, persistent refs, parent worlds, FO4 spatial data
Tranche C. GRAS/REGN placement + full SpeedTree rendering replacing
billboard-only coverage; deterministic/streamed/deadline-budgeted density;
persistent/temporary ref ownership across parent worldspaces; FO4
precombine/previs/occlusion render+collision+fallback+mod-invalidation
coverage; no double geometry between absorbed refs and precombines; soak
telemetry proving clean unload. Depends on near terrain (EX-10), streaming
telemetry (EX-06/07), ownership soak (EX-08, done).

## #2370 — EX-09/17: exterior transitions, save/load, load-order conformance
Tranche D. Interior<->exterior and exterior<->exterior transitions plus
save/load restore worldspace/grid/player/weather/change-forms without
duplicate persistent refs; save/load during active streaming
cancels/rebinds deterministically; master/DLC/mod overrides merge
WRLD/CELL/LAND/REFR/environment records correctly including deleted refs
and partial worldspace overrides; parent-world inheritance preserved across
transitions; base-game/DLC/synthetic override-chain conformance profiles.
Depends on EX-02 (done) and EX-08 (done, ownership soak).

## #2371 — EX-10/11: near-terrain correctness and complete distant LOD bands
Tranche C. Real-data guards for LAND height/normal/vertex-color/splat per
game; no cracks/discontinuities between adjacent near cells; 4/8/16/32
terrain+object LOD band selection with stable hysteresis (ROADMAP M35: only
level 4 currently loads); .btr normal maps + VWD full-model culling;
far-plane/reversed-Z policy for full exterior scale; automated
overlap/double-draw/hole/thrash detection while crossing boundaries.
Calibrate on Oblivion Tamriel, FNV WastelandNV, Skyrim Tamriel, FO4
Commonwealth.

## #2372 — EX-16: integrate REGN, NAVM, ambient audio, AI with exterior streaming
Tranche D. REGN drives ambient sound/fog-weather overlays/ground
cover/encounter metadata with deterministic priority; NAVM tiles load/unload
with cells preserving cross-cell path connectivity; actors/packages
suspend/migrate/resume across stream boundaries without duplication or
dangling refs; ambient audio emitters crossfade and reclaim ownership on
unload; per-cell REGN/NAVM/audio/AI owner telemetry; boundary/soak tests
for an actor path crossing a cell edge mid-unload. Depends on ownership
soak (EX-08, done) + ground-cover streaming (EX-14) + M42/M44 gameplay
systems. Buildable slices already split out: #2737 (EX-16a, REGN RDAT
parse) and #2738 (EX-16b, NAVM geometry+connectivity parse, landed
2026-08-13 per the doc).
