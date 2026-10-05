# #5323: PAR-D3-2026-10-05-01: The sfmaterial skip path still walks fields in declaration order after #3398 moved the read path to `read_order`

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5323
- **Labels**: low,import-pipeline,nifal,bug,game:starfield
- **Source**: `docs/audits/AUDIT_PARSERS_2026-10-05.md` (PAR-D3-2026-10-05-01)

_From `docs/audits/AUDIT_PARSERS_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW
- **Dimension**: Version Gating
- **Location**: `crates/sfmaterial/src/reader.rs:796-835` (`skip_user_class_body` iterates `field_layout`), compared with `crates/sfmaterial/src/reader.rs:997-1050` (`read_user_class_body` iterates `read_order`). Contract stated at `crates/sfmaterial/src/types.rs:127-134`.
- **Status**: NEW (gap left by `224a19372`)
- **Trigger Input**: a CDB class whose declaration order differs from offset order **and** that has a variable-size inline field (`String`) or a chunk field (`List`/`Map`) among the reordered ones.
- **Description**:
  - `Class::read_order`'s own doc says "any sequential reader MUST walk this order".
  - The skip path still consumes inline bytes, and queues side chunks, in declaration order. The read path and the skip path can therefore disagree on how many bytes a field consumes, or on which `LIST`/`MAPC` belongs to which field.
  - In vanilla, `XMCOLOR` is the only divergent class. It is four `u8`s with no chunk fields, so both orders consume identical bytes, which is why nothing fails today.
- **Evidence**: the two loops above. `validate_instances` (the exact 97 / 1,438,780 pin) runs the skip path. `MaterialIndex::build` skips everything before the index and reads everything after it, so it uses both paths. No test pins skip ≡ read for a reordered class with a variable-size field.
- **Impact**: latent. A mod or Creation CDB with such a class would desync the skip path, giving `ObjectTrailingBytes`/`WrongChunkType`, or a skipped instance that consumes the wrong side chunks. The real-data pin validates only the skip path.
- **Related**: #3398, #4275
- **Suggested Fix**: iterate `read_order` in `skip_user_class_body`'s non-diff arm, as the read path does. Add a reordered-class fixture with a `String` field and a `List` field, and assert that skip and read consume the same bytes and chunks.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
