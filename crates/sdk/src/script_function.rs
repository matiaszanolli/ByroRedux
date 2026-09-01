//! Typed, bounded functions exposed by sandbox components to engine scripts.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::identity::{
    ComponentId, EntityRef, ExtensionId, FormRef, ScriptFunctionId, ScriptParameterId,
};

/// Maximum typed functions one extension package may publish.
pub const MAX_SCRIPT_FUNCTIONS: usize = 256;
/// Maximum ordered parameters accepted by one function.
pub const MAX_SCRIPT_FUNCTION_PARAMETERS: usize = 16;
/// Maximum UTF-8 bytes in a function description.
pub const MAX_SCRIPT_FUNCTION_DESCRIPTION_BYTES: usize = 256;
/// Maximum ASCII bytes in one Papyrus provider or function identifier.
pub const MAX_PAPYRUS_IDENTIFIER_BYTES: usize = 128;
/// Maximum UTF-8 bytes in one string argument or result.
pub const MAX_SCRIPT_STRING_BYTES: usize = 4 * 1024;
/// Maximum aggregate variable-width payload for one call.
pub const MAX_SCRIPT_CALL_BYTES: usize = 32 * 1024;

/// Scalar types that cross the engine-script/sandbox boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScriptValueType {
    Boolean,
    Integer,
    Float,
    String,
    Form,
    Entity,
}

/// One bounded callback value. `None` is valid only for optional parameters or
/// an absent result whose declaration permits no return value.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "type", content = "value")]
pub enum ScriptValue {
    None,
    Boolean(bool),
    Integer(i64),
    Float(f32),
    String(String),
    Form(FormRef),
    Entity(EntityRef),
}

impl ScriptValue {
    /// Return whether this value satisfies a declared type and nullability.
    pub fn matches(&self, value_type: ScriptValueType, optional: bool) -> bool {
        match self {
            Self::None => optional,
            Self::Boolean(_) => value_type == ScriptValueType::Boolean,
            Self::Integer(_) => value_type == ScriptValueType::Integer,
            Self::Float(value) => value_type == ScriptValueType::Float && value.is_finite(),
            Self::String(value) => {
                value_type == ScriptValueType::String && value.len() <= MAX_SCRIPT_STRING_BYTES
            }
            Self::Form(_) => value_type == ScriptValueType::Form,
            Self::Entity(_) => value_type == ScriptValueType::Entity,
        }
    }

    fn variable_bytes(&self) -> usize {
        match self {
            Self::String(value) => value.len(),
            _ => 0,
        }
    }
}

/// One named parameter in declaration order.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptParameterDeclaration {
    pub id: ScriptParameterId,
    pub value_type: ScriptValueType,
    #[serde(default)]
    pub optional: bool,
}

/// Declared return type. `optional` permits an explicit [`ScriptValue::None`]
/// result, which is required for nullable form and entity references.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptResultDeclaration {
    pub value_type: ScriptValueType,
    #[serde(default)]
    pub optional: bool,
}

/// A legal static-call spelling accepted by Papyrus source and PEX bytecode.
///
/// Extension and function IDs deliberately use a broader namespaced grammar,
/// so they cannot safely double as Papyrus identifiers. This explicit alias
/// lets a package publish `WeatherNative.GetAt(...)` while the host still
/// routes to the authenticated `ext.org.example.weather.weather-at` function.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PapyrusFunctionAlias {
    pub provider: String,
    pub function: String,
}

impl PapyrusFunctionAlias {
    /// Return the case-folded key used by Papyrus/PEX resolution.
    pub fn canonical_key(&self) -> (String, String) {
        (
            self.provider.to_ascii_lowercase(),
            self.function.to_ascii_lowercase(),
        )
    }

    /// Papyrus identifiers are ASCII, begin with a letter or underscore, and
    /// contain only letters, decimal digits, or underscores.
    pub fn is_valid(&self) -> bool {
        valid_papyrus_identifier(&self.provider) && valid_papyrus_identifier(&self.function)
    }
}

fn valid_papyrus_identifier(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_PAPYRUS_IDENTIFIER_BYTES {
        return false;
    }
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// One principal-namespaced typed function routed to a sandbox component.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptFunctionDeclaration {
    pub id: ScriptFunctionId,
    pub component: ComponentId,
    #[serde(default)]
    pub parameters: Vec<ScriptParameterDeclaration>,
    #[serde(default)]
    pub result: Option<ScriptResultDeclaration>,
    /// Optional engine-owned static-call spelling for Papyrus source/PEX.
    #[serde(default)]
    pub papyrus: Option<PapyrusFunctionAlias>,
    pub description: String,
}

impl ScriptFunctionDeclaration {
    /// Fully qualified spelling reserved to the declaring principal.
    pub fn qualified_name(&self, extension: &ExtensionId) -> String {
        format!("ext.{extension}.{}", self.id)
    }

    /// Validate declaration-local invariants independent of a manifest.
    pub fn validate(&self) -> Result<(), ScriptFunctionError> {
        if self.description.trim().is_empty()
            || self.description.chars().any(char::is_control)
            || self.description.len() > MAX_SCRIPT_FUNCTION_DESCRIPTION_BYTES
        {
            return Err(ScriptFunctionError::InvalidDescription);
        }
        if self.parameters.len() > MAX_SCRIPT_FUNCTION_PARAMETERS {
            return Err(ScriptFunctionError::TooManyParameters {
                actual: self.parameters.len(),
                maximum: MAX_SCRIPT_FUNCTION_PARAMETERS,
            });
        }
        if self.papyrus.as_ref().is_some_and(|alias| !alias.is_valid()) {
            return Err(ScriptFunctionError::InvalidPapyrusAlias);
        }
        let mut ids = BTreeSet::new();
        let mut optional_seen = false;
        for parameter in &self.parameters {
            if !ids.insert(parameter.id.clone()) {
                return Err(ScriptFunctionError::DuplicateParameter(
                    parameter.id.clone(),
                ));
            }
            optional_seen |= parameter.optional;
            if optional_seen && !parameter.optional {
                return Err(ScriptFunctionError::RequiredAfterOptional(
                    parameter.id.clone(),
                ));
            }
        }
        Ok(())
    }

    /// Validate one callback argument vector against this declaration.
    pub fn validate_arguments(&self, values: &[ScriptValue]) -> Result<(), ScriptFunctionError> {
        self.validate()?;
        let required = self
            .parameters
            .iter()
            .take_while(|parameter| !parameter.optional)
            .count();
        if !(required..=self.parameters.len()).contains(&values.len()) {
            return Err(ScriptFunctionError::InvalidArgumentCount {
                actual: values.len(),
                minimum: required,
                maximum: self.parameters.len(),
            });
        }
        let mut bytes = 0usize;
        for (index, value) in values.iter().enumerate() {
            let parameter = &self.parameters[index];
            if !value.matches(parameter.value_type, parameter.optional) {
                return Err(ScriptFunctionError::InvalidArgument {
                    index,
                    parameter: parameter.id.clone(),
                });
            }
            bytes = bytes.saturating_add(value.variable_bytes());
        }
        if bytes > MAX_SCRIPT_CALL_BYTES {
            return Err(ScriptFunctionError::PayloadTooLarge {
                actual: bytes,
                maximum: MAX_SCRIPT_CALL_BYTES,
            });
        }
        Ok(())
    }

    /// Validate the callback result against the declared return type.
    pub fn validate_result(&self, value: &ScriptValue) -> Result<(), ScriptFunctionError> {
        match self.result {
            None if value == &ScriptValue::None => Ok(()),
            Some(result) if value.matches(result.value_type, result.optional) => Ok(()),
            _ => Err(ScriptFunctionError::InvalidResult),
        }
    }
}

/// Contract violation detected before guest execution or result publication.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ScriptFunctionError {
    #[error("script function description is empty, unsafe, or too long")]
    InvalidDescription,
    #[error("script function has {actual} parameters; maximum is {maximum}")]
    TooManyParameters { actual: usize, maximum: usize },
    #[error("duplicate script function parameter {0}")]
    DuplicateParameter(ScriptParameterId),
    #[error("required parameter {0} follows an optional parameter")]
    RequiredAfterOptional(ScriptParameterId),
    #[error("Papyrus provider/function alias is not a legal bounded identifier")]
    InvalidPapyrusAlias,
    #[error("script function received {actual} arguments; expected {minimum}..={maximum}")]
    InvalidArgumentCount {
        actual: usize,
        minimum: usize,
        maximum: usize,
    },
    #[error("script function argument {index} for {parameter} has an invalid value or type")]
    InvalidArgument {
        index: usize,
        parameter: ScriptParameterId,
    },
    #[error("script function payload is {actual} bytes; maximum is {maximum}")]
    PayloadTooLarge { actual: usize, maximum: usize },
    #[error("script function returned a value that does not match its declaration")]
    InvalidResult,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration() -> ScriptFunctionDeclaration {
        ScriptFunctionDeclaration {
            id: ScriptFunctionId::new("lookup-nearby").unwrap(),
            component: ComponentId::new("runtime").unwrap(),
            parameters: vec![
                ScriptParameterDeclaration {
                    id: ScriptParameterId::new("radius").unwrap(),
                    value_type: ScriptValueType::Float,
                    optional: false,
                },
                ScriptParameterDeclaration {
                    id: ScriptParameterId::new("label").unwrap(),
                    value_type: ScriptValueType::String,
                    optional: true,
                },
            ],
            result: Some(ScriptResultDeclaration {
                value_type: ScriptValueType::Form,
                optional: true,
            }),
            papyrus: Some(PapyrusFunctionAlias {
                provider: "ByroSearch".to_owned(),
                function: "LookupNearby".to_owned(),
            }),
            description: "Find an authored reference near the caller".to_owned(),
        }
    }

    #[test]
    fn declaration_is_namespaced_typed_and_optional_suffix_only() {
        let declaration = declaration();
        declaration.validate().unwrap();
        assert_eq!(
            declaration.qualified_name(&ExtensionId::new("org.example.search").unwrap()),
            "ext.org.example.search.lookup-nearby"
        );
        assert_eq!(
            declaration.papyrus.as_ref().unwrap().canonical_key(),
            ("byrosearch".to_owned(), "lookupnearby".to_owned())
        );

        let mut invalid = declaration;
        invalid.parameters.swap(0, 1);
        assert!(matches!(
            invalid.validate(),
            Err(ScriptFunctionError::RequiredAfterOptional(_))
        ));
    }

    #[test]
    fn papyrus_aliases_use_the_cross_game_identifier_grammar() {
        let mut declaration = declaration();
        declaration.papyrus.as_mut().unwrap().provider = "9Invalid".to_owned();
        assert_eq!(
            declaration.validate(),
            Err(ScriptFunctionError::InvalidPapyrusAlias)
        );

        declaration.papyrus.as_mut().unwrap().provider = "Valid_9".to_owned();
        declaration.papyrus.as_mut().unwrap().function = "has-hyphen".to_owned();
        assert_eq!(
            declaration.validate(),
            Err(ScriptFunctionError::InvalidPapyrusAlias)
        );
    }

    #[test]
    fn arguments_and_results_enforce_types_finiteness_and_bounds() {
        let declaration = declaration();
        declaration
            .validate_arguments(&[ScriptValue::Float(12.5)])
            .unwrap();
        declaration
            .validate_arguments(&[
                ScriptValue::Float(12.5),
                ScriptValue::String("actor".to_owned()),
            ])
            .unwrap();
        assert!(declaration
            .validate_arguments(&[ScriptValue::Float(f32::NAN)])
            .is_err());
        assert!(declaration
            .validate_arguments(&[ScriptValue::Integer(12)])
            .is_err());
        assert!(declaration.validate_arguments(&[]).is_err());

        declaration
            .validate_result(&ScriptValue::Form(FormRef::new([7; 16], 42)))
            .unwrap();
        declaration.validate_result(&ScriptValue::None).unwrap();
        assert!(declaration
            .validate_result(&ScriptValue::String("wrong".to_owned()))
            .is_err());
    }
}
