//! Regression tests for the provider pipeline.
//!
//! Split by what each test needs (#3852): `lower` is pure and
//! table-testable, `execute` drives a live `World`. They previously shared
//! one 2447-line `#[cfg(test)]` boundary inside the 6158-line single file,
//! so every lowering test recompiled the interpreter.

mod execute;
mod lower;

use std::sync::Mutex;

use super::*;
use byroredux_papyrus::{ast::ScriptItem, parse_script};
use byroredux_sdk::{
    identity::{ComponentId, ScriptFunctionId, ScriptParameterId},
    script_function::{PapyrusFunctionAlias, ScriptParameterDeclaration},
};

fn declaration() -> ScriptFunctionDeclaration {
    ScriptFunctionDeclaration {
        id: ScriptFunctionId::new("weather-at").unwrap(),
        component: ComponentId::new("runtime").unwrap(),
        parameters: vec![
            ScriptParameterDeclaration {
                id: ScriptParameterId::new("day").unwrap(),
                value_type: ScriptValueType::Integer,
                optional: false,
            },
            ScriptParameterDeclaration {
                id: ScriptParameterId::new("fallback").unwrap(),
                value_type: ScriptValueType::String,
                optional: true,
            },
        ],
        result: Some(ScriptResultDeclaration {
            value_type: ScriptValueType::String,
            optional: false,
        }),
        papyrus: Some(PapyrusFunctionAlias {
            provider: "WeatherNative".to_owned(),
            function: "WeatherAt".to_owned(),
        }),
        description: "Return weather at a day index".to_owned(),
    }
}

fn boolean_declaration() -> ScriptFunctionDeclaration {
    ScriptFunctionDeclaration {
        id: ScriptFunctionId::new("is-storm").unwrap(),
        component: ComponentId::new("runtime").unwrap(),
        parameters: Vec::new(),
        result: Some(ScriptResultDeclaration {
            value_type: ScriptValueType::Boolean,
            optional: false,
        }),
        papyrus: Some(PapyrusFunctionAlias {
            provider: "WeatherNative".to_owned(),
            function: "IsStorm".to_owned(),
        }),
        description: "Whether the current weather is a storm".to_owned(),
    }
}

fn entity_declaration() -> ScriptFunctionDeclaration {
    ScriptFunctionDeclaration {
        id: ScriptFunctionId::new("inspect-entity").unwrap(),
        component: ComponentId::new("runtime").unwrap(),
        parameters: vec![ScriptParameterDeclaration {
            id: ScriptParameterId::new("target").unwrap(),
            value_type: ScriptValueType::Entity,
            optional: false,
        }],
        result: Some(ScriptResultDeclaration {
            value_type: ScriptValueType::String,
            optional: false,
        }),
        papyrus: Some(PapyrusFunctionAlias {
            provider: "WeatherNative".to_owned(),
            function: "InspectEntity".to_owned(),
        }),
        description: "Inspect one opaque entity handle".to_owned(),
    }
}

fn self_declaration() -> ScriptFunctionDeclaration {
    ScriptFunctionDeclaration {
        id: ScriptFunctionId::new("touch-self").unwrap(),
        component: ComponentId::new("runtime").unwrap(),
        parameters: vec![
            ScriptParameterDeclaration {
                id: ScriptParameterId::new("receiver").unwrap(),
                value_type: ScriptValueType::Entity,
                optional: false,
            },
            ScriptParameterDeclaration {
                id: ScriptParameterId::new("value").unwrap(),
                value_type: ScriptValueType::Integer,
                optional: false,
            },
        ],
        result: None,
        papyrus: Some(PapyrusFunctionAlias {
            provider: PAPYRUS_SELF_PROVIDER.to_owned(),
            function: "Touch".to_owned(),
        }),
        description: "Touch the current script owner".to_owned(),
    }
}

fn form_declaration() -> ScriptFunctionDeclaration {
    ScriptFunctionDeclaration {
        id: ScriptFunctionId::new("inspect-form").unwrap(),
        component: ComponentId::new("runtime").unwrap(),
        parameters: vec![ScriptParameterDeclaration {
            id: ScriptParameterId::new("form").unwrap(),
            value_type: ScriptValueType::Form,
            optional: false,
        }],
        result: Some(ScriptResultDeclaration {
            value_type: ScriptValueType::String,
            optional: false,
        }),
        papyrus: Some(PapyrusFunctionAlias {
            provider: "WeatherNative".to_owned(),
            function: "InspectForm".to_owned(),
        }),
        description: "Inspect one stable authored form".to_owned(),
    }
}

fn expression(source: &str) -> Expr {
    let source = format!("ScriptName Fixture\nEvent OnInit()\n  {source}\nEndEvent\n");
    let (script, errors) = parse_script(&source).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    let ScriptItem::Event(event) = &script.body[0].node else {
        panic!("expected event");
    };
    let byroredux_papyrus::ast::Stmt::ExprStmt(expression) = &event.body[0].node else {
        panic!("expected expression statement");
    };
    expression.node.clone()
}

fn catalog() -> PapyrusProviderCatalog {
    let mut catalog = PapyrusProviderCatalog::default();
    catalog
        .insert(
            &ExtensionId::new("org.example.weather").unwrap(),
            &declaration(),
        )
        .unwrap();
    catalog
        .insert(
            &ExtensionId::new("org.example.weather").unwrap(),
            &boolean_declaration(),
        )
        .unwrap();
    catalog
        .insert(
            &ExtensionId::new("org.example.weather").unwrap(),
            &entity_declaration(),
        )
        .unwrap();
    catalog
        .insert(
            &ExtensionId::new("org.example.weather").unwrap(),
            &form_declaration(),
        )
        .unwrap();
    catalog
}

fn self_catalog() -> PapyrusProviderCatalog {
    let mut catalog = catalog();
    catalog
        .insert(
            &ExtensionId::new("org.example.self").unwrap(),
            &self_declaration(),
        )
        .unwrap();
    catalog
}

fn object_declaration() -> ScriptFunctionDeclaration {
    ScriptFunctionDeclaration {
        id: ScriptFunctionId::new("touch-object").unwrap(),
        component: ComponentId::new("runtime").unwrap(),
        parameters: vec![
            ScriptParameterDeclaration {
                id: ScriptParameterId::new("receiver").unwrap(),
                value_type: ScriptValueType::Entity,
                optional: false,
            },
            ScriptParameterDeclaration {
                id: ScriptParameterId::new("value").unwrap(),
                value_type: ScriptValueType::Integer,
                optional: false,
            },
        ],
        result: None,
        papyrus: Some(PapyrusFunctionAlias {
            provider: "ObjectReference".to_owned(),
            function: "Touch".to_owned(),
        }),
        description: "Touch a typed object receiver".to_owned(),
    }
}

fn object_catalog() -> PapyrusProviderCatalog {
    let mut catalog = PapyrusProviderCatalog::engine_compatibility();
    catalog
        .insert(
            &ExtensionId::new("org.example.object").unwrap(),
            &object_declaration(),
        )
        .unwrap();
    catalog
}

fn provider_call_pex_bytes() -> Vec<u8> {
    use byroredux_pex::OpCode;

    let mut writer = PexBytes::new();
    for value in [
        "ProviderFixture",
        "ObjectReference",
        "",
        "None",
        "OnLoad",
        "WeatherNative",
        "WeatherAt",
        "::nonevar",
        "clear",
    ] {
        writer.intern(value);
    }

    // PEX magic is always little-endian; this marker selects Skyrim's
    // big-endian layout for every later multi-byte field.
    writer
        .bytes
        .extend_from_slice(&0xDEC0_57FA_u32.to_le_bytes());
    writer.u8(3);
    writer.u8(2);
    writer.u16(0);
    writer.i64(1_700_000_000);
    writer.string("ProviderFixture.psc");
    writer.string("byroredux");
    writer.string("provider conformance");

    let strings = writer.strings.clone();
    writer.u16(strings.len() as u16);
    for value in &strings {
        writer.string(value);
    }

    writer.u8(0); // no debug metadata
    writer.u16(0); // no user flags
    writer.u16(1); // one object
    writer.string_index("ProviderFixture");
    writer.u32(0); // ignored object size
    writer.string_index("ObjectReference");
    writer.string_index("");
    writer.u32(0);
    writer.string_index(""); // auto state
    writer.u16(0); // variables
    writer.u16(0); // properties
    writer.u16(1); // states
    writer.string_index("");
    writer.u16(1); // functions
    writer.string_index("OnLoad");
    writer.string_index("None");
    writer.string_index("");
    writer.u32(0);
    writer.u8(0);
    writer.u16(0); // parameters
    writer.u16(0); // locals
    writer.u16(2); // instructions

    writer.u8(OpCode::CallStatic as u8);
    for value in ["WeatherNative", "WeatherAt", "::nonevar"] {
        writer.u8(1); // identifier
        writer.string_index(value);
    }
    writer.u8(3); // integer vararg count
    writer.u32(2);
    writer.u8(3); // integer literal
    writer.u32(4);
    writer.u8(2); // string literal
    writer.string_index("clear");
    writer.u8(OpCode::Return as u8);
    writer.u8(0); // None

    writer.bytes
}

fn send_mod_event_pex_bytes() -> Vec<u8> {
    use byroredux_pex::OpCode;

    let mut writer = PexBytes::new();
    for value in [
        "SendFixture",
        "ObjectReference",
        "",
        "None",
        "OnLoad",
        "SendModEvent",
        "self",
        "::nonevar",
        "ByroReady",
    ] {
        writer.intern(value);
    }

    writer
        .bytes
        .extend_from_slice(&0xDEC0_57FA_u32.to_le_bytes());
    writer.u8(3);
    writer.u8(2);
    writer.u16(0);
    writer.i64(1_700_000_000);
    writer.string("SendFixture.psc");
    writer.string("byroredux");
    writer.string("instance ModEvent conformance");

    let strings = writer.strings.clone();
    writer.u16(strings.len() as u16);
    for value in &strings {
        writer.string(value);
    }

    writer.u8(0);
    writer.u16(0);
    writer.u16(1);
    writer.string_index("SendFixture");
    writer.u32(0);
    writer.string_index("ObjectReference");
    writer.string_index("");
    writer.u32(0);
    writer.string_index("");
    writer.u16(0);
    writer.u16(0);
    writer.u16(1);
    writer.string_index("");
    writer.u16(1);
    writer.string_index("OnLoad");
    writer.string_index("None");
    writer.string_index("");
    writer.u32(0);
    writer.u8(0);
    writer.u16(0);
    writer.u16(0);
    writer.u16(2);

    writer.u8(OpCode::CallMethod as u8);
    for value in ["SendModEvent", "self", "::nonevar"] {
        writer.u8(1);
        writer.string_index(value);
    }
    writer.u8(3);
    writer.u32(1);
    writer.u8(2);
    writer.string_index("ByroReady");
    writer.u8(OpCode::Return as u8);
    writer.u8(0);

    writer.bytes
}

struct PexBytes {
    bytes: Vec<u8>,
    strings: Vec<String>,
}

impl PexBytes {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            strings: Vec::new(),
        }
    }

    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn i64(&mut self, value: i64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn string(&mut self, value: &str) {
        self.u16(value.len() as u16);
        self.bytes.extend_from_slice(value.as_bytes());
    }

    fn intern(&mut self, value: &str) {
        if !self.strings.iter().any(|candidate| candidate == value) {
            self.strings.push(value.to_owned());
        }
    }

    fn string_index(&mut self, value: &str) {
        let index = self
            .strings
            .iter()
            .position(|candidate| candidate == value)
            .expect("PEX fixture string was pre-interned");
        self.u16(index as u16);
    }
}
