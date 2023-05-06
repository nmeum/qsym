use qbe_reader::types::*;
use qbe_reader::Definition;
use std::collections::HashMap;
use z3::ast;
use z3::Context;

use crate::error::*;
use crate::memory::*;
use crate::util::*;

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
    pub fn new(
        ctx: &'ctx Context,
        source: &'src Vec<Definition>,
    ) -> Result<State<'ctx, 'src>, Error> {
        let mut state = State {
            ctx: ctx,

            func: HashMap::new(),
            data: HashMap::new(),
            local: HashMap::new(),

            labels: None,
            mem: Memory::new(ctx),
        };

        let mut func_end_ptr = ast::BV::from_i64(ctx, 0, 64);
        for x in source.into_iter() {
            if let Definition::Func(f) = x {
                func_end_ptr = state.add_func(func_end_ptr.clone(), f);
            }
        }

        let mut data_end_ptr = func_end_ptr.clone();
        for x in source.into_iter() {
            if let Definition::Data(d) = x {
                data_end_ptr = state.add_data(data_end_ptr.clone(), d)?;
            }
        }

        Ok(state)
    }

    fn add_func(&mut self, addr: ast::BV<'ctx>, func: &'src FuncDef) -> ast::BV<'ctx> {
        self.mem
            .store_word(addr.clone(), ast::BV::from_u64(self.ctx, FUNC_PATTERN, 32));
        let end_addr = addr.bvadd(&ast::BV::from_u64(self.ctx, 4, 64));

        self.func.insert(&func.name, (addr.clone(), func));
        end_addr
    }

    fn add_data(
        &mut self,
        addr: ast::BV<'ctx>,
        data: &'src DataDef,
    ) -> Result<ast::BV<'ctx>, Error> {
        // Insert into map before actually inserting the data into memory
        // to support self-referencing data decls: `data $c = { l $c }`.
        self.data.insert(&data.name, (addr.clone(), data));

        let mut end_addr = addr;
        for obj in data.objs.iter() {
            end_addr = self.insert_data_object(end_addr.clone(), obj)?;
        }
        Ok(end_addr)
    }

    fn insert_data_object(
        &mut self,
        addr: ast::BV<'ctx>,
        obj: &DataObj,
    ) -> Result<ast::BV<'ctx>, Error> {
        let mut cur_addr = addr;
        match obj {
            DataObj::DataItem(ty, items) => {
                for item in items.iter() {
                    cur_addr = self.insert_data_item(ty, cur_addr, item)?;
                }
            }
            DataObj::ZeroFill(n) => {
                let zero = ast::BV::from_i64(self.ctx, 0, 8);
                for i in 0..*n {
                    cur_addr = cur_addr.bvadd(&ast::BV::from_u64(self.ctx, i, 64));
                    self.mem.store_byte(cur_addr.clone(), zero.clone())
                }
            }
        }

        Ok(cur_addr)
    }

    pub fn insert_data_item(
        &mut self,
        ty: &ExtType,
        addr: ast::BV<'ctx>,
        item: &DataItem,
    ) -> Result<ast::BV<'ctx>, Error> {
        let mut cur_addr = addr;
        match item {
            DataItem::Symbol(name, offset) => {
                let mut ptr = cast_to(
                    ty,
                    self.get_ptr(name)
                        .ok_or(Error::UnknownVariable(name.to_string()))?,
                );
                if let Some(off) = offset {
                    let off = ast::BV::from_u64(self.ctx, *off, ptr.get_size());
                    ptr = ptr.bvadd(&off);
                }

                assert!(ptr.get_size() % 8 == 0);
                let bytes = (ptr.get_size() / 8) as u64;

                self.mem.store_bitvector(cur_addr.clone(), ptr);
                cur_addr = cur_addr.bvadd(&ast::BV::from_u64(self.ctx, bytes, 64));
            }
            DataItem::String(str) => {
                if *ty != ExtType::Byte {
                    return Err(Error::UnsupportedStringType);
                }
                cur_addr = self.mem.store_string(cur_addr, str);
            }
            // TODO: Reduce code duplication with get_const() from interp.rs
            DataItem::Const(c) => match c {
                Const::Number(n) => {
                    let size = extty_to_size(ty);
                    ast::BV::from_i64(self.ctx, *n, size);
                    cur_addr = cur_addr.bvadd(&ast::BV::from_u64(self.ctx, size as u64, 64));
                }
                Const::SFP(_) => {
                    panic!("single precision floating points not supported")
                }
                Const::DFP(_) => {
                    panic!("double precision floating points not supported")
                }
                Const::Global(_) => unreachable!(),
            },
        }

        Ok(cur_addr)
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
