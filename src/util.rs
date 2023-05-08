use qbe_reader::types::*;
use z3::ast::BV;

pub fn basety_to_size(ty: &BaseType) -> u32 {
    match ty {
        BaseType::Word => 32,
        BaseType::Long => 64,
        BaseType::Single => panic!("floating points not supported"),
        BaseType::Double => panic!("floating points not supported"),
    }
}

pub fn extty_to_size(ty: &ExtType) -> u32 {
    match ty {
        ExtType::Base(b) => basety_to_size(b),
        ExtType::Byte => 8,
        ExtType::Halfword => 16,
    }
}

// TODO: This only reduces the size of the bitvector
// and does not increase the size of the bitvector.
pub fn cast_to<'ctx>(ty: &ExtType, bv: BV<'ctx>) -> BV<'ctx> {
    let byte_size = extty_to_size(ty);
    bv.extract(byte_size - 1, 0)
}
