# #5296: ESM-2026-10-05-D2-02: TRNS "Around Origin" is decoded from header bit `0x8000`, but every vanilla TRNS that sets it uses `0x10000` (xEdit flag index 16)

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5296
- **Labels**: low,esm-plugin,bug,game:fo4,game:starfield
- **Source**: `docs/audits/AUDIT_ESM_2026-10-05.md` (ESM-2026-10-05-D2-02)

_From `docs/audits/AUDIT_ESM_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW. The field is decoded but nothing consumes it.
- **Dimension**: Sub-Record Byte Accounting (record-header flags)
- **Record / Sub-record**: `TRNS` / record header
- **Location**: `crates/plugin/src/esm/records/load_screen.rs:146,159,178`; `crates/plugin/src/esm/records/load_screen/tests.rs:228-232`
- **Status**: NEW
- **Description**: xEdit FO4 declares `wbRecord(TRNS, 'Transform', wbFlags(wbFlagsList([{0x00008000} 16, 'Around Origin'])), …)`. `wbFlagsList` takes the bit **index** (16 = `0x10000`). The `{0x00008000}` brace comment is a typo in xEdit itself; every other entry in these lists pairs `{1 << n}` with `n`. The decoder followed the comment (`flags & 0x8000`), and its unit test feeds `0x8000` and asserts `around_origin`, which pins the wrong bit.
- **Evidence**: census of TRNS header flags (`scripts/trns.py`):

  | master | TRNS records | flags `0x10000` | flags `0x8000` |
  |---|---|---|---|
  | `Fallout4.esm` | 949 | 50 | 0 |
  | `DLCRobot.esm` | 15 | 1 | 0 |
  | `DLCCoast.esm` | 85 | 6 | 0 |
  | `DLCNukaWorld.esm` | 133 | 5 | 0 |
  | `DLCworkshop01.esm` | 65 | 0 | 0 |
  | **FO4 total** | | **62** | **0** |
  | `Starfield.esm` | 712 | 19 | 0 |

  On Starfield, 32 more records carry `0x30000` (which includes the `0x10000` bit) and 291 carry `0x20000`, an SF-only bit.
- **Impact**: `around_origin` is `false` on every record. The loading-cover model stage constructs it as `false` and does not read it (`byroredux/src/loading_screen.rs:907,1058`). Once it is honoured, the 62 FO4 screens authored "around origin" would pivot on bounds centre instead.
- **Related**: #5229 (TRNS radians, closed), e60911864 (LSCR model cover).
- **Suggested Fix**: Test `flags & 0x0001_0000`. Change the docs to "header bit 16 (`0x10000`); xEdit's `{0x00008000}` comment is wrong". Fix the test fixture, and add a real-bytes pin, e.g. one of the 50 `Fallout4.esm` records.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
