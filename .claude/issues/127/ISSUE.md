# NIF-211: bhkRigidBody body_flags threshold 76 should be 83 per nif.xml

## Finding

**Audit**: NIF 2026-04-05b | **Severity**: LOW | **Dimension**: Version Handling

**Location**: `crates/nif/src/blocks/collision.rs:185`
**Game Affected**: None in practice (no game uses BSVER 35-75)

Body flags change from u32 to u16 at SKY_AND_LATER (BSVER >= 83 per nif.xml). Code uses threshold 76. One-line fix: change `bsver < 76` to `bsver < 83`.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
