# Investigation: FO4-DIM6-06 / #591 — NPC face-morph capture

## Audit premise verification — partial mismatch

Wrote `crates/plugin/examples/dump_npc_subs.rs` and ran against real
`Fallout4.esm` to ground-truth the audit's sub-record layouts before
implementing. **Several audit claims are stale.**

### What's actually in vanilla FO4 NPC records (verified bytes)

| sub-record | audit claim                       | actual (vanilla FO4)                                  |
| ---------- | --------------------------------- | ----------------------------------------------------- |
| `FMRI`     | i32 morph-range index             | **u32 FormID**, 4 bytes ✓                             |
| `FMRS`     | f32 0..1 slider value             | **9 × f32** (36 bytes) — pos[3] + rot[3] + scale[3] ✗ |
| `MSDK`     | morph-slider keys (kv)            | **u32 array** (FormIDs), variable length ✓            |
| `MSDV`     | morph-slider values (kv)          | **f32 array** parallel to MSDK ✓                      |
| `NAM9`     | 19 × f32 nose/cheek/jaw           | **NOT FOUND** on any sampled named FO4 NPC ✗          |
| `QNAM`     | face texture set ref              | **4 × f32 RGB+alpha** (texture lighting tint), 16 B   |
| `NAMA`     | face texture set ref              | **NOT FOUND** on any sampled named FO4 NPC ✗          |
| `FTSM`     | face texture set ref              | **NOT FOUND** on any sampled named FO4 NPC ✗          |
| `BCLF`     | body color override               | u32 FormID (claim plausible; not on sampled NPCs) ⚠   |
| `HCLF`     | (not in audit)                    | **u32 FormID** — hair color ✓ added                   |
| `PNAM`     | (not in audit)                    | **u32 FormID** per head part (multiple) ✓ added       |

### Sample data

`MQ101KelloggScene_PlayerDuplicate` (form `0020CE37`):
- 30 × FMRI (120 B) + 30 × FMRS (1080 B) → 30 paired morph entries
- 1 × MSDK (36 B) + 1 × MSDV (36 B) → 9 slider key/value pairs
- 7 × PNAM (28 B) → 7 head-part FormIDs
- 1 × HCLF (4 B), 1 × QNAM (16 B)

`Hancock` (form `00022613`): 6 paired morphs, 3 PNAM, HCLF + QNAM.
`Piper`-prefixed records: similar shape, varying morph counts.

NAM9, NAMA, FTSM, BCLF were not present on any of the seven named
companions sampled. They may exist on Skyrim records (older format) or
on records the sample didn't reach. **Capturing what verifiably exists**
beats capturing the audit's full theoretical list with wrong layouts.

## Fix design

```rust
pub struct NpcFaceMorph {
    /// FMRI — morph-target FormID (HDPT or face-morph-data form).
    pub form_id: u32,
    /// FMRS — 9 floats per morph: position[3], rotation[3], scale[3].
    pub setting: [f32; 9],
}

pub struct NpcFaceMorphs {
    /// FMRI / FMRS appear in alternating order on the wire and pair
    /// 1-to-1. Mismatched counts truncate to the shorter of the two
    /// (defensive against malformed records).
    pub morphs: Vec<NpcFaceMorph>,
    /// MSDK + MSDV — parallel slider key / value arrays.
    pub slider_keys: Vec<u32>,
    pub slider_values: Vec<f32>,
    /// QNAM — RGB texture-lighting tint + alpha (4 × f32).
    pub texture_lighting: Option<[f32; 4]>,
    /// HCLF — hair color FormID.
    pub hair_color: Option<u32>,
    /// BCLF — body color FormID (rare on vanilla; preserved when present).
    pub body_color: Option<u32>,
    /// PNAM — head-part FormIDs (multiple).
    pub head_parts: Vec<u32>,
}
```

`face_morphs: Option<NpcFaceMorphs>` on `NpcRecord`. Set to `Some(_)`
only when at least one of the captured sub-records was present (most
generic settler NPCs have none — only named NPCs).

The audit's NAM9 / NAMA / FTSM capture is dropped from this fix —
re-add only when a real FO4 record turns up that ships them, with
verified bytes.

## Scope

- 1 file: `crates/plugin/src/esm/records/actor.rs` — extend `NpcRecord`,
  `parse_npc`; add `NpcFaceMorph` + `NpcFaceMorphs`; tests inline.
- 1 new file: `crates/plugin/examples/dump_npc_subs.rs` — the diagnostic
  scout. Useful long-term for the upcoming HDPT/RACE work.
- No new deps. RACE-side defaults (audit's SIBLING completeness item)
  deferred to a separate fix — same scout would need to confirm formats.
