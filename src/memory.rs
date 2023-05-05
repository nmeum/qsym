use z3::ast;
use z3::Context;
use z3::Sort;

pub struct Memory<'ctx> {
    ctx: &'ctx Context,
    pub data: ast::Array<'ctx>,
}

impl<'ctx> Memory<'ctx> {
    pub fn new(ctx: &'ctx Context) -> Memory<'ctx> {
        let ary = ast::Array::new_const(
            ctx,
            "memory",
            &Sort::bitvector(&ctx, 64), // index type
            &Sort::bitvector(&ctx, 8),  // value type
        );

        Memory {
            ctx: ctx,
            data: ary,
        }
    }

    pub fn store_byte(&mut self, addr: ast::BV<'ctx>, value: ast::BV<'ctx>) {
        assert!(addr.get_size() == 64);
        assert!(value.get_size() == 8);
        self.data = self.data.store(&addr, &value);
    }

    pub fn load_byte(&self, addr: ast::BV<'ctx>) -> ast::BV<'ctx> {
        assert!(addr.get_size() == 64);
        self.data.select(&addr).as_bv().unwrap()
    }

    fn store_bitvector(&mut self, addr: ast::BV<'ctx>, value: ast::BV<'ctx>) {
        assert!(value.get_size() % 8 == 0);
        let amount = value.get_size() / 8;

        // Extract nth bytes from the bitvector
        let bytes = (1..=amount)
            .into_iter()
            .rev()
            .map(|n| value.extract((n * 8) - 1, (n - 1) * 8));

        // Store each byte in memory
        bytes.enumerate().for_each(|(n, b)| {
            assert!(b.get_size() == 8);
            self.store_byte(addr.bvadd(&ast::BV::from_u64(self.ctx, n as u64, 64)), b)
        });
    }

    fn load_bitvector(&self, addr: ast::BV<'ctx>, amount: u64) -> ast::BV<'ctx> {
        // Load amount bytes from memory
        let bytes = (0..amount)
            .into_iter()
            .map(|n| self.load_byte(addr.bvadd(&ast::BV::from_u64(self.ctx, n, 64))));

        // Concat the bytes into a single bitvector
        bytes.reduce(|acc, e| acc.concat(&e)).unwrap()
    }

    pub fn store_word(&mut self, addr: ast::BV<'ctx>, value: ast::BV<'ctx>) {
        assert!(value.get_size() == 32);
        self.store_bitvector(addr, value)
    }

    pub fn load_word(&self, addr: ast::BV<'ctx>) -> ast::BV<'ctx> {
        self.load_bitvector(addr, 4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use z3::ast::Ast;
    use z3::Config;
    use z3::SatResult;
    use z3::Solver;

    #[test]
    fn test_byte() {
        let cfg = Config::new();
        let ctx = Context::new(&cfg);
        let mut mem = Memory::new(&ctx);

        let addr = ast::BV::from_u64(&ctx, 0x800000, 64);
        let value = ast::BV::from_u64(&ctx, 0x23, 8);

        mem.store_byte(addr.clone(), value.clone());
        let loaded = mem.load_byte(addr);

        let solver = Solver::new(&ctx);
        solver.assert(&loaded._eq(&value));
        assert_eq!(SatResult::Sat, solver.check());
    }

    #[test]
    fn test_word() {
        let cfg = Config::new();
        let ctx = Context::new(&cfg);
        let mut mem = Memory::new(&ctx);

        let addr = ast::BV::from_u64(&ctx, 0x1000, 64);
        let word = ast::BV::from_u64(&ctx, 0xdeadbeef, 32);

        mem.store_word(addr.clone(), word.clone());
        let bytes = vec![
            mem.load_byte(ast::BV::from_u64(&ctx, 0x1000, 64)),
            mem.load_byte(ast::BV::from_u64(&ctx, 0x1001, 64)),
            mem.load_byte(ast::BV::from_u64(&ctx, 0x1002, 64)),
            mem.load_byte(ast::BV::from_u64(&ctx, 0x1003, 64)),
        ];

        let solver = Solver::new(&ctx);
        solver.assert(&bytes[0]._eq(&ast::BV::from_u64(&ctx, 0xde, 8)));
        solver.assert(&bytes[1]._eq(&ast::BV::from_u64(&ctx, 0xad, 8)));
        solver.assert(&bytes[2]._eq(&ast::BV::from_u64(&ctx, 0xbe, 8)));
        solver.assert(&bytes[3]._eq(&ast::BV::from_u64(&ctx, 0xef, 8)));
        assert_eq!(SatResult::Sat, solver.check());

        solver.reset();

        let loaded_word = mem.load_word(addr);
        solver.assert(&loaded_word._eq(&word));
        assert_eq!(SatResult::Sat, solver.check());
    }
}
