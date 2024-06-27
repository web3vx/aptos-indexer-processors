use move_core_types::value::{MoveTypeLayout, MoveValue};
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
        "0x1::string::String" => Some(MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8))),
        _ => parse_vector(type_string),
    }
}

fn parse_vector(type_string: &str) -> Option<MoveTypeLayout> {
    if let Some(stripped) = type_string
        .strip_prefix("vector<")
        .and_then(|s| s.strip_suffix(">"))
    {
        map_string_to_move_type(stripped).map(|layout| MoveTypeLayout::Vector(Box::new(layout)))
    } else {
        None
    }
}

pub fn parse_nested_move_values(input: &MoveValue) -> String {
    match input {
        MoveValue::Vector(vec) => {
            if vec.is_empty() {
                return String::from("[]");
            }
            if let MoveValue::U8(_) = vec[0] {
                return format!("\"{}\"",parse_string_vectors(&input.to_string()))
            }
            let mut result = String::from("[");
            for value in vec {
                result.push_str(&parse_nested_move_values(value));
                result.push_str(", ");
            }
            result.pop();
            result.pop();
            result.push(']');
            result
        },
        MoveValue::U8(byte) => byte.to_string(),
        MoveValue::U64(num) => num.to_string(),
        MoveValue::U128(num) => num.to_string(),
        MoveValue::U256(num) => num.to_string(),
        MoveValue::Bool(boolean) => boolean.to_string(),
        MoveValue::Address(address) => format!("\"{}\"", address.to_string()),
        MoveValue::Signer(signer) => format!("\"{}\"", signer.to_string()),
        _ => String::from("Unsupported type"),
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
