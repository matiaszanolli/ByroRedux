//! `script_functions` WIT host interface.

use crate::runtime::*;

impl script_functions::Host for HostState {
    fn argument_count(&mut self) -> wasmtime::Result<u32> {
        self.require_script_function_context()?;
        Ok(
            u32::try_from(self.current_script_arguments.as_ref().map_or(0, Vec::len))
                .expect("script function argument count is bounded below u32::MAX"),
        )
    }

    fn argument_type(
        &mut self,
        index: u32,
    ) -> wasmtime::Result<Option<script_functions::ValueType>> {
        self.require_script_function_context()?;
        Ok(self
            .current_script_arguments
            .as_ref()
            .and_then(|arguments| arguments.get(index as usize))
            .and_then(|value| match value {
                ScriptValue::None => None,
                ScriptValue::Boolean(_) => Some(script_functions::ValueType::Boolean),
                ScriptValue::Integer(_) => Some(script_functions::ValueType::Integer),
                ScriptValue::Float(_) => Some(script_functions::ValueType::FloatingPoint),
                ScriptValue::String(_) => Some(script_functions::ValueType::Text),
                ScriptValue::Form(_) => Some(script_functions::ValueType::Form),
                ScriptValue::Entity(_) => Some(script_functions::ValueType::Entity),
                ScriptValue::BooleanArray(_) => Some(script_functions::ValueType::BooleanArray),
                ScriptValue::IntegerArray(_) => Some(script_functions::ValueType::IntegerArray),
                ScriptValue::FloatArray(_) => Some(script_functions::ValueType::FloatArray),
                ScriptValue::StringArray(_) => Some(script_functions::ValueType::StringArray),
                ScriptValue::FormArray(_) => Some(script_functions::ValueType::FormArray),
                ScriptValue::EntityArray(_) => Some(script_functions::ValueType::EntityArray),
            }))
    }

    fn argument_boolean(&mut self, index: u32) -> wasmtime::Result<Option<bool>> {
        self.require_script_function_context()?;
        Ok(match self.script_argument(index) {
            Some(ScriptValue::Boolean(value)) => Some(*value),
            _ => None,
        })
    }

    fn argument_integer(&mut self, index: u32) -> wasmtime::Result<Option<i64>> {
        self.require_script_function_context()?;
        Ok(match self.script_argument(index) {
            Some(ScriptValue::Integer(value)) => Some(*value),
            _ => None,
        })
    }

    fn argument_float(&mut self, index: u32) -> wasmtime::Result<Option<f32>> {
        self.require_script_function_context()?;
        Ok(match self.script_argument(index) {
            Some(ScriptValue::Float(value)) => Some(*value),
            _ => None,
        })
    }

    fn argument_string(&mut self, index: u32) -> wasmtime::Result<Option<String>> {
        self.require_script_function_context()?;
        Ok(match self.script_argument(index) {
            Some(ScriptValue::String(value)) => Some(value.clone()),
            _ => None,
        })
    }

    fn argument_form(&mut self, index: u32) -> wasmtime::Result<Option<state::FormRef>> {
        self.require_script_function_context()?;
        Ok(match self.script_argument(index) {
            Some(ScriptValue::Form(value)) => Some(wit_form_ref(*value)),
            _ => None,
        })
    }

    fn argument_entity(&mut self, index: u32) -> wasmtime::Result<Option<state::EntityRef>> {
        self.require_script_function_context()?;
        Ok(match self.script_argument(index) {
            Some(ScriptValue::Entity(value)) => Some(state::EntityRef {
                world_generation: value.world_generation(),
                object: value.object(),
            }),
            _ => None,
        })
    }

    fn argument_boolean_array(&mut self, index: u32) -> wasmtime::Result<Option<Vec<bool>>> {
        self.require_script_function_context()?;
        Ok(match self.script_argument(index) {
            Some(ScriptValue::BooleanArray(values)) => Some(values.clone()),
            _ => None,
        })
    }

    fn argument_integer_array(&mut self, index: u32) -> wasmtime::Result<Option<Vec<i64>>> {
        self.require_script_function_context()?;
        Ok(match self.script_argument(index) {
            Some(ScriptValue::IntegerArray(values)) => Some(values.clone()),
            _ => None,
        })
    }

    fn argument_float_array(&mut self, index: u32) -> wasmtime::Result<Option<Vec<f32>>> {
        self.require_script_function_context()?;
        Ok(match self.script_argument(index) {
            Some(ScriptValue::FloatArray(values)) => Some(values.clone()),
            _ => None,
        })
    }

    fn argument_string_array(&mut self, index: u32) -> wasmtime::Result<Option<Vec<String>>> {
        self.require_script_function_context()?;
        Ok(match self.script_argument(index) {
            Some(ScriptValue::StringArray(values)) => Some(values.clone()),
            _ => None,
        })
    }

    fn argument_form_array(
        &mut self,
        index: u32,
    ) -> wasmtime::Result<Option<Vec<Option<state::FormRef>>>> {
        self.require_script_function_context()?;
        Ok(match self.script_argument(index) {
            Some(ScriptValue::FormArray(values)) => {
                Some(values.iter().map(|value| value.map(wit_form_ref)).collect())
            }
            _ => None,
        })
    }

    fn argument_entity_array(
        &mut self,
        index: u32,
    ) -> wasmtime::Result<Option<Vec<Option<state::EntityRef>>>> {
        self.require_script_function_context()?;
        Ok(match self.script_argument(index) {
            Some(ScriptValue::EntityArray(values)) => Some(
                values
                    .iter()
                    .map(|value| {
                        value.map(|value| state::EntityRef {
                            world_generation: value.world_generation(),
                            object: value.object(),
                        })
                    })
                    .collect(),
            ),
            _ => None,
        })
    }

    fn set_result_none(&mut self) -> wasmtime::Result<()> {
        self.set_script_result(ScriptValue::None)
    }

    fn set_result_boolean(&mut self, value: bool) -> wasmtime::Result<()> {
        self.set_script_result(ScriptValue::Boolean(value))
    }

    fn set_result_integer(&mut self, value: i64) -> wasmtime::Result<()> {
        self.set_script_result(ScriptValue::Integer(value))
    }

    fn set_result_float(&mut self, value: f32) -> wasmtime::Result<()> {
        self.set_script_result(ScriptValue::Float(value))
    }

    fn set_result_string(&mut self, value: String) -> wasmtime::Result<()> {
        if value.len() > MAX_SCRIPT_STRING_BYTES {
            wasmtime::bail!(
                "script function string result is {} bytes, exceeding {MAX_SCRIPT_STRING_BYTES}",
                value.len()
            );
        }
        self.set_script_result(ScriptValue::String(value))
    }

    fn set_result_form(&mut self, value: state::FormRef) -> wasmtime::Result<()> {
        self.set_script_result(ScriptValue::Form(sdk_form_ref(value)))
    }

    fn set_result_entity(&mut self, value: state::EntityRef) -> wasmtime::Result<()> {
        self.set_script_result(ScriptValue::Entity(sdk_entity_ref(value)?))
    }

    fn set_result_boolean_array(&mut self, value: Vec<bool>) -> wasmtime::Result<()> {
        self.set_script_result(ScriptValue::BooleanArray(value))
    }

    fn set_result_integer_array(&mut self, value: Vec<i64>) -> wasmtime::Result<()> {
        self.set_script_result(ScriptValue::IntegerArray(value))
    }

    fn set_result_float_array(&mut self, value: Vec<f32>) -> wasmtime::Result<()> {
        self.set_script_result(ScriptValue::FloatArray(value))
    }

    fn set_result_string_array(&mut self, value: Vec<String>) -> wasmtime::Result<()> {
        self.set_script_result(ScriptValue::StringArray(value))
    }

    fn set_result_form_array(
        &mut self,
        value: Vec<Option<state::FormRef>>,
    ) -> wasmtime::Result<()> {
        self.set_script_result(ScriptValue::FormArray(
            value
                .into_iter()
                .map(|value| value.map(sdk_form_ref))
                .collect(),
        ))
    }

    fn set_result_entity_array(
        &mut self,
        value: Vec<Option<state::EntityRef>>,
    ) -> wasmtime::Result<()> {
        self.set_script_result(ScriptValue::EntityArray(
            value
                .into_iter()
                .map(|value| value.map(sdk_entity_ref).transpose())
                .collect::<std::result::Result<Vec<_>, _>>()?,
        ))
    }
}
