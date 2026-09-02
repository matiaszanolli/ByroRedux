//! Bounded structural decoding for legacy compiled `SCDA` scripts.
//!
//! This is deliberately not a full ObScript decompiler. It walks the framed
//! statement stream and ordinary `Set`/`If`/`ElseIf` expressions, recovering
//! only extender opcodes pinned by the upstream command tables.
//!
//! Framing and expression tags follow xNVSE's `ScriptAnalyzer`; command IDs
//! are derived in registration order from the official xNVSE and xOBSE
//! `CommandTable.cpp` sources. Keep the dialect tables separate: the same
//! numeric opcode can name unrelated FOSE/xNVSE/OBSE commands.

use byroredux_sdk::compatibility::LegacyObscriptLoadOrderCall;

const SET_TO: u16 = 0x15;
const IF: u16 = 0x16;
const ELSE_IF: u16 = 0x18;
const REFERENCE_FUNCTION: u16 = 0x1c;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObscriptDialect {
    Xnvse,
    Obse,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObscriptCall {
    pub command: &'static str,
    pub byte_offset: usize,
    pub arguments: Vec<ObscriptArgument>,
}

/// Literal argument recovered from the default ObScript command encoding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObscriptArgument {
    String(String),
    Integer(i32),
    FloatBits(u64),
    /// A runtime variable/reference expression that cannot be resolved by the
    /// source-less structural decoder alone.
    Dynamic,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObscriptLoadOrderCallError {
    InvalidArguments {
        command: &'static str,
        expected: &'static str,
    },
    NumericOutOfRange,
}

impl std::fmt::Display for ObscriptLoadOrderCallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidArguments { command, expected } => {
                if *expected == "no arguments" {
                    write!(formatter, "{command} does not accept arguments")
                } else {
                    write!(
                        formatter,
                        "{command} requires exactly one {expected} literal argument"
                    )
                }
            }
            Self::NumericOutOfRange => formatter
                .write_str("GetNthModName numeric argument is outside the ObScript i32 range"),
        }
    }
}

impl std::error::Error for ObscriptLoadOrderCallError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObscriptDecodeDiagnostic {
    pub byte_offset: usize,
    pub message: &'static str,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ObscriptDecode {
    pub calls: Vec<ObscriptCall>,
    pub diagnostics: Vec<ObscriptDecodeDiagnostic>,
}

/// Recover supported extender calls from one compiled SCDA blob.
pub fn decode_extender_calls(bytes: &[u8], dialect: ObscriptDialect) -> ObscriptDecode {
    let mut decoded = ObscriptDecode::default();
    let mut offset = 0usize;
    while offset < bytes.len() {
        let Some(opcode) = read_u16(bytes, offset) else {
            malformed(&mut decoded, offset, "truncated SCDA statement opcode");
            break;
        };
        let (actual_opcode, length_offset, payload_offset, command_offset) =
            if opcode == REFERENCE_FUNCTION {
                let Some(actual_opcode) = read_u16(bytes, offset + 4) else {
                    malformed(
                        &mut decoded,
                        offset,
                        "truncated SCDA reference-function header",
                    );
                    break;
                };
                (actual_opcode, offset + 6, offset + 8, offset + 4)
            } else {
                (opcode, offset + 2, offset + 4, offset)
            };
        let Some(payload_len) = read_u16(bytes, length_offset).map(usize::from) else {
            malformed(&mut decoded, offset, "truncated SCDA statement length");
            break;
        };
        let Some(payload_end) = payload_offset.checked_add(payload_len) else {
            malformed(&mut decoded, offset, "overflowing SCDA statement length");
            break;
        };
        if payload_end > bytes.len() {
            malformed(
                &mut decoded,
                offset,
                "SCDA statement payload exceeds compiled bytecode",
            );
            break;
        }

        let payload = &bytes[payload_offset..payload_end];
        push_known_call(
            &mut decoded,
            dialect,
            actual_opcode,
            command_offset,
            payload,
        );
        match actual_opcode {
            IF | ELSE_IF => scan_expression(payload, 2, payload_offset, dialect, &mut decoded),
            SET_TO => scan_set_to(payload, payload_offset, dialect, &mut decoded),
            _ => {}
        }
        offset = payload_end;
    }
    decoded
}

/// Decode one complete `X` expression token into a supported engine-native
/// load-order call. Used by the conservative compiled-handler lowerer: any
/// trailing operand/operator or unknown/version-probe command is a miss.
pub(crate) fn decode_exact_load_order_expression(
    expression: &[u8],
    dialect: ObscriptDialect,
) -> Option<LegacyObscriptLoadOrderCall> {
    if expression.first() != Some(&b'X') {
        return None;
    }
    let opcode = read_u16(expression, 1)?;
    let argument_len = usize::from(read_u16(expression, 3)?);
    let end = 5usize.checked_add(argument_len)?;
    if end != expression.len() {
        return None;
    }
    let mut decoded = ObscriptDecode::default();
    push_known_call(&mut decoded, dialect, opcode, 1, &expression[5..end]);
    if !decoded.diagnostics.is_empty() || decoded.calls.len() != 1 {
        return None;
    }
    legacy_load_order_call(&decoded.calls[0]).ok().flatten()
}

fn scan_set_to(
    payload: &[u8],
    base_offset: usize,
    dialect: ObscriptDialect,
    decoded: &mut ObscriptDecode,
) {
    let target_len = match payload.first().copied() {
        Some(b'r') => 6,
        Some(b's' | b'f' | b'G') => 3,
        Some(_) => return,
        None => {
            malformed(decoded, base_offset, "truncated SCDA set target");
            return;
        }
    };
    scan_expression(payload, target_len, base_offset, dialect, decoded);
}

fn scan_expression(
    payload: &[u8],
    length_offset: usize,
    base_offset: usize,
    dialect: ObscriptDialect,
    decoded: &mut ObscriptDecode,
) {
    let Some(expression_len) = read_u16(payload, length_offset).map(usize::from) else {
        malformed(
            decoded,
            base_offset + length_offset,
            "truncated SCDA expression length",
        );
        return;
    };
    let expression_start = length_offset + 2;
    let Some(expression_end) = expression_start.checked_add(expression_len) else {
        malformed(
            decoded,
            base_offset + length_offset,
            "overflowing SCDA expression length",
        );
        return;
    };
    if expression_end > payload.len() {
        malformed(
            decoded,
            base_offset + length_offset,
            "SCDA expression exceeds statement payload",
        );
        return;
    }

    let expression = &payload[expression_start..expression_end];
    let expression_base = base_offset + expression_start;
    let mut cursor = 0usize;
    while cursor < expression.len() {
        match expression[cursor] {
            b's' | b'l' | b'f' | b'G' | b'Z' | b'r' => {
                if cursor + 3 > expression.len() {
                    malformed(
                        decoded,
                        expression_base + cursor,
                        "truncated SCDA expression variable",
                    );
                    return;
                }
                cursor += 3;
            }
            b'"' => {
                let Some(string_len) = read_u16(expression, cursor + 1).map(usize::from) else {
                    malformed(
                        decoded,
                        expression_base + cursor,
                        "truncated SCDA expression string",
                    );
                    return;
                };
                let Some(next) = cursor
                    .checked_add(3)
                    .and_then(|v| v.checked_add(string_len))
                else {
                    malformed(
                        decoded,
                        expression_base + cursor,
                        "overflowing SCDA expression string",
                    );
                    return;
                };
                if next > expression.len() {
                    malformed(
                        decoded,
                        expression_base + cursor,
                        "SCDA expression string exceeds expression payload",
                    );
                    return;
                }
                cursor = next;
            }
            b'X' => {
                let Some(opcode) = read_u16(expression, cursor + 1) else {
                    malformed(
                        decoded,
                        expression_base + cursor,
                        "truncated SCDA expression command opcode",
                    );
                    return;
                };
                let Some(data_len) = read_u16(expression, cursor + 3).map(usize::from) else {
                    malformed(
                        decoded,
                        expression_base + cursor,
                        "truncated SCDA expression command length",
                    );
                    return;
                };
                let Some(next) = cursor.checked_add(5).and_then(|v| v.checked_add(data_len)) else {
                    malformed(
                        decoded,
                        expression_base + cursor,
                        "overflowing SCDA expression command length",
                    );
                    return;
                };
                if next > expression.len() {
                    malformed(
                        decoded,
                        expression_base + cursor,
                        "SCDA expression command exceeds expression payload",
                    );
                    return;
                }
                push_known_call(
                    decoded,
                    dialect,
                    opcode,
                    expression_base + cursor + 1,
                    &expression[cursor + 5..next],
                );
                cursor = next;
            }
            _ => cursor += 1,
        }
    }
}

fn push_known_call(
    decoded: &mut ObscriptDecode,
    dialect: ObscriptDialect,
    opcode: u16,
    byte_offset: usize,
    payload: &[u8],
) {
    let command = match (dialect, opcode) {
        (ObscriptDialect::Xnvse, 0x1400) => "GetNVSEVersion",
        (ObscriptDialect::Xnvse, 0x1401) => "GetNVSERevision",
        (ObscriptDialect::Xnvse, 0x1402) => "GetNVSEBeta",
        (ObscriptDialect::Xnvse, 0x14ae) => "IsModLoaded",
        (ObscriptDialect::Xnvse, 0x14af) => "GetModIndex",
        (ObscriptDialect::Xnvse, 0x14b0) => "GetNumLoadedMods",
        (ObscriptDialect::Xnvse, 0x1586) => "GetNthModName",
        (ObscriptDialect::Obse, 0x1438) => "GetOBSEVersion",
        (ObscriptDialect::Obse, 0x1651) => "IsModLoaded",
        (ObscriptDialect::Obse, 0x165e) => "GetModIndex",
        (ObscriptDialect::Obse, 0x16b7) => "GetOBSERevision",
        (ObscriptDialect::Obse, 0x171a) => "GetNthModName",
        _ => return,
    };
    let arguments = decode_known_arguments(command, payload, byte_offset, decoded);
    decoded.calls.push(ObscriptCall {
        command,
        byte_offset,
        arguments,
    });
}

fn decode_known_arguments(
    command: &'static str,
    payload: &[u8],
    byte_offset: usize,
    decoded: &mut ObscriptDecode,
) -> Vec<ObscriptArgument> {
    let expected = usize::from(matches!(
        command,
        "IsModLoaded" | "GetModIndex" | "GetNthModName"
    ));
    if payload.is_empty() && expected == 0 {
        return Vec::new();
    }
    let Some(count) = read_u16(payload, 0).map(usize::from) else {
        malformed(
            decoded,
            byte_offset,
            "truncated SCDA command argument count",
        );
        return Vec::new();
    };
    if count != expected {
        malformed(
            decoded,
            byte_offset,
            "unexpected SCDA command argument count",
        );
        return Vec::new();
    }
    if expected == 0 {
        return Vec::new();
    }

    if matches!(command, "IsModLoaded" | "GetModIndex") {
        let Some(length) = read_u16(payload, 2).map(usize::from) else {
            malformed(
                decoded,
                byte_offset,
                "truncated SCDA string argument length",
            );
            return Vec::new();
        };
        let Some(end) = 4usize.checked_add(length) else {
            malformed(
                decoded,
                byte_offset,
                "overflowing SCDA string argument length",
            );
            return Vec::new();
        };
        let Some(bytes) = payload.get(4..end) else {
            malformed(
                decoded,
                byte_offset,
                "SCDA string argument exceeds command payload",
            );
            return Vec::new();
        };
        let Ok(value) = std::str::from_utf8(bytes) else {
            malformed(decoded, byte_offset, "SCDA string argument is not UTF-8");
            return Vec::new();
        };
        return vec![ObscriptArgument::String(value.to_owned())];
    }

    match payload.get(2).copied() {
        Some(b'n') => {
            let Some(bytes) = payload.get(3..7).and_then(|bytes| bytes.try_into().ok()) else {
                malformed(decoded, byte_offset, "truncated SCDA integer literal");
                return Vec::new();
            };
            vec![ObscriptArgument::Integer(i32::from_le_bytes(bytes))]
        }
        Some(b'z') => {
            let Some(bytes) = payload.get(3..11).and_then(|bytes| bytes.try_into().ok()) else {
                malformed(
                    decoded,
                    byte_offset,
                    "truncated SCDA floating-point literal",
                );
                return Vec::new();
            };
            vec![ObscriptArgument::FloatBits(u64::from_le_bytes(bytes))]
        }
        Some(_) => vec![ObscriptArgument::Dynamic],
        None => {
            malformed(decoded, byte_offset, "missing SCDA numeric argument");
            Vec::new()
        }
    }
}

/// Convert a decoded literal load-order probe into the SDK's executable
/// engine-semantic call. Version probes intentionally return `Ok(None)`:
/// callers must use SDK/service feature discovery instead of fake versions.
pub fn legacy_load_order_call(
    call: &ObscriptCall,
) -> Result<Option<LegacyObscriptLoadOrderCall>, ObscriptLoadOrderCallError> {
    let string_argument = || match call.arguments.as_slice() {
        [ObscriptArgument::String(value)] => Ok(value.clone()),
        _ => Err(ObscriptLoadOrderCallError::InvalidArguments {
            command: call.command,
            expected: "string",
        }),
    };
    let no_arguments = || {
        if call.arguments.is_empty() {
            Ok(())
        } else {
            Err(ObscriptLoadOrderCallError::InvalidArguments {
                command: call.command,
                expected: "no arguments",
            })
        }
    };
    let result = match call.command {
        "IsModLoaded" => LegacyObscriptLoadOrderCall::IsModLoaded {
            plugin: string_argument()?,
        },
        "GetModIndex" => LegacyObscriptLoadOrderCall::GetModIndex {
            plugin: string_argument()?,
        },
        "GetNumLoadedMods" => {
            no_arguments()?;
            LegacyObscriptLoadOrderCall::GetNumLoadedMods
        }
        "GetNumLoadedPlugins" => {
            no_arguments()?;
            LegacyObscriptLoadOrderCall::GetNumLoadedPlugins
        }
        "GetNthModName" => {
            let index = match call.arguments.as_slice() {
                [ObscriptArgument::Integer(value)] => *value,
                [ObscriptArgument::FloatBits(bits)] => {
                    let value = f64::from_bits(*bits);
                    if !value.is_finite()
                        || value.fract() != 0.0
                        || value < f64::from(i32::MIN)
                        || value > f64::from(i32::MAX)
                    {
                        return Err(ObscriptLoadOrderCallError::NumericOutOfRange);
                    }
                    value as i32
                }
                _ => {
                    return Err(ObscriptLoadOrderCallError::InvalidArguments {
                        command: call.command,
                        expected: "numeric",
                    });
                }
            };
            LegacyObscriptLoadOrderCall::GetNthModName { index }
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    let pair: [u8; 2] = bytes.get(offset..end)?.try_into().ok()?;
    Some(u16::from_le_bytes(pair))
}

fn malformed(decoded: &mut ObscriptDecode, byte_offset: usize, message: &'static str) {
    decoded.diagnostics.push(ObscriptDecodeDiagnostic {
        byte_offset,
        message,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_sdk::compatibility::{
        adapt_legacy_obscript_load_order, LegacyObscriptLoadOrderCall,
        LegacyObscriptLoadOrderResult,
    };
    use byroredux_sdk::content::{ContentCatalog, PluginInfo, PluginKind};

    fn conditional(expression: &[u8]) -> Vec<u8> {
        let payload_len = 4 + expression.len();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&IF.to_le_bytes());
        bytes.extend_from_slice(&(payload_len as u16).to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&(expression.len() as u16).to_le_bytes());
        bytes.extend_from_slice(expression);
        bytes
    }

    fn expression_call(opcode: u16) -> Vec<u8> {
        expression_call_with_args(opcode, &[])
    }

    fn expression_call_with_args(opcode: u16, arguments: &[u8]) -> Vec<u8> {
        let mut bytes = vec![b'X'];
        bytes.extend_from_slice(&opcode.to_le_bytes());
        bytes.extend_from_slice(&(arguments.len() as u16).to_le_bytes());
        bytes.extend_from_slice(arguments);
        bytes
    }

    fn string_arguments(value: &str) -> Vec<u8> {
        let mut bytes = 1u16.to_le_bytes().to_vec();
        bytes.extend_from_slice(&(value.len() as u16).to_le_bytes());
        bytes.extend_from_slice(value.as_bytes());
        bytes
    }

    fn integer_arguments(value: i32) -> Vec<u8> {
        let mut bytes = 1u16.to_le_bytes().to_vec();
        bytes.push(b'n');
        bytes.extend_from_slice(&value.to_le_bytes());
        bytes
    }

    #[test]
    fn decodes_xnvse_and_obse_calls_inside_framed_conditions() {
        let nvse = decode_extender_calls(
            &conditional(&expression_call(0x1400)),
            ObscriptDialect::Xnvse,
        );
        assert_eq!(nvse.calls[0].command, "GetNVSEVersion");
        assert!(nvse.diagnostics.is_empty());

        let obse = decode_extender_calls(
            &conditional(&expression_call(0x16b7)),
            ObscriptDialect::Obse,
        );
        assert_eq!(obse.calls[0].command, "GetOBSERevision");
        assert!(obse.diagnostics.is_empty());
    }

    #[test]
    fn skips_opcode_shaped_bytes_inside_expression_strings() {
        let mut expression = vec![b'"'];
        expression.extend_from_slice(&3u16.to_le_bytes());
        expression.extend_from_slice(&[b'X', 0x00, 0x14]);
        let decoded = decode_extender_calls(&conditional(&expression), ObscriptDialect::Xnvse);
        assert!(decoded.calls.is_empty());
        assert!(decoded.diagnostics.is_empty());
    }

    #[test]
    fn decodes_reference_function_header_and_rejects_truncation() {
        let mut referenced = REFERENCE_FUNCTION.to_le_bytes().to_vec();
        referenced.extend_from_slice(&1u16.to_le_bytes());
        referenced.extend_from_slice(&0x14afu16.to_le_bytes());
        referenced.extend_from_slice(&0u16.to_le_bytes());
        let decoded = decode_extender_calls(&referenced, ObscriptDialect::Xnvse);
        assert_eq!(decoded.calls[0].command, "GetModIndex");

        let malformed = decode_extender_calls(&[IF as u8, 0, 9, 0], ObscriptDialect::Xnvse);
        assert!(malformed.calls.is_empty());
        assert_eq!(malformed.diagnostics.len(), 1);
    }

    #[test]
    fn decodes_string_and_numeric_literals_without_interpreting_variables() {
        let plugin = string_arguments("Companion.esp");
        let decoded = decode_extender_calls(
            &conditional(&expression_call_with_args(0x14af, &plugin)),
            ObscriptDialect::Xnvse,
        );
        assert_eq!(
            decoded.calls[0].arguments,
            vec![ObscriptArgument::String("Companion.esp".to_owned())]
        );
        assert!(decoded.diagnostics.is_empty());

        let index = integer_arguments(7);
        let decoded = decode_extender_calls(
            &conditional(&expression_call_with_args(0x1586, &index)),
            ObscriptDialect::Xnvse,
        );
        assert_eq!(
            decoded.calls[0].arguments,
            vec![ObscriptArgument::Integer(7)]
        );

        let dynamic = [1, 0, b's', 0, 0];
        let decoded = decode_extender_calls(
            &conditional(&expression_call_with_args(0x1586, &dynamic)),
            ObscriptDialect::Xnvse,
        );
        assert_eq!(decoded.calls[0].arguments, vec![ObscriptArgument::Dynamic]);
        assert!(legacy_load_order_call(&decoded.calls[0]).is_err());
    }

    #[test]
    fn compiled_get_mod_index_executes_against_engine_content_catalog() {
        let arguments = string_arguments("Companion.esp");
        let compiled = conditional(&expression_call_with_args(0x14af, &arguments));
        let decoded = decode_extender_calls(&compiled, ObscriptDialect::Xnvse);
        let call = legacy_load_order_call(&decoded.calls[0])
            .unwrap()
            .expect("load-order command");
        let catalog = ContentCatalog::new(vec![
            PluginInfo::new("FalloutNV.esm", [1; 16], PluginKind::Regular).unwrap(),
            PluginInfo::new("Companion.esp", [2; 16], PluginKind::Regular).unwrap(),
        ])
        .unwrap();

        assert_eq!(
            adapt_legacy_obscript_load_order(&catalog, call),
            Ok(LegacyObscriptLoadOrderResult::Integer(1))
        );
    }

    #[test]
    fn compiled_get_num_loaded_plugins_preserves_catalog_count() {
        let call = ObscriptCall {
            command: "GetNumLoadedPlugins",
            byte_offset: 0,
            arguments: Vec::new(),
        };
        let call = legacy_load_order_call(&call)
            .unwrap()
            .expect("load-order command");
        assert_eq!(call, LegacyObscriptLoadOrderCall::GetNumLoadedPlugins);

        let catalog = ContentCatalog::new(vec![
            PluginInfo::new("FalloutNV.esm", [1; 16], PluginKind::Regular).unwrap(),
            PluginInfo::new("Companion.esp", [2; 16], PluginKind::Regular).unwrap(),
        ])
        .unwrap();
        assert_eq!(
            adapt_legacy_obscript_load_order(&catalog, call),
            Ok(LegacyObscriptLoadOrderResult::Integer(2))
        );

        let malformed = ObscriptCall {
            command: "GetNumLoadedPlugins",
            byte_offset: 0,
            arguments: vec![ObscriptArgument::Integer(1)],
        };
        assert_eq!(
            legacy_load_order_call(&malformed),
            Err(ObscriptLoadOrderCallError::InvalidArguments {
                command: "GetNumLoadedPlugins",
                expected: "no arguments",
            })
        );
    }
}
