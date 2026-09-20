# PEX-D1-2026-09-19-01: exhaustive-prefix guard samples never reach the FO4+ debug skip paths, full-property bodies, struct infos, or Value::Float

- **ID**: D1-01
- **Labels**: low,scripting,bug,test-gap
- **Filed from**: docs/audits/AUDIT_PAPYRUS_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4474

**Severity**: LOW (test gap; the paths are corpus-proven correct today) · **Dimension**: PEX Reader · **Untrusted-Input**: Yes
**Source**: `docs/audits/AUDIT_PAPYRUS_2026-09-19.md` (PEX-D1-2026-09-19-01) · **Location**: `crates/pex/src/lib.rs:892-918` (sample array); gaps at `crates/pex/src/reader.rs:257-283` (`skip_property_groups`/`skip_struct_orders`), `reader.rs:413-419` (full getter/setter wire bodies), `reader.rs:342-362` (`read_struct_infos`), `reader.rs:173` (`Value::Float`)

**Description**
The #3942 exhaustive-prefix test enumerates every prefix of 4 handbuilt samples, but none of the samples contains: FO4+/Starfield debug info (so `skip_property_groups`/`skip_struct_orders` never run — the extender sample is BE, which stops before them, and the LE samples are debug-absent), a non-auto property with getter/setter *bodies on the wire*, non-empty `struct_infos` (the Starfield sample has count 0), or a `Value::Float` (tag 4) operand. A one-byte regression in any of those desyncs the stream silently and `cargo test -p byroredux-pex` stays green. Same class: the FO76 dialect (`game_id 3`) appears in zero corpus files and no handbuilt sample.

**Evidence**
Dim-1 sample-builder analysis (`build_sample` debug `u8(0)` + auto-property only; `build_sample_skyrim_be` BE ⇒ skip fns unreachable; `build_sample_starfield_with_guards` `u16(0)` struct infos, Integer-only values). The paths *work*: a corpus probe parsed 12,308 LE+debug vanilla files and 4,740 Starfield files with 0 failures — this is a regression-protection gap, not a live bug.

**Impact**
A future skip-reader refactor (the "one wrong u16" class) ships silently broken for FO4/FO76/Starfield debug-compiled scripts — mis-decoded objects or errors on real game files only.

**Related**: #3942

**Suggested Fix**
Add two samples to the prefix test's array: an FO4/Starfield file with debug present including non-empty property groups + struct orders, and one with a full (non-auto) property carrying getter/setter bodies, a non-empty struct info with members, and a `Value::Float` operand (~30 lines each on the existing `PexWriter`).

## Completeness Checks
- [ ] **SIBLING**: Consider an FO76 (`game_id 3`) sample while touching the builders
- [ ] **TESTS**: The new samples run through `every_prefix_of_every_wire_valid_sample_is_rejected`
