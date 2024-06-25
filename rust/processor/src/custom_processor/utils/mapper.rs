use move_core_types::value::MoveTypeLayout;

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
    if let Some(stripped) = type_string.strip_prefix("vector<").and_then(|s| s.strip_suffix(">")) {
        map_string_to_move_type(stripped).map(|layout| MoveTypeLayout::Vector(Box::new(layout)))
    } else {
        None
    }
}