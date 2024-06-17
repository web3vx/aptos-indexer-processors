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
        "vector<address>" => Some(MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8))),
        "vector<bool>" => Some(MoveTypeLayout::Vector(Box::new(MoveTypeLayout::Bool))),
        "vector<u8>" => Some(MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U8))),
        "vector<u16>" => Some(MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U16))),
        "vector<u32>" => Some(MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U32))),
        "vector<u64>" => Some(MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U64))),
        "vector<u128>" => Some(MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U128))),
        "vector<u256>" => Some(MoveTypeLayout::Vector(Box::new(MoveTypeLayout::U256))),
        _ => None,
    }
}
