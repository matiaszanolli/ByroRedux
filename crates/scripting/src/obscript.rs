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

const SET_TO: u16 = 0x15;
const IF: u16 = 0x16;
const ELSE_IF: u16 = 0x18;
const REFERENCE_FUNCTION: u16 = 0x1c;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObscriptDialect {
    Xnvse,
    Obse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObscriptCall {
    pub command: &'static str,
    pub byte_offset: usize,
}

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

        push_known_call(&mut decoded.calls, dialect, actual_opcode, command_offset);
        let payload = &bytes[payload_offset..payload_end];
        match actual_opcode {
            IF | ELSE_IF => scan_expression(payload, 2, payload_offset, dialect, &mut decoded),
            SET_TO => scan_set_to(payload, payload_offset, dialect, &mut decoded),
            _ => {}
        }
        offset = payload_end;
    }
    decoded
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
                push_known_call(
                    &mut decoded.calls,
                    dialect,
                    opcode,
                    expression_base + cursor + 1,
                );
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
                cursor = next;
            }
            _ => cursor += 1,
        }
    }
}

fn push_known_call(
    calls: &mut Vec<ObscriptCall>,
    dialect: ObscriptDialect,
    opcode: u16,
    byte_offset: usize,
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
    calls.push(ObscriptCall {
        command,
        byte_offset,
    });
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
        let mut bytes = vec![b'X'];
        bytes.extend_from_slice(&opcode.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
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
}
