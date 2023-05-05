use crate::memory::*;
use qbe_reader::types::*;
use qbe_reader::Definition;
use std::collections::HashMap;
use z3::ast;
use z3::Context;

// Bit pattern used to pretend that we actually store functions
// in memory (which we don't) cause we don't have an instruction
// representation. Hence, we just store this pattern instead.
const FUNC_PATTERN: u64 = 0xdeadbeef;

pub struct State<'ctx, 'src> {
    ctx: &'ctx Context,

    func: HashMap<&'src str, (ast::BV<'ctx>, &'src FuncDef)>,
    data: HashMap<&'src str, (ast::BV<'ctx>, &'src DataDef)>,

    labels: Option<HashMap<&'src str, &'src Block>>,

    pub mem: Memory<'ctx>,
    pub local: HashMap<String, ast::BV<'ctx>>,
}

impl<'ctx, 'src> State<'ctx, 'src> {
    pub fn new(ctx: &'ctx Context, source: &'src Vec<Definition>) -> State<'ctx, 'src> {
        let mut state = State {
            ctx: ctx,

            func: HashMap::new(),
            data: HashMap::new(),
            local: HashMap::new(),

            labels: None,
            mem: Memory::new(ctx),
        };

        let mut func_end_ptr = ast::BV::from_i64(ctx, 0, 64);
        source.into_iter().for_each(|x| match x {
            Definition::Func(f) => {
                func_end_ptr = state.add_func(func_end_ptr.clone(), f);
            }
            _ => (),
        });

        let mut data_end_ptr = func_end_ptr.clone();
        source.into_iter().for_each(|x| match x {
            Definition::Data(d) => {
                data_end_ptr = state.add_data(data_end_ptr.clone(), d);
            }
            _ => (),
        });

        state
    }

    fn add_func(&mut self, addr: ast::BV<'ctx>, func: &'src FuncDef) -> ast::BV<'ctx> {
        self.mem
            .store_word(addr.clone(), ast::BV::from_u64(self.ctx, FUNC_PATTERN, 32));
        let end_addr = addr.bvadd(&ast::BV::from_u64(self.ctx, 4, 64));

        self.func.insert(&func.name, (addr.clone(), func));
        end_addr
    }

    fn add_data(&mut self, addr: ast::BV<'ctx>, data: &'src DataDef) -> ast::BV<'ctx> {
        let mut end_addr = addr.clone();
        for obj in data.objs.iter() {
            let inserted_bytes = self.insert_data_object(end_addr.clone(), obj);
            end_addr = end_addr.bvadd(&ast::BV::from_u64(self.ctx, inserted_bytes, 64));
        }

        self.data.insert(&data.name, (addr.clone(), data));
        end_addr
    }

    fn insert_data_object(&mut self, addr: ast::BV<'ctx>, obj: &DataObj) -> u64 {
        match obj {
            DataObj::DataItem(_, _) => {
                todo!()
            }
            DataObj::ZeroFill(n) => {
                let zero = ast::BV::from_i64(self.ctx, 0, 8);
                for i in 0..*n {
                    self.mem.store_byte(
                        addr.bvadd(&ast::BV::from_u64(self.ctx, i, 64)),
                        zero.clone(),
                    )
                }

                *n
            }
        }
    }

    pub fn get_ptr(&self, name: &str) -> Option<ast::BV<'ctx>> {
        // TODO: Check based on end pointer which map we need to consult.
        match self.data.get(name) {
            Some((addr, _)) => Some(addr.clone()),
            None => match self.func.get(name) {
                Some((addr, _)) => Some(addr.clone()),
                None => None,
            },
        }
    }

    // TODO: Return a FuncFrame type here and store functional-local information in it.
    pub fn set_func(&mut self, name: &str) -> Option<&'src FuncDef> {
        let (_, func) = self.func.get(name)?;

        let blocks = func.body.iter().map(|blk| (blk.label.as_str(), blk));
        self.labels = Some(HashMap::from_iter(blocks));

        Some(func)
    }

    pub fn get_block(&self, name: &str) -> Option<&'src Block> {
        match &self.labels {
            Some(m) => m.get(name).map(|b| *b),
            None => None,
        }
    }

    pub fn add_local(&mut self, name: String, value: ast::BV<'ctx>) {
        self.local.insert(name, value);
    }

    pub fn get_local(&self, name: &str) -> Option<ast::BV<'ctx>> {
        // ast::BV should be an owned object modeled on a C++
        // smart pointer. Hence the clone here is cheap
        self.local.get(name).cloned()
    }

    // TODO: Requires a stack of hash maps.
    // pub fn pop_func(&mut self) {
    //     self.local = HashMap::new();
    // }
}
