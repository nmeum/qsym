use qbe_reader::types::*;
use z3::ast;

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

pub fn cast_to<'ctx>(ty: &ExtType, bv: ast::BV<'ctx>) -> ast::BV<'ctx> {
    let byte_size = extty_to_size(ty);
    bv.extract(byte_size - 1, 0)
}
