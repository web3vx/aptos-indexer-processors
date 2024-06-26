use move_core_types::value::MoveTypeLayout;
use regex::Regex;
use std::error::Error;
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

pub fn parse_nested_vectors(input: &str) -> String {
    let mut content = input.trim();
    let mut start_string_result = String::from("");
    let mut end_string_result = String::from("");
    while let Some(start) = content.find('[') {
        if let Some(end) = content.rfind(']') {
            start_string_result.push_str("[");
            end_string_result.push_str("]");
            content = &content[start + 1..end].trim();
        } else {
            break;
        }
    }

    // At this point, `content` should be the innermost list: "114, 95, 110, 97, 109, 101"
    let bytes: Result<Vec<u8>, _> = content
        .split(',')
        .map(str::trim)
        .map(|num| num.parse::<u8>())
        .collect();

    let parsed_string = match bytes {
        Ok(vec) => {
            start_string_result.pop();
            end_string_result.pop();
            from_utf8(&vec)
                .map(String::from)
                .ok()
                .unwrap_or_else(|| content.to_string())
        },
        Err(_) => content.to_string(), // Handle conversion error
    };
    format!(
        "{}{}{}",
        start_string_result, parsed_string, end_string_result
    )
}
