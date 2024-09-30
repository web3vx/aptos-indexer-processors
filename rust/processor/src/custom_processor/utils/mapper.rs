use move_core_types::value::{MoveStructLayout, MoveTypeLayout, MoveValue};
use serde_json::{Number, Value};
use std::str::from_utf8;

pub fn map_string_to_move_type(type_string: &str) -> Option<MoveTypeLayout> {
    match type_string {
        "address" => Some(MoveTypeLayout::Address),
        "bool" => Some(MoveTypeLayout::Bool),
        "u8" => Some(MoveTypeLayout::U8),
        "u16" => Some(MoveTypeLayout::U16),
        "u32" => Some(MoveTypeLayout::U32),
        "u64" => Some(MoveTypeLayout::U64),
        "u128" => Some(MoveTypeLayout::U128),
        "u256" => Some(MoveTypeLayout::U256),
        "&signer" => Some(MoveTypeLayout::Signer),
        "0x1::object::Object" => Some(MoveTypeLayout::Address),
        s if s.starts_with("0x1::object::Object<") => Some(MoveTypeLayout::Address),
        "0x1::string::String" => Some(MoveTypeLayout::Struct(MoveStructLayout::Runtime(vec![
            MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8)),
        ]))),
        _ => parse_vector(type_string),
    }
}

fn parse_vector(type_string: &str) -> Option<MoveTypeLayout> {
    let mut replaced = type_string.to_string();
    if type_string.starts_with("0x1::option::Option") {
        replaced = type_string.replace("0x1::option::Option", "vector");
    }
    if let Some(stripped) = replaced
        .strip_prefix("vector<")
        .and_then(|s| s.strip_suffix(">"))
    {
        map_string_to_move_type(stripped).map(|layout| MoveTypeLayout::Vector(Box::new(layout)))
    } else {
        None
    }
}

pub fn parse_nested_move_values(input: &MoveValue) -> Value {
    match input {
        MoveValue::Vector(vec) => {
            if vec.is_empty() {
                return serde_json::Value::Array(vec![]);
            }
            let mut result: Vec<Value> = Vec::new();
            for value in vec {
                result.push(parse_nested_move_values(value));
            }

            Value::Array(result)
        },
        MoveValue::U8(num) => serde_json::Value::Number(Number::from(*num)),
        MoveValue::U16(num) => serde_json::Value::Number(Number::from(*num)),
        MoveValue::U32(num) => serde_json::Value::Number(Number::from(*num)),
        MoveValue::U64(num) => serde_json::Value::Number(Number::from(*num)),
        MoveValue::U128(num) => serde_json::Value::Number(Number::from(*num as u64)),
        MoveValue::U256(num) => serde_json::Value::Number(Number::from(num.unchecked_as_u64())),
        MoveValue::Bool(boolean) => serde_json::Value::Bool(*boolean),
        MoveValue::Address(address) => serde_json::Value::String(address.to_string()),
        MoveValue::Signer(signer) => serde_json::Value::String(signer.to_string()),
        _ => serde_json::Value::Null,
    }
}

pub fn parse_string_vectors(input: &str) -> String {
    let mut content = input.trim();
    while let Some(start) = content.find('[') {
        if let Some(end) = content.rfind(']') {
            content = &content[start + 1..end].trim();
        } else {
            break;
        }
    }

    let bytes: Result<Vec<u8>, _> = content
        .split(',')
        .map(str::trim)
        .map(|num| num.parse::<u8>())
        .collect();

    match bytes {
        Ok(vec) => from_utf8(&vec)
            .map(String::from)
            .ok()
            .unwrap_or_else(|| content.to_string()),
        Err(_) => content.to_string(), // Handle conversion error
    }
}
