# #5878: PEX-D3-2026-10-15-02: the auto-property default lookup (a649b55b4) rescans `object.variables` once per property

- **Labels**: medium,bug,scripting,performance
- **Filed from**: `docs/audits/AUDIT_PAPYRUS_2026-10-15.md`

**Full title**: the auto-property default lookup (a649b55b4) rescans `object.variables` once per property. That is O(P × V), the #5683 class again: a 1.77 MB `.pex` stalls `decompile_script` for 39 s

- **Severity**: MEDIUM (same class and rating as the baseline's PEX-D3-2026-10-12-01 / #5683)
- **Dimension**: Decompiler Boolean/Control-Flow/Lower
- **Untrusted-Input**: Yes
- **Location**: `crates/pex/src/decompile/lower.rs:460-464` (`lower_property`, `initial_value`)
- **Status**: NEW (introduced by a649b55b4, after the baseline's "this is the only quadratic site in `lower.rs`" grep)
- **Description**: `prop.auto_var_name … .and_then(|var| object.variables.iter().find(|v| v.name.eq_ignore_ascii_case(var)))` runs once per property. Both tables are bounded only by their `u16` wire counts. The parse string budget does not help, because one shared string index serves every name. #5683's `UserFlagMasks` fixed the user-flag rescan, but this lookup was added afterwards. The #5683 pin `a_hostile_user_flag_table_decompiles_in_linear_time` (`lower.rs:816`) builds no properties, so it cannot see this.
- **Evidence**: The probe (`quad` mode) has N variables named `::aaaaaaaaaaX` and N auto properties whose backing name `::aaaaaaaaaaY` never matches (same length, so each comparison runs to the last byte). FO4 format, release build:

  | N | file | `parse` | `decompile_script` |
  |---|---|---|---|
  | 4,000 | 108 KB | 1.7 ms | 0.15 s |
  | 8,000 | 216 KB | 2.4 ms | 0.60 s |
  | 16,000 | 432 KB | 4.6 ms | 2.36 s |
  | 32,000 | 864 KB | 8.7 ms | 9.43 s |
  | 65,535 | **1.77 MB** | 19 ms | **39.0 s** |

  Doubling the input multiplies the time by about 4. Vanilla is unaffected: the largest P×V in the census is 358,801 (Skyrim).
- **Impact**: Same as #5683. A mod script stalls its loader thread for tens of seconds, and no net catches it.
- **Related**: #5683 (fixed, same class), #3938
- **Suggested Fix**: Build a case-insensitive `name → &Variable` map once per object, so the cost is O(P + V). Fix -04 needs the same map, because it reads the backing variable's flags too. Add auto properties to the hostile linear-time test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
