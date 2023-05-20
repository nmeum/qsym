use qbe_reader::types::*;
use z3::{ast::BV, Context};

// TODO: Would be cool if we could enforce some additional type
// safety via this abstraction. For example, avoiding that BVs
// of different sizes are added, multiplied, et cetera.

pub const BYTE_SIZE: u32 = 8;
pub const HALF_SIZE: u32 = 16;
pub const WORD_SIZE: u32 = 32;
pub const LONG_SIZE: u32 = 64;

pub struct ValueFactory<'ctx> {
    ctx: &'ctx Context,
}

impl<'ctx> ValueFactory<'ctx> {
    pub fn new(ctx: &'ctx Context) -> ValueFactory<'ctx> {
        return ValueFactory { ctx };
    }

    ////
    // Associated Methods
    ////

    fn basety_to_size(ty: BaseType) -> u32 {
        match ty {
            BaseType::Word => WORD_SIZE,
            BaseType::Long => LONG_SIZE,
            BaseType::Single => panic!("floating points not supported"),
            BaseType::Double => panic!("floating points not supported"),
        }
    }

    fn extty_to_size(ty: ExtType) -> u32 {
        match ty {
            ExtType::Base(b) => Self::basety_to_size(b),
            ExtType::Byte => BYTE_SIZE,
            ExtType::Halfword => HALF_SIZE,
        }
    }

    fn subwty_to_size(ty: SubWordType) -> u32 {
        match ty {
            SubWordType::SignedByte => BYTE_SIZE,
            SubWordType::UnsignedByte => BYTE_SIZE,
            SubWordType::SignedHalf => HALF_SIZE,
            SubWordType::UnsignedHalf => HALF_SIZE,
        }
    }

    fn sublty_to_size(ty: SubLongType) -> u32 {
        match ty {
            SubLongType::SubWord(x) => Self::subwty_to_size(x),
            SubLongType::UnsignedWord => 32,
            SubLongType::SignedWord => 32,
        }
    }

    pub fn loadty_to_size(ty: LoadType) -> u32 {
        match ty {
            LoadType::Base(x) => Self::basety_to_size(x),
            LoadType::SubLong(x) => Self::sublty_to_size(x),
        }
    }

    ////
    // Bitvector Factory Functions
    ////

    pub fn from_ext(&self, ty: ExtType, name: String) -> BV<'ctx> {
        let size = Self::extty_to_size(ty);
        BV::new_const(self.ctx, name, size)
    }

    pub fn from_ext_i64(&self, ty: ExtType, v: i64) -> BV<'ctx> {
        let size = Self::extty_to_size(ty);
        BV::from_i64(self.ctx, v, size)
    }

    pub fn from_base(&self, ty: BaseType, name: String) -> BV<'ctx> {
        let size = Self::basety_to_size(ty);
        BV::new_const(self.ctx, name, size)
    }

    pub fn from_base_u64(&self, ty: BaseType, v: u64) -> BV<'ctx> {
        let size = Self::basety_to_size(ty);
        BV::from_u64(self.ctx, v, size)
    }

    pub fn from_base_i64(&self, ty: BaseType, v: i64) -> BV<'ctx> {
        let size = Self::basety_to_size(ty);
        BV::from_i64(self.ctx, v, size)
    }

    ////
    // Operations on created Bitvectors
    ////

    // Extend a bitvector of a SubWordType to a word, i.e. 32-bit.
    // The extended bits are treated as unconstrained symbolic this
    // is the case because QBE mandates that the most significant
    // bits of an extended subword are unspecified/undefined.
    pub fn extend_subword(&self, ty: SubWordType, val: BV<'ctx>) -> BV<'ctx> {
        let size = Self::subwty_to_size(ty);
        assert!(val.get_size() == size);

        assert!(val.get_size() < 32);
        let rem = WORD_SIZE - size;

        let uncons = BV::fresh_const(self.ctx, "undef-msbsw", rem);
        val.concat(&uncons) // TODO: Does this set the MSB?
    }

    pub fn cast_to(&self, ty: ExtType, val: BV<'ctx>) -> BV<'ctx> {
        let cur_size = val.get_size();
        let tgt_size = Self::extty_to_size(ty);

        if tgt_size == cur_size {
            val
        } else if tgt_size > cur_size {
            val.zero_ext(tgt_size - cur_size)
        } else {
            val.extract(tgt_size - 1, 0)
        }
    }

    pub fn sign_ext_to(&self, ty: BaseType, val: BV<'ctx>) -> BV<'ctx> {
        let cur_size = val.get_size();
        let tgt_size = Self::basety_to_size(ty);
        if cur_size == tgt_size {
            return val;
        }

        assert!(tgt_size > cur_size);
        val.sign_ext(tgt_size - cur_size)
    }

    pub fn zero_ext_to(&self, ty: BaseType, val: BV<'ctx>) -> BV<'ctx> {
        let cur_size = val.get_size();
        let tgt_size = Self::basety_to_size(ty);
        if cur_size == tgt_size {
            return val;
        }

        assert!(tgt_size > cur_size);
        val.zero_ext(tgt_size - cur_size)
    }

    ////
    // Syntatic Sugar
    ////

    pub fn make_byte(&self, v: u8) -> BV<'ctx> {
        BV::from_u64(self.ctx, v.into(), BYTE_SIZE)
    }
    pub fn make_half(&self, v: u16) -> BV<'ctx> {
        BV::from_u64(self.ctx, v.into(), HALF_SIZE)
    }
    pub fn make_word(&self, v: u32) -> BV<'ctx> {
        BV::from_u64(self.ctx, v.into(), WORD_SIZE)
    }
    pub fn make_long(&self, v: u64) -> BV<'ctx> {
        BV::from_u64(self.ctx, v, LONG_SIZE)
    }
}
