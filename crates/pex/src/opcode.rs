//! Papyrus bytecode opcodes (Champollion `Pex::OpCode` + the `OPCODES`
//! metadata table). The discriminant order is the on-disk encoding — an
//! instruction's leading byte indexes directly into [`OpCode`], so the
//! order here must match Champollion exactly.

/// A Papyrus VM opcode. Discriminants are the on-disk byte values
/// (`NOP == 0` … `TRY_LOCK_GUARDS == 50`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OpCode {
    Nop = 0,
    IAdd,
    FAdd,
    ISub,
    FSub,
    IMul,
    FMul,
    IDiv,
    FDiv,
    IMod,
    Not,
    INeg,
    FNeg,
    Assign,
    Cast,
    CmpEq,
    CmpLt,
    CmpLte,
    CmpGt,
    CmpGte,
    Jmp,
    JmpT,
    JmpF,
    CallMethod,
    CallParent,
    CallStatic,
    Return,
    StrCat,
    PropGet,
    PropSet,
    ArrayCreate,
    ArrayLength,
    ArrayGetElement,
    ArraySetElement,
    ArrayFindElement,
    ArrayRFindElement,
    // New in Fallout 4
    Is,
    StructCreate,
    StructGet,
    StructSet,
    ArrayFindStruct,
    ArrayRFindStruct,
    ArrayAdd,
    ArrayInsert,
    ArrayRemoveLast,
    ArrayRemove,
    ArrayClear,
    // New in Fallout 76
    ArrayGetAllMatchingStructs,
    // New in Starfield
    LockGuards,
    UnlockGuards,
    TryLockGuards,
}

/// One past the last valid opcode byte (Champollion `MAX_OPCODE`).
pub const MAX_OPCODE: u8 = 51;

/// `(mnemonic, fixed-arg count, has-varargs)` for every opcode, indexed by
/// discriminant. A verbatim port of Champollion's `OPCODES` table — the
/// arg counts are the reader's contract for how many operands to consume.
const OPCODES: [(&str, u8, bool); MAX_OPCODE as usize] = [
    ("nop", 0, false),
    ("iadd", 3, false),
    ("fadd", 3, false),
    ("isub", 3, false),
    ("fsub", 3, false),
    ("imul", 3, false),
    ("fmul", 3, false),
    ("idiv", 3, false),
    ("fdiv", 3, false),
    ("imod", 3, false),
    ("not", 2, false),
    ("ineg", 2, false),
    ("fneg", 2, false),
    ("assign", 2, false),
    ("cast", 2, false),
    ("cmp_eq", 3, false),
    ("cmp_lt", 3, false),
    ("cmp_lte", 3, false),
    ("cmp_gt", 3, false),
    ("cmp_gte", 3, false),
    ("jmp", 1, false),
    ("jmpt", 2, false),
    ("jmpf", 2, false),
    ("callmethod", 3, true),
    ("callparent", 2, true),
    ("callstatic", 3, true),
    ("return", 1, false),
    ("strcat", 3, false),
    ("propget", 3, false),
    ("propset", 3, false),
    ("array_create", 2, false),
    ("array_length", 2, false),
    ("array_getelement", 3, false),
    ("array_setelement", 3, false),
    ("array_findelement", 4, false),
    ("array_rfindelement", 4, false),
    ("is", 3, false),
    ("struct_create", 1, false),
    ("struct_get", 3, false),
    ("struct_set", 3, false),
    ("array_findstruct", 5, false),
    ("array_rfindstruct", 5, false),
    ("array_add", 3, false),
    ("array_insert", 3, false),
    ("array_removelast", 1, false),
    ("array_remove", 3, false),
    ("array_clear", 1, false),
    ("array_getallmatchingstructs", 6, false),
    ("lock_guards", 0, true),
    ("unlock_guards", 0, true),
    ("try_lock_guards", 1, true),
];

impl OpCode {
    /// Decode a leading opcode byte. `None` if out of range (Champollion
    /// throws on `opcode >= MAX_OPCODE`).
    pub fn from_u8(byte: u8) -> Option<OpCode> {
        if byte >= MAX_OPCODE {
            return None;
        }
        // SAFETY: `OpCode` is `#[repr(u8)]` with contiguous discriminants
        // `0..MAX_OPCODE`, and `byte` is checked in range above.
        Some(unsafe { std::mem::transmute::<u8, OpCode>(byte) })
    }

    /// The mnemonic (`"callmethod"`, `"iadd"`, …).
    pub fn name(self) -> &'static str {
        OPCODES[self as usize].0
    }

    /// Number of fixed operands the instruction carries.
    pub fn arg_count(self) -> usize {
        OPCODES[self as usize].1 as usize
    }

    /// Whether a trailing var-arg operand list follows the fixed operands.
    pub fn has_varargs(self) -> bool {
        OPCODES[self as usize].2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discriminants_match_on_disk_order() {
        assert_eq!(OpCode::Nop as u8, 0);
        assert_eq!(OpCode::CallMethod as u8, 23);
        assert_eq!(OpCode::Is as u8, 36);
        assert_eq!(OpCode::TryLockGuards as u8, 50);
        assert_eq!(MAX_OPCODE, 51);
    }

    #[test]
    fn from_u8_round_trips_and_rejects_oob() {
        for b in 0..MAX_OPCODE {
            assert_eq!(OpCode::from_u8(b).unwrap() as u8, b);
        }
        assert!(OpCode::from_u8(MAX_OPCODE).is_none());
        assert!(OpCode::from_u8(255).is_none());
    }

    #[test]
    fn metadata_matches_champollion() {
        assert_eq!(OpCode::Nop.arg_count(), 0);
        assert!(!OpCode::Nop.has_varargs());
        assert_eq!(OpCode::IAdd.arg_count(), 3);
        assert_eq!(OpCode::CallMethod.arg_count(), 3);
        assert!(OpCode::CallMethod.has_varargs());
        assert_eq!(OpCode::CallParent.arg_count(), 2);
        assert!(OpCode::CallParent.has_varargs());
        assert_eq!(OpCode::ArrayGetAllMatchingStructs.arg_count(), 6);
        assert_eq!(OpCode::LockGuards.arg_count(), 0);
        assert!(OpCode::LockGuards.has_varargs());
        assert_eq!(OpCode::TryLockGuards.arg_count(), 1);
        assert!(OpCode::TryLockGuards.has_varargs());
        assert_eq!(OpCode::CmpEq.name(), "cmp_eq");
    }

    /// Full-table pin for #2127/SCR-D1-NEW2-01 — the spot-check above only
    /// covered 7 of 51 rows, so a typo'd arg-count digit or a reordered row
    /// elsewhere in `OPCODES` would pass `cargo test` silently. This checks
    /// every discriminant's `(name, arg_count, has_varargs)` against a
    /// literal expected table (independently transcribed from Champollion,
    /// not copy-pasted from `OPCODES` itself).
    #[test]
    fn metadata_matches_champollion_full_table() {
        const EXPECTED: [(&str, usize, bool); MAX_OPCODE as usize] = [
            ("nop", 0, false),
            ("iadd", 3, false),
            ("fadd", 3, false),
            ("isub", 3, false),
            ("fsub", 3, false),
            ("imul", 3, false),
            ("fmul", 3, false),
            ("idiv", 3, false),
            ("fdiv", 3, false),
            ("imod", 3, false),
            ("not", 2, false),
            ("ineg", 2, false),
            ("fneg", 2, false),
            ("assign", 2, false),
            ("cast", 2, false),
            ("cmp_eq", 3, false),
            ("cmp_lt", 3, false),
            ("cmp_lte", 3, false),
            ("cmp_gt", 3, false),
            ("cmp_gte", 3, false),
            ("jmp", 1, false),
            ("jmpt", 2, false),
            ("jmpf", 2, false),
            ("callmethod", 3, true),
            ("callparent", 2, true),
            ("callstatic", 3, true),
            ("return", 1, false),
            ("strcat", 3, false),
            ("propget", 3, false),
            ("propset", 3, false),
            ("array_create", 2, false),
            ("array_length", 2, false),
            ("array_getelement", 3, false),
            ("array_setelement", 3, false),
            ("array_findelement", 4, false),
            ("array_rfindelement", 4, false),
            ("is", 3, false),
            ("struct_create", 1, false),
            ("struct_get", 3, false),
            ("struct_set", 3, false),
            ("array_findstruct", 5, false),
            ("array_rfindstruct", 5, false),
            ("array_add", 3, false),
            ("array_insert", 3, false),
            ("array_removelast", 1, false),
            ("array_remove", 3, false),
            ("array_clear", 1, false),
            ("array_getallmatchingstructs", 6, false),
            ("lock_guards", 0, true),
            ("unlock_guards", 0, true),
            ("try_lock_guards", 1, true),
        ];
        for (byte, (name, arg_count, has_varargs)) in EXPECTED.into_iter().enumerate() {
            let op = OpCode::from_u8(byte as u8).unwrap();
            assert_eq!(op.name(), name, "opcode {byte} name mismatch");
            assert_eq!(op.arg_count(), arg_count, "opcode {byte} ({name}) arg_count mismatch");
            assert_eq!(
                op.has_varargs(),
                has_varargs,
                "opcode {byte} ({name}) has_varargs mismatch"
            );
        }
    }
}
