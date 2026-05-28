Surfaced 2026-05-28 in Cydonia render attempts.

## Symptom

Loading \`CityCydoniaMainLevel\` spawns the player at \`(-37.9, 29.0, 19.2)\` — just the cell-origin default + a small offset:

```
[scene] M28.5 player character spawned at (-37.9, 29.0, 19.2); eyes at (5.4, 58.5, 302.9)
```

Compare to FNV \`GSDocMitchellHouse\` (a reference FNV interior) and \`FreesideAtomicWrangler\` from the audit-runtime baseline — both go through the door-teleporter spawn path:

```
M28.5 spawn at door teleporter: door at (3296.0, 13912.0, -2768.0); inward nudge (-53.6, _, 34.9) BU; placing capsule at (3242.4, 13962.0, -2733.1)
```

FNV uses the door teleporter — the player spawns at the cell ENTRANCE, looking inward. Cydonia falls back to cell-origin, which is typically inside the architecture (the player spawns inside the floor / wall geometry).

## Likely root cause

The door-teleporter spawn path in \`byroredux/src/scene.rs\` (probably) looks for a DOOR REFR with a TNAM (teleporter) subrecord and uses its position + an inward nudge. For Starfield:
1. The DOOR REFR may use a different subrecord layout (TNAM-equivalent renamed?)
2. OR Cydonia's main interior cell legitimately has no door teleporter (it's a complex multi-level cell with multiple entrances; the "main" interior isn't reached via a single door)
3. OR Starfield's spaceport / outpost concept uses a different teleporter mechanism than the Fallout/Skyrim door

## Why this is medium not high

Workaround exists: \`Escape\` captures the mouse for the fly camera, and the player can fly to whatever location they want. Doesn't block Cydonia inspection. But for a "render Cydonia" tech demo, the spawn-below-floor is the first impression — and it's even more confusing now that [SF-COL-01](#) will also need fixing before the player can stand anywhere.

## Suggested investigation

1. \`probe_form\` or \`dump_cell_refs\` on the Cydonia cell — find any DOOR REFRs with teleporter data.
2. If they exist: trace why the spawn path doesn't find them. Probably a SF-specific subrecord layout.
3. If they don't exist: Starfield cells may legitimately not use door teleporters as the universal spawn mechanism — needs alternative (XCLW water-level offset? XCMT cell-marker? cell-name-driven manual spawn override?).

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: confirm what FO4 cells do — if FO4 cells DO use door teleporters successfully, then the SF-specific gap is narrower (just the SF subrecord)
- [ ] **TESTS**: regression test that picks a known-good cell (Cydonia or sibling) and asserts spawn position is non-default (not at \`(_, 29.0, _)\`)

## References

- Sibling issue: [SF-COL-01](#) (filed alongside this; static colliders also broken so even a correct spawn wouldn't fix free-fall)
- Cell loader spawn path: \`byroredux/src/scene.rs\` (door-teleporter lookup)
- FNV Atomic Wrangler log (for comparison): [docs/audits/SF_FIRST_RENDER_2026-05-28.md](docs/audits/SF_FIRST_RENDER_2026-05-28.md)
