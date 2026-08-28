# #3424 — RT-2026-08-27-07: light.dump has emitted a point-light tally for 13 days, the skill says it does not, and light_count_directional is a dead gate

Labels: low, doc-rot, test-gap, tech-debt, bug
Filed: 2026-08-27 by `/audit-publish docs/audits/AUDIT_RUNTIME_2026-08-27.md`
Source report: `docs/audits/AUDIT_RUNTIME_2026-08-27.md`

---

Source: `docs/audits/AUDIT_RUNTIME_2026-08-27.md` — RT-2026-08-27-07 (live headless runs at `969d81c8`).

- **Severity**: LOW
- **Dimension**: audit-infrastructure doc-rot / test gap
- **Location**: `.claude/commands/audit-runtime/SKILL.md` (Phase 3 metric table :177 + the `light.dump` quirk note :206-209); emitter at `byroredux/src/commands/scene.rs:195`

## Description

The skill states that `light.dump` "surfaces the one directional sun, not a per-point-light tally, so `light_count_directional` is effectively a constant 1 and there is no `light_count_point`". Since `5f970bae` (2026-08-15) the command prints `LightSource emitters: {}` plus a full per-emitter dump (kind, source, position, radiance, dimmer, range, attenuation, visibility and flag words):

```rust
lines.push(format!("LightSource emitters: {}", emitters.len()));
```

This audit captured it on every game: `fnv` 30, `fo3` 11, `oblivion` 8, `skyrim_se` 28, `fo4` 685.

The other half is worse than stale. All five baselined cells are interiors; every one dumps `directional_color = [0.000, 0.000, 0.000]`, and the baselined `light_count_directional` row is not read from any printed count — the skill's Phase 3 table sources it as "`light.dump` `CellLightingRes` (always 1 sun)", i.e. it is inferred from the mere presence of a `CellLightingRes` block. The row therefore cannot fail, on any cell, ever. It is a gate that measures nothing while a real, discriminating per-cell light count sits uncaptured beside it.

## Impact

One of eight structural gates is inert, and a genuinely useful one (a light count is exactly the sort of thing a cell-loader or LIGH-parsing regression moves) is not collected. `fo4`'s 685 emitters against `oblivion`'s 8 shows the metric has real dynamic range.

## Suggested Fix

Add a `light_count_point` row sourced from `LightSource emitters: N`, direction "exact match"; either drop `light_count_directional` or redefine it as the count of `kind=Directional` entries in the same dump so it is actually parsed rather than assumed. Update the skill's Phase 3 quirk note.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Phase 3 metric rows sourced by inference rather than by parsing a printed value)
- [ ] **TESTS**: A regression test pins this specific fix
