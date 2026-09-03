# #3625 — OBL-D4-02: NiTexturingProperty Apply Mode values 1 and 3 are decoded and then dropped (681 Oblivion properties)

**Severity**: LOW · **Dimension**: Rendering Path for Oblivion Shaders
**Location**: `crates/nif/src/blocks/properties.rs::NiTexturingProperty::apply_mode`

## Fix

The issue's own suggested fix is explicit: **"No heuristic is proposed
and none should be invented"** (this project's no-guessing policy) —
either establish the semantics from a primary source before consuming
values 1/3, or document them as deliberately dropped. No primary source
for the real Oblivion-PC semantics of `APPLY_DECAL`/`APPLY_HILIGHT`
exists (value 3 is PS2-only per nif.xml, and Gamebryo v3.2 renamed both
3 and 4 to `APPLY_DEPRECATED`/`APPLY_DEPRECATED2`), so applied the
documentation option: extended `apply_mode`'s doc comment with the
measured histogram (`APPLY_DECAL = 18`, `APPLY_HILIGHT = 663`,
`APPLY_HILIGHT2 = 1,274` out of 30,121 instances) and an explicit note
that the 681 non-default-non-HILIGHT2 properties are deliberately
unconsumed pending a primary source, not silently forgotten.

Left `legacy_properties.rs`'s consumer site untouched — it makes no
claim about values 1/3 either way, so there was nothing misleading to
correct there (unlike #3544's stale "compositor exists" claim).

## TESTS (issue's own checklist item — conditional: "if either value is
later consumed, a regression test pins the decode and the downstream
material effect")

Neither value was consumed, so that conditional test doesn't apply. Added
a source-scan regression instead:
`apply_mode_doc_records_the_unconsumed_value_measurement` pins that the
`apply_mode` field's doc comment still carries the `#3625` marker and the
three measured histogram numbers — so a future edit that trims the doc
(e.g. during an unrelated cleanup pass) doesn't silently lose the only
record that this gap was measured rather than unknown.

**Reintroduce-and-revert verification**: temporarily removed the added
doc paragraph — confirmed the new test failed with the expected message.
Restored the fix and reran — all 33 tests in
`blocks::properties::tests` pass again.

## Verification

- `cargo check -p byroredux-nif --tests`: clean, zero warnings.
- `cargo test -q -p byroredux-nif --lib blocks::properties::tests::`: 33
  passing, 0 failing (+1 new).
- `cargo test -q --no-fail-fast` (full workspace): **7171 passing, 0
  failing**.
