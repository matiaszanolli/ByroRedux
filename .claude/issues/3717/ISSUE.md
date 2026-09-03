# #3717 — NIF-2026-08-30-D2-01: NiDynamicEffect's pre-4.0.0.2 affected-nodes fields are never read, on the one band where the miss cascades

**Severity**: LOW · **Dimension**: Version Gating
**Location**: `crates/nif/src/blocks/base.rs::NiDynamicEffectData::parse`

## Fix

nif.xml gives `NiDynamicEffect` two affected-nodes field groups; the
parser implemented only the `since="10.1.0.0"` pair. The `until="4.0.0.2"`
pair (`Num Affected Nodes: uint` always, then either `Affected Nodes: Ptr`
for `<= 3.3.0.13` or `Affected Node Pointers: uint` for
`4.0.0.0..=4.0.0.2`) was never read at all, under-reading by
`4 + 4×N` bytes on any `NiLight` / `NiTextureEffect` in that band.

Added the missing arm. Both sub-ranges of the `until="4.0.0.2"` group are
byte-identical on disk — a `count: u32` followed by `count` 4-byte
entries — nif.xml's `Ptr` vs `uint` distinction is a type-annotation
difference (block-index vs raw pointer-hash), not a layout one, so one
read covers both:

```rust
} else if pre_fo4 && stream.version() <= NifVersion::V4_0_0_2 {
    let count = stream.read_u32_le()? as usize;
    stream.read_u32_array(count)?
}
```

Verified the exact field text against the authoritative `nif.xml`
(`/mnt/data/src/reference/nifxml/nif.xml:3497-3505`) before implementing —
matches the issue's own cited evidence verbatim.

## SIBLING (issue's own checklist item)

Checked the rest of the `NiDynamicEffect` block family (`NiDynamicEffect`
itself, `NiLight`, `NiTextureEffect`) in `nif.xml` for any other
`until="4.0.0.2"` field group — none exists; the affected-nodes pair was
the only one. `NiLight` and `NiTextureEffect` both delegate to
`NiDynamicEffectData::parse` (confirmed via `light.rs`/`texture.rs`), so
both subclasses pick up the fix automatically — no separate change
needed at either call site.

## TESTS (issue's own checklist item)

Per the issue's own suggested fix — no vanilla sample exists in this
band (5 files in the corpus, all markers with neither `NiLight` nor
`NiTextureEffect`) — added a synthetic fixture module,
`ni_dynamic_effect_data_version_gate_tests`, matching this file's
existing `niavobject_version_gate_tests` convention (`header_at` helper,
`NifStream::new(&bytes, &header)`, assert exact stream-position
consumption):
- `v4_0_0_2_reads_the_affected_nodes_array` — the `4.0.0.0..=4.0.0.2`
  `uint` sub-range.
- `v3_3_0_13_reads_the_affected_nodes_array` — the `<= 3.3.0.13` `Ptr`
  sub-range, pinning both halves of the group read identically.
- `gap_window_reads_nothing` — the `(4.0.0.2, 10.1.0.0)` window has
  neither field group; mirrors `NiAVObjectData`'s own documented
  gap-window discipline for the sibling `has_bv`/`collision_ref` field.

**Reintroduce-and-revert verification**: temporarily removed the new
`else if` arm — confirmed both `v4_0_0_2_...` and `v3_3_0_13_...` failed
(`affected_nodes` empty instead of the seeded values). Restored the fix
and reran — all 7 tests across both version-gate modules in this file
pass again.

## Verification

- `cargo check -p byroredux-nif --tests`: clean, zero warnings.
- `cargo test -p byroredux-nif --lib blocks::base::`: 7 tests passing, 0
  failing (+3 new).
- `cargo test -q -p byroredux-nif`: 1224 tests passing (+3), 0 failing.
- `cargo test -q --no-fail-fast` (full workspace): **7118 passing, 0
  failing**.
