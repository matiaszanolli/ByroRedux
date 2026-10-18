# ESM-2026-10-18-EXT-05: the Skyrim cloud-layer FourCC index treats `'A'` as layer 10, but the real sequence is the ASCII run `'0'..'9', ':'..'@', 'A'..'O'`

Labels: low,esm-plugin,terrain-exterior,bug,game:skyrim
Source: docs/audits/AUDIT_ESM_2026-10-18.md

- **Severity**: LOW. The full 32-slot table has no non-test consumer. Layers 0 to 9 (digits) are right, so the four layers the renderer uses are unaffected.
- **Dimension**: Sub-Record Byte Accounting
- **Record / Sub-record**: `WTHR` / `:0TX`..`O0TX`
- **Location**: `weather.rs:879-888` (`skyrim_cloud_layer_index`); test `weather.rs:1960` pins `A0TX` as layer 10 and `V0TX` as layer 31
- **Status**: NEW
- **Description**: xEdit's list is `_30_0TX`..`_39_0TX` (layers 0 to 9), `_3A_0TX`..`_40_0TX` (layers 10 to 16, the bytes `':' ';' '<' '=' '>' '?' '@'`), then `A0TX` = layer 17 up to `O0TX` = layer 31. The code maps only digits and `'A'..='V'`, with `'A'` = 10. So layers 10 to 16 are dropped and layers 17 to 28 are stored 7 slots too low. `'V'` does not exist on the wire.
- **Evidence**: census shows `:0TX`..`@0TX` present on Skyrim weathers (`:0TX` ×49 records, `@0TX` ×23, and so on), `A0TX`..`L0TX` present and `M0TX`..`O0TX` absent, consistent with `LNAM Max Cloud Layers = 29` (10 + 7 + 12).
- **Suggested Fix**: index = `byte - b'0'` for the first byte of the FourCC, with the ASCII range ending at `'O'`; update the test.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_ESM_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
