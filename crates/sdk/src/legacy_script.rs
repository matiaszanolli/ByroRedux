//! Versioned wire encoding for engine-native calls embedded in legacy SCDA.
//!
//! Legacy ObScript bytecode identifies commands by process-local numeric
//! opcodes. That cannot address manifest-defined SDK functions safely, so
//! ByroRedux reserves one expression opcode whose payload carries the stable
//! principal-qualified function name and typed literal arguments.

use thiserror::Error;

use crate::script_function::{
    ScriptValue, MAX_SCRIPT_CALL_BYTES, MAX_SCRIPT_FUNCTION_PARAMETERS, MAX_SCRIPT_STRING_BYTES,
};

/// Reserved command opcode inside an `X`-prefixed SCDA expression.
pub const LEGACY_OBSCRIPT_SDK_CALL_OPCODE: u16 = 0xfffe;
/// Current payload version for [`encode_legacy_obscript_sdk_call`].
pub const LEGACY_OBSCRIPT_SDK_CALL_VERSION: u8 = 1;
/// Maximum encoded principal-qualified route length.
pub const MAX_LEGACY_OBSCRIPT_SDK_ROUTE_BYTES: usize = 512;

const MAGIC: &[u8; 8] = b"BYROSDK\0";
const NONE: u8 = 0;
const BOOLEAN: u8 = 1;
const INTEGER: u8 = 2;
const FLOAT: u8 = 3;
const STRING: u8 = 4;
const FORM: u8 = 5;

/// One decoded engine-native function call from legacy bytecode.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyObscriptSdkCall {
    pub qualified_name: String,
    pub arguments: Vec<ScriptValue>,
}

/// Rejection reason for malformed or unsafe legacy SDK-call payloads.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum LegacyObscriptSdkCallError {
    #[error("legacy SDK call route is not a canonical ext.* name")]
    InvalidQualifiedName,
    #[error("legacy SDK call has too many arguments")]
    TooManyArguments,
    #[error("legacy SDK call payload exceeds the bounded call budget")]
    PayloadTooLarge,
    #[error("legacy SDK call argument {index} cannot be encoded")]
    UnsupportedArgument { index: usize },
    #[error("legacy SDK call payload is truncated")]
    Truncated,
    #[error("legacy SDK call payload has invalid magic")]
    InvalidMagic,
    #[error("legacy SDK call payload version {0} is unsupported")]
    UnsupportedVersion(u8),
    #[error("legacy SDK call payload contains invalid UTF-8")]
    InvalidUtf8,
    #[error("legacy SDK call payload contains invalid value tag {0}")]
    InvalidValueTag(u8),
    #[error("legacy SDK call payload contains invalid scalar data")]
    InvalidScalar,
    #[error("legacy SDK call payload has trailing bytes")]
    TrailingBytes,
}

/// Encode the payload placed after the reserved SCDA expression opcode.
pub fn encode_legacy_obscript_sdk_call(
    qualified_name: &str,
    arguments: &[ScriptValue],
) -> Result<Vec<u8>, LegacyObscriptSdkCallError> {
    validate_route(qualified_name)?;
    if arguments.len() > MAX_SCRIPT_FUNCTION_PARAMETERS {
        return Err(LegacyObscriptSdkCallError::TooManyArguments);
    }
    let variable_bytes =
        arguments
            .iter()
            .enumerate()
            .try_fold(0usize, |bytes, (index, value)| {
                let added = match value {
                    ScriptValue::String(value) if value.len() <= MAX_SCRIPT_STRING_BYTES => {
                        value.len()
                    }
                    ScriptValue::String(_) | ScriptValue::Entity(_) => {
                        return Err(LegacyObscriptSdkCallError::UnsupportedArgument { index });
                    }
                    ScriptValue::Float(value) if !value.is_finite() => {
                        return Err(LegacyObscriptSdkCallError::InvalidScalar);
                    }
                    _ => 0,
                };
                Ok(bytes.saturating_add(added))
            })?;
    if variable_bytes > MAX_SCRIPT_CALL_BYTES {
        return Err(LegacyObscriptSdkCallError::PayloadTooLarge);
    }

    let mut payload = Vec::with_capacity(MAGIC.len() + qualified_name.len() + variable_bytes + 16);
    payload.extend_from_slice(MAGIC);
    payload.push(LEGACY_OBSCRIPT_SDK_CALL_VERSION);
    push_u16(&mut payload, qualified_name.len())?;
    payload.extend_from_slice(qualified_name.as_bytes());
    push_u16(&mut payload, arguments.len())?;
    for (index, argument) in arguments.iter().enumerate() {
        match argument {
            ScriptValue::None => payload.push(NONE),
            ScriptValue::Boolean(value) => {
                payload.push(BOOLEAN);
                payload.push(u8::from(*value));
            }
            ScriptValue::Integer(value) => {
                payload.push(INTEGER);
                payload.extend_from_slice(&value.to_le_bytes());
            }
            ScriptValue::Float(value) if value.is_finite() => {
                payload.push(FLOAT);
                payload.extend_from_slice(&value.to_bits().to_le_bytes());
            }
            ScriptValue::String(value) if value.len() <= MAX_SCRIPT_STRING_BYTES => {
                payload.push(STRING);
                push_u16(&mut payload, value.len())?;
                payload.extend_from_slice(value.as_bytes());
            }
            ScriptValue::Form(value) => {
                payload.push(FORM);
                payload.extend_from_slice(&value.source());
                payload.extend_from_slice(&value.local().to_le_bytes());
            }
            _ => return Err(LegacyObscriptSdkCallError::UnsupportedArgument { index }),
        }
    }
    if payload.len() > usize::from(u16::MAX) {
        return Err(LegacyObscriptSdkCallError::PayloadTooLarge);
    }
    Ok(payload)
}

/// Decode and fully validate one reserved SCDA SDK-call payload.
pub fn decode_legacy_obscript_sdk_call(
    payload: &[u8],
) -> Result<LegacyObscriptSdkCall, LegacyObscriptSdkCallError> {
    let mut cursor = 0usize;
    if take(payload, &mut cursor, MAGIC.len())? != MAGIC {
        return Err(LegacyObscriptSdkCallError::InvalidMagic);
    }
    let version = *take(payload, &mut cursor, 1)?
        .first()
        .ok_or(LegacyObscriptSdkCallError::Truncated)?;
    if version != LEGACY_OBSCRIPT_SDK_CALL_VERSION {
        return Err(LegacyObscriptSdkCallError::UnsupportedVersion(version));
    }
    let route_len = usize::from(read_u16(payload, &mut cursor)?);
    let qualified_name = std::str::from_utf8(take(payload, &mut cursor, route_len)?)
        .map_err(|_| LegacyObscriptSdkCallError::InvalidUtf8)?
        .to_owned();
    validate_route(&qualified_name)?;
    let argument_count = usize::from(read_u16(payload, &mut cursor)?);
    if argument_count > MAX_SCRIPT_FUNCTION_PARAMETERS {
        return Err(LegacyObscriptSdkCallError::TooManyArguments);
    }
    let mut arguments = Vec::with_capacity(argument_count);
    let mut variable_bytes = 0usize;
    for _ in 0..argument_count {
        let tag = *take(payload, &mut cursor, 1)?
            .first()
            .ok_or(LegacyObscriptSdkCallError::Truncated)?;
        let value = match tag {
            NONE => ScriptValue::None,
            BOOLEAN => match take(payload, &mut cursor, 1)?[0] {
                0 => ScriptValue::Boolean(false),
                1 => ScriptValue::Boolean(true),
                _ => return Err(LegacyObscriptSdkCallError::InvalidScalar),
            },
            INTEGER => {
                let bytes: [u8; 8] = take(payload, &mut cursor, 8)?
                    .try_into()
                    .map_err(|_| LegacyObscriptSdkCallError::Truncated)?;
                ScriptValue::Integer(i64::from_le_bytes(bytes))
            }
            FLOAT => {
                let bytes: [u8; 4] = take(payload, &mut cursor, 4)?
                    .try_into()
                    .map_err(|_| LegacyObscriptSdkCallError::Truncated)?;
                let value = f32::from_bits(u32::from_le_bytes(bytes));
                if !value.is_finite() {
                    return Err(LegacyObscriptSdkCallError::InvalidScalar);
                }
                ScriptValue::Float(value)
            }
            STRING => {
                let len = usize::from(read_u16(payload, &mut cursor)?);
                if len > MAX_SCRIPT_STRING_BYTES {
                    return Err(LegacyObscriptSdkCallError::PayloadTooLarge);
                }
                variable_bytes = variable_bytes.saturating_add(len);
                let value = std::str::from_utf8(take(payload, &mut cursor, len)?)
                    .map_err(|_| LegacyObscriptSdkCallError::InvalidUtf8)?;
                ScriptValue::String(value.to_owned())
            }
            FORM => {
                let source: [u8; 16] = take(payload, &mut cursor, 16)?
                    .try_into()
                    .map_err(|_| LegacyObscriptSdkCallError::Truncated)?;
                let local: [u8; 4] = take(payload, &mut cursor, 4)?
                    .try_into()
                    .map_err(|_| LegacyObscriptSdkCallError::Truncated)?;
                ScriptValue::Form(crate::identity::FormRef::new(
                    source,
                    u32::from_le_bytes(local),
                ))
            }
            other => return Err(LegacyObscriptSdkCallError::InvalidValueTag(other)),
        };
        arguments.push(value);
    }
    if cursor != payload.len() {
        return Err(LegacyObscriptSdkCallError::TrailingBytes);
    }
    if variable_bytes > MAX_SCRIPT_CALL_BYTES {
        return Err(LegacyObscriptSdkCallError::PayloadTooLarge);
    }
    Ok(LegacyObscriptSdkCall {
        qualified_name,
        arguments,
    })
}

fn validate_route(qualified_name: &str) -> Result<(), LegacyObscriptSdkCallError> {
    if qualified_name.len() > MAX_LEGACY_OBSCRIPT_SDK_ROUTE_BYTES
        || !qualified_name.starts_with("ext.")
        || qualified_name.split('.').any(|segment| {
            segment.is_empty()
                || segment.starts_with('-')
                || segment.ends_with('-')
                || !segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
    {
        return Err(LegacyObscriptSdkCallError::InvalidQualifiedName);
    }
    Ok(())
}

fn push_u16(output: &mut Vec<u8>, value: usize) -> Result<(), LegacyObscriptSdkCallError> {
    let value = u16::try_from(value).map_err(|_| LegacyObscriptSdkCallError::PayloadTooLarge)?;
    output.extend_from_slice(&value.to_le_bytes());
    Ok(())
}

fn read_u16(payload: &[u8], cursor: &mut usize) -> Result<u16, LegacyObscriptSdkCallError> {
    let bytes: [u8; 2] = take(payload, cursor, 2)?
        .try_into()
        .map_err(|_| LegacyObscriptSdkCallError::Truncated)?;
    Ok(u16::from_le_bytes(bytes))
}

fn take<'a>(
    payload: &'a [u8],
    cursor: &mut usize,
    len: usize,
) -> Result<&'a [u8], LegacyObscriptSdkCallError> {
    let end = cursor
        .checked_add(len)
        .ok_or(LegacyObscriptSdkCallError::Truncated)?;
    let bytes = payload
        .get(*cursor..end)
        .ok_or(LegacyObscriptSdkCallError::Truncated)?;
    *cursor = end;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_sdk_call_round_trips_every_supported_literal() {
        let call = LegacyObscriptSdkCall {
            qualified_name: "ext.org.example.math.answer".to_owned(),
            arguments: vec![
                ScriptValue::None,
                ScriptValue::Boolean(true),
                ScriptValue::Integer(-7),
                ScriptValue::Float(2.5),
                ScriptValue::String("hello world".to_owned()),
                ScriptValue::Form(crate::identity::FormRef::new([7; 16], 0x1234)),
            ],
        };
        let payload =
            encode_legacy_obscript_sdk_call(&call.qualified_name, &call.arguments).unwrap();
        assert_eq!(decode_legacy_obscript_sdk_call(&payload).unwrap(), call);
    }

    #[test]
    fn legacy_sdk_call_rejects_unsafe_identity_and_malformed_payloads() {
        assert_eq!(
            encode_legacy_obscript_sdk_call("Game.GetModCount", &[]),
            Err(LegacyObscriptSdkCallError::InvalidQualifiedName)
        );
        assert_eq!(
            encode_legacy_obscript_sdk_call(
                "ext.org.example.inspect.entity",
                &[ScriptValue::Entity(
                    crate::identity::EntityRef::new(1, 2).unwrap(),
                )],
            ),
            Err(LegacyObscriptSdkCallError::UnsupportedArgument { index: 0 })
        );

        let mut payload = encode_legacy_obscript_sdk_call("ext.org.example.math.answer", &[])
            .expect("fixture is valid");
        payload.push(0);
        assert_eq!(
            decode_legacy_obscript_sdk_call(&payload),
            Err(LegacyObscriptSdkCallError::TrailingBytes)
        );
    }
}
