# Tech-Debt: ESM parse-but-don't-consume — 4 new fields + #1047 consumer-side follow-ups

**Labels**: medium, tech-debt
**Status**: Open

## Description

[#1047](https://github.com/matiaszanolli/ByroRedux/issues/1047) tagged parse-but-don't-consume subrecords with `// MILESTONE: …` comments so future audits could grep for them. The tagging itself shipped; this issue tracks the consumer-side follow-ups (the actual milestone deliveries) plus 4 net-new cases the 2026-05-14 audit found that the #1047 sweep missed.

## Net-new cases (Session 36 audit)

### TD5-010 — `OblivionHdrLighting` parsed but not consumed
- **Re-export site**: `crates/plugin/src/esm/records/mod.rs:66`
- **Producer**: `crates/plugin/src/esm/records/weather.rs::parse_wthr` decodes the HDR sub-block
- **Consumer**: zero. `weather_system` in `byroredux/src/systems/weather.rs` reads the SDR colors but not the HDR ones
- **Gating milestone**: M-LIGHT v2 (HDR sky / cloud relighting). Tag the producer with `// MILESTONE: M-LIGHT v2 — see #NNNN`

### TD5-011 — TREE.SNAM (leaf-index animation list) + TREE.CNAM (canopy curve)
- **Producer**: `crates/plugin/src/esm/records/tree.rs:23,86` — fields are decoded into the record but never read by the SpeedTree placeholder
- **Consumer**: `crates/spt/src/import.rs` ignores both — placeholder billboard doesn't animate
- **Gating milestone**: SpeedTree Phase 2 (real leaf animation). Tag with `// MILESTONE: SpeedTree Phase 2`

### TD5-013 — NPC_ face_morphs decoded but no GPU consumer
- **Producer**: `crates/plugin/src/esm/records/misc/character.rs` extracts the per-NPC morph weight array
- **Consumer**: `byroredux/src/npc_spawn.rs` ignores them; FaceGen Phase 4 (#794 family) didn't wire this path
- **Gating milestone**: M41.0.5 (per-vertex morph runtime). Tag accordingly

### TD5-016 — BPTD body-part data parsed but unused
- **Producer**: `crates/plugin/src/esm/records/mod.rs:1101` extracts the BPTD records
- **Consumer**: none. Dismemberment routing + biped slots from BPTD never reach the physics or render layer
- **Gating milestone**: Tier-7 ragdoll/dismemberment. Tag accordingly

## #1047 consumer-side follow-ups (TD5-001..003 family)

#1047 itself closed via the tag-don't-implement convention. The actual consumer wiring is its own work, deferred until each gating milestone fires. Inventory:

| Tag | Consumer site (when ready) | Gating milestone |
|---|---|---|
| TD5-001 (CLMT TNAM weather-hour curve) | `weather_system::interpolate_at_hour` already exists; needs per-WRLD CLMT lookup | M33 phase 2 (done — verify wiring) |
| TD5-002 (REGN region-keyed ambient sounds) | `audio_system` needs a `current_region` query | M44 phase 7 (REGN-driven ambient) |
| TD5-003 (LIGH XPWR power-circuit ref) | requires power-grid simulation | Tier-7 |

The 4 new tags above (TD5-010..016) add to the same tag-and-defer pool.

## Severity rationale

**MEDIUM** (promoted from default LOW). The "duplicated logic with divergent bug-fix history" amplification trigger fires: TD5-013 nearly had a second face_morphs decoder added during M41.0.5 scaffolding (the original site at \`character.rs\` was already pulling the bytes; a naive M41.0.5 patch would've added a second parser instead of wiring the existing one). Tag-and-grep cost: small. Cost of stepping on this rake: a partial-double-parse that survives review.

## Proposed fix

For each net-new case (TD5-010, TD5-011, TD5-013, TD5-016): add the same `// MILESTONE: <name> — see this issue` comment shape #1047 established at the producer site. Defer the consumer-side wiring to the gating milestone's own issue.

## Completeness Checks

- [ ] **UNSAFE**: n/a
- [ ] **SIBLING**: After tagging, grep for `// MILESTONE:` across `crates/plugin/src/esm/records/` and confirm the count matches expectations
- [ ] **DROP**: n/a
- [ ] **LOCK_ORDER**: n/a
- [ ] **FFI**: n/a
- [ ] **TESTS**: no regression risk — tags are comments only

## Dedup notes

Sibling of **#1047** (CLOSED — pattern + first 3 tags). Distinct because the 4 net-new sites weren't in scope of #1047's sweep, and the consumer-side follow-up needs a tracker now that the tags are in place.
Status: Closed (fc8c9d7)
