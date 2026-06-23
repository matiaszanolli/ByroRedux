//! Byte-level `.pex` reader — a structural port of Champollion's
//! `Pex::FileReader`. Mirrors its read order and endianness handling
//! exactly; the only behavioural change is resolving string indices to
//! owned `String`s as they're read (see [`crate::model`]).

use crate::model::*;
use crate::opcode::OpCode;
use crate::PexError;

/// Little-endian magic — FO4 / FO76 / Starfield "new format".
const LE_MAGIC: u32 = 0xFA57_C0DE;
/// Big-endian magic — original Skyrim "old format".
const BE_MAGIC: u32 = 0xDEC0_57FA;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Endian {
    Big,
    Little,
}

/// A cursor over the `.pex` byte buffer.
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    endian: Endian,
    /// Filled after the string table is read; every `string_index` read
    /// resolves against it.
    strings: Vec<String>,
}

type R<T> = Result<T, PexError>;

impl<'a> Reader<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            // Provisional; set for real once the magic is read.
            endian: Endian::Little,
            strings: Vec::new(),
        }
    }

    /// Decode the whole file (Champollion `FileReader::read(Binary&)`).
    pub(crate) fn read_binary(mut self) -> R<Pex> {
        let header = self.read_header()?;
        let script_type = match self.endian {
            Endian::Big => ScriptType::Skyrim,
            Endian::Little => match header.game_id {
                4 => ScriptType::Starfield,
                3 => ScriptType::Fallout76,
                _ => ScriptType::Fallout4,
            },
        };

        self.strings = self.read_string_table()?;
        let debug_info = self.read_debug_info()?;
        let user_flags = self.read_user_flags()?;
        let objects = self.read_objects(script_type)?;

        Ok(Pex {
            script_type,
            header,
            string_table: self.strings,
            debug_info,
            user_flags,
            objects,
        })
    }

    // ── primitives ──────────────────────────────────────────────────

    fn take(&mut self, n: usize) -> R<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.data.len())
            .ok_or(PexError::UnexpectedEof { offset: self.pos })?;
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> R<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> R<u16> {
        let b: [u8; 2] = self.take(2)?.try_into().unwrap();
        Ok(match self.endian {
            Endian::Big => u16::from_be_bytes(b),
            Endian::Little => u16::from_le_bytes(b),
        })
    }

    /// `le_override` reads little-endian regardless of `endian` — used only
    /// for the magic, before endianness is known.
    fn u32_opt(&mut self, le_override: bool) -> R<u32> {
        let b: [u8; 4] = self.take(4)?.try_into().unwrap();
        Ok(if !le_override && self.endian == Endian::Big {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    }

    fn u32(&mut self) -> R<u32> {
        self.u32_opt(false)
    }

    fn f32(&mut self) -> R<f32> {
        let b: [u8; 4] = self.take(4)?.try_into().unwrap();
        Ok(match self.endian {
            Endian::Big => f32::from_be_bytes(b),
            Endian::Little => f32::from_le_bytes(b),
        })
    }

    /// 64-bit `time_t`, verbatim (Champollion `getTime`).
    fn time(&mut self) -> R<i64> {
        let b: [u8; 8] = self.take(8)?.try_into().unwrap();
        Ok(match self.endian {
            Endian::Big => i64::from_be_bytes(b),
            Endian::Little => i64::from_le_bytes(b),
        })
    }

    /// A length-prefixed string (`u16` byte count + raw bytes). Papyrus
    /// strings are Windows-1252-ish; we keep them as lossy UTF-8 (vanilla
    /// identifiers are ASCII).
    fn string(&mut self) -> R<String> {
        let len = self.u16()? as usize;
        let bytes = self.take(len)?;
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }

    /// A `u16` index into the already-read string table, resolved to its
    /// string. Out-of-range indices are an error (Champollion throws).
    fn string_index(&mut self) -> R<String> {
        let idx = self.u16()? as usize;
        self.strings
            .get(idx)
            .cloned()
            .ok_or(PexError::BadStringIndex {
                index: idx,
                table_len: self.strings.len(),
            })
    }

    fn value(&mut self) -> R<Value> {
        let ty = self.u8()?;
        Ok(match ty {
            0 => Value::None,
            1 => Value::Identifier(self.string_index()?),
            2 => Value::Str(self.string_index()?),
            3 => Value::Integer(self.u32()? as i32),
            4 => Value::Float(self.f32()?),
            5 => Value::Bool(self.u8()? != 0),
            other => return Err(PexError::BadValueType { ty: other }),
        })
    }

    // ── structural reads (FileReader.cpp order) ─────────────────────

    fn read_header(&mut self) -> R<Header> {
        let magic = self.u32_opt(true)?;
        self.endian = if magic == LE_MAGIC {
            Endian::Little
        } else if magic == BE_MAGIC {
            Endian::Big
        } else {
            return Err(PexError::BadMagic { magic });
        };
        Ok(Header {
            major_version: self.u8()?,
            minor_version: self.u8()?,
            game_id: self.u16()?,
            compilation_time: self.time()?,
            source_file_name: self.string()?,
            user_name: self.string()?,
            computer_name: self.string()?,
        })
    }

    fn read_string_table(&mut self) -> R<Vec<String>> {
        let len = self.u16()? as usize;
        let mut table = Vec::with_capacity(len);
        for _ in 0..len {
            table.push(self.string()?);
        }
        Ok(table)
    }

    fn read_debug_info(&mut self) -> R<DebugInfo> {
        let present = self.u8()? != 0;
        if !present {
            return Ok(DebugInfo::default());
        }
        let modification_time = self.time()?;
        let function_count = self.u16()? as usize;
        let mut function_infos = Vec::with_capacity(function_count);
        for _ in 0..function_count {
            let object_name = self.string_index()?;
            let state_name = self.string_index()?;
            let function_name = self.string_index()?;
            let function_type = match self.u8()? {
                0 => Some(FunctionType::Method),
                1 => Some(FunctionType::Getter),
                2 => Some(FunctionType::Setter),
                _ => None,
            };
            let instr_count = self.u16()? as usize;
            let mut line_numbers = Vec::with_capacity(instr_count);
            for _ in 0..instr_count {
                line_numbers.push(self.u16()?);
            }
            function_infos.push(FunctionInfo {
                object_name,
                state_name,
                function_name,
                function_type,
                line_numbers,
            });
        }

        // Skyrim (big-endian) stops here; FO4+ adds property groups +
        // struct orders. Phase 1 consumes-and-discards them so the stream
        // stays aligned for the objects that follow.
        if self.endian != Endian::Big {
            self.skip_property_groups()?;
            self.skip_struct_orders()?;
        }

        Ok(DebugInfo {
            present,
            modification_time,
            function_infos,
        })
    }

    fn skip_property_groups(&mut self) -> R<()> {
        let group_count = self.u16()?;
        for _ in 0..group_count {
            let _object_name = self.string_index()?;
            let _group_name = self.string_index()?;
            let _doc = self.string_index()?;
            let _user_flags = self.u32()?;
            let name_count = self.u16()?;
            for _ in 0..name_count {
                let _name = self.string_index()?;
            }
        }
        Ok(())
    }

    fn skip_struct_orders(&mut self) -> R<()> {
        let order_count = self.u16()?;
        for _ in 0..order_count {
            let _object_name = self.string_index()?;
            let _order_name = self.string_index()?;
            let name_count = self.u16()?;
            for _ in 0..name_count {
                let _name = self.string_index()?;
            }
        }
        Ok(())
    }

    fn read_user_flags(&mut self) -> R<Vec<UserFlag>> {
        let count = self.u16()? as usize;
        let mut flags = Vec::with_capacity(count);
        for _ in 0..count {
            flags.push(UserFlag {
                name: self.string_index()?,
                flag_index: self.u8()?,
            });
        }
        Ok(flags)
    }

    fn read_objects(&mut self, script_type: ScriptType) -> R<Vec<Object>> {
        let count = self.u16()? as usize;
        let mut objects = Vec::with_capacity(count);
        for _ in 0..count {
            let name = self.string_index()?;
            let _size = self.u32()?; // object byte-size, ignored
            let parent_class_name = self.string_index()?;
            let doc_string = self.string_index()?;
            let const_flag = if script_type.is_skyrim() {
                0
            } else {
                self.u8()?
            };
            let user_flags = self.u32()?;
            let auto_state_name = self.string_index()?;
            let struct_infos = if script_type.is_skyrim() {
                Vec::new()
            } else {
                self.read_struct_infos()?
            };
            let variables = self.read_variables(script_type)?;
            let guards = if script_type == ScriptType::Starfield {
                self.read_guards()?
            } else {
                Vec::new()
            };
            let properties = self.read_properties()?;
            let states = self.read_states()?;
            objects.push(Object {
                name,
                parent_class_name,
                doc_string,
                const_flag,
                user_flags,
                auto_state_name,
                struct_infos,
                variables,
                guards,
                properties,
                states,
            });
        }
        Ok(objects)
    }

    fn read_struct_infos(&mut self) -> R<Vec<StructInfo>> {
        let count = self.u16()? as usize;
        let mut infos = Vec::with_capacity(count);
        for _ in 0..count {
            let name = self.string_index()?;
            let member_count = self.u16()? as usize;
            let mut members = Vec::with_capacity(member_count);
            for _ in 0..member_count {
                members.push(StructMember {
                    name: self.string_index()?,
                    type_name: self.string_index()?,
                    user_flags: self.u32()?,
                    value: self.value()?,
                    const_flag: self.u8()?,
                    doc_string: self.string_index()?,
                });
            }
            infos.push(StructInfo { name, members });
        }
        Ok(infos)
    }

    fn read_variables(&mut self, script_type: ScriptType) -> R<Vec<Variable>> {
        let count = self.u16()? as usize;
        let mut vars = Vec::with_capacity(count);
        for _ in 0..count {
            let name = self.string_index()?;
            let type_name = self.string_index()?;
            let user_flags = self.u32()?;
            let default_value = self.value()?;
            let const_flag = if script_type.is_skyrim() {
                0
            } else {
                self.u8()?
            };
            vars.push(Variable {
                name,
                type_name,
                user_flags,
                default_value,
                const_flag,
            });
        }
        Ok(vars)
    }

    fn read_guards(&mut self) -> R<Vec<Guard>> {
        let count = self.u16()? as usize;
        let mut guards = Vec::with_capacity(count);
        for _ in 0..count {
            guards.push(Guard {
                name: self.string_index()?,
            });
        }
        Ok(guards)
    }

    fn read_properties(&mut self) -> R<Vec<Property>> {
        let count = self.u16()? as usize;
        let mut props = Vec::with_capacity(count);
        for _ in 0..count {
            let mut prop = Property {
                name: self.string_index()?,
                type_name: self.string_index()?,
                doc_string: self.string_index()?,
                user_flags: self.u32()?,
                flags: self.u8()?,
                ..Property::default()
            };
            if prop.has_auto_var() {
                prop.auto_var_name = Some(self.string_index()?);
            } else {
                if prop.is_readable() {
                    prop.read_function = Some(self.read_function()?);
                }
                if prop.is_writable() {
                    prop.write_function = Some(self.read_function()?);
                }
            }
            props.push(prop);
        }
        Ok(props)
    }

    fn read_states(&mut self) -> R<Vec<State>> {
        let count = self.u16()? as usize;
        let mut states = Vec::with_capacity(count);
        for _ in 0..count {
            let name = self.string_index()?;
            let functions = self.read_named_functions()?;
            states.push(State { name, functions });
        }
        Ok(states)
    }

    /// A state's function list: each entry is `name + body`. (Property
    /// getter/setter bodies, by contrast, carry no name — see
    /// [`Self::read_function`].)
    fn read_named_functions(&mut self) -> R<Vec<Function>> {
        let count = self.u16()? as usize;
        let mut functions = Vec::with_capacity(count);
        for _ in 0..count {
            let name = self.string_index()?;
            let mut function = self.read_function()?;
            function.name = name;
            functions.push(function);
        }
        Ok(functions)
    }

    /// A bare function body (Champollion `read(Function&)`): everything
    /// after the (optional) name.
    fn read_function(&mut self) -> R<Function> {
        Ok(Function {
            name: String::new(),
            return_type_name: self.string_index()?,
            doc_string: self.string_index()?,
            user_flags: self.u32()?,
            flags: self.u8()?,
            params: self.read_typed_names()?,
            locals: self.read_typed_names()?,
            instructions: self.read_instructions()?,
        })
    }

    fn read_typed_names(&mut self) -> R<Vec<TypedName>> {
        let count = self.u16()? as usize;
        let mut names = Vec::with_capacity(count);
        for _ in 0..count {
            names.push(TypedName {
                name: self.string_index()?,
                type_name: self.string_index()?,
            });
        }
        Ok(names)
    }

    fn read_instructions(&mut self) -> R<Vec<Instruction>> {
        let count = self.u16()? as usize;
        let mut instructions = Vec::with_capacity(count);
        for _ in 0..count {
            let byte = self.u8()?;
            let op = OpCode::from_u8(byte).ok_or(PexError::BadOpcode { byte })?;
            let mut args = Vec::with_capacity(op.arg_count());
            for _ in 0..op.arg_count() {
                args.push(self.value()?);
            }
            let var_args = if op.has_varargs() {
                match self.value()? {
                    Value::Integer(n) if n >= 0 => {
                        // #1710 — `n` is attacker-controlled up to i32::MAX
                        // (~2.1B); `Value` carries a String (≥24 B), so a
                        // `with_capacity(n)` would request tens of GB and abort
                        // (OOM) before the per-element read can hit `take`'s EOF
                        // guard. Grow geometrically instead: each `value()` reads
                        // ≥1 byte, so the loop EOFs at the first read past the
                        // buffer and `v` never exceeds the bytes actually present.
                        let mut v = Vec::new();
                        for _ in 0..n {
                            v.push(self.value()?);
                        }
                        v
                    }
                    _ => return Err(PexError::BadVarArgCount),
                }
            } else {
                Vec::new()
            };
            instructions.push(Instruction { op, args, var_args });
        }
        Ok(instructions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1710 / SCR-D1-01 — a hostile var-arg count must decode to `Err`, not
    /// abort the process via an OOM `Vec::with_capacity`. `n` is attacker-
    /// controlled up to i32::MAX; pre-fix the reader pre-allocated `n` × ≥24 B
    /// (tens of GB) before the per-element read could hit the EOF guard.
    #[test]
    fn hostile_vararg_count_errors_instead_of_ooming() {
        // One `lock_guards` (opcode 48: 0 fixed args, has-varargs) whose
        // var-arg count is Integer(i32::MAX), with no element bytes following.
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(&1u16.to_le_bytes()); // instruction count = 1
        buf.push(OpCode::LockGuards as u8); // opcode 48
        buf.push(3); // Value tag 3 = Integer (the var-arg count)
        buf.extend_from_slice(&i32::MAX.to_le_bytes()); // count = 2_147_483_647
        // Deliberately no further bytes: the first element read must EOF.

        let result = Reader::new(&buf).read_instructions();
        assert!(
            matches!(result, Err(PexError::UnexpectedEof { .. })),
            "expected UnexpectedEof on the first absent element, got {result:?}"
        );
    }
}
