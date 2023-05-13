use qbe_reader::types::*;
use qbe_reader::Definition;
use std::collections::HashMap;

use z3::{
    ast::{Ast, BV},
    Context,
};

use crate::error::*;
use crate::memory::*;
use crate::value::*;

// Bit pattern used to pretend that we actually store functions
// in memory (which we don't) cause we don't have an instruction
// representation. Hence, we just store this pattern instead.
//
// TODO: Just store unconstrained symbolic bytes instead.
const FUNC_PATTERN: u32 = 0xdeadbeef;

struct FuncState<'ctx, 'src> {
    labels: HashMap<&'src str, &'src Block>,
    local: HashMap<&'src str, BV<'ctx>>,

    // Value of the stack pointer when this stack frame was created.
    stkptr: BV<'ctx>,
}

pub struct State<'ctx, 'src> {
    v: ValueFactory<'ctx>,
    pub mem: Memory<'ctx>,
    stkptr: BV<'ctx>,

    func: HashMap<&'src str, (BV<'ctx>, &'src FuncDef)>,
    data: HashMap<&'src str, (BV<'ctx>, &'src DataDef)>,
    stck: Vec<FuncState<'ctx, 'src>>,
}

impl<'ctx, 'src> State<'ctx, 'src> {
    pub fn new(
        ctx: &'ctx Context,
        source: &'src Vec<Definition>,
    ) -> Result<State<'ctx, 'src>, Error> {
        let v = ValueFactory::new(ctx);
        let mut state = State {
            stkptr: v.make_long(0),
            v,

            func: HashMap::new(),
            data: HashMap::new(),
            stck: Vec::new(),

            mem: Memory::new(ctx),
        };

        let mut func_end_ptr = state.v.make_long(0);
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

        state.stkptr = data_end_ptr;
        Ok(state)
    }

    fn add_func(&mut self, addr: BV<'ctx>, func: &'src FuncDef) -> BV<'ctx> {
        self.mem
            .store_word(addr.clone(), self.v.make_word(FUNC_PATTERN));
        let end_addr = addr.bvadd(&self.v.make_long(4));

        self.func.insert(&func.name, (addr.clone(), func));
        end_addr
    }

    fn add_data(&mut self, addr: BV<'ctx>, data: &'src DataDef) -> Result<BV<'ctx>, Error> {
        // Insert into map before actually inserting the data into memory
        // to support self-referencing data decls: `data $c = { l $c }`.
        self.data.insert(&data.name, (addr.clone(), data));

        let mut end_addr = addr;
        for obj in data.objs.iter() {
            end_addr = self.insert_data_object(end_addr.clone(), obj)?;
        }
        Ok(end_addr)
    }

    fn insert_data_object(&mut self, addr: BV<'ctx>, obj: &DataObj) -> Result<BV<'ctx>, Error> {
        let mut cur_addr = addr;
        match obj {
            DataObj::DataItem(ty, items) => {
                for item in items.iter() {
                    cur_addr = self.insert_data_item(ty, cur_addr, item)?;
                }
            }
            DataObj::ZeroFill(n) => {
                let zero = self.v.make_byte(0);
                for i in 0..*n {
                    cur_addr = cur_addr.bvadd(&self.v.make_long(i));
                    self.mem.store_byte(cur_addr.clone(), zero.clone())
                }
            }
        }

        Ok(cur_addr)
    }

    pub fn insert_data_item(
        &mut self,
        ty: &ExtType,
        addr: BV<'ctx>,
        item: &DataItem,
    ) -> Result<BV<'ctx>, Error> {
        let mut cur_addr = addr;
        match item {
            DataItem::Symbol(name, offset) => {
                let mut ptr = self
                    .get_ptr(name)
                    .ok_or(Error::UnknownVariable(name.to_string()))?;
                assert!(ptr.get_size() == LONG_SIZE);
                if let Some(off) = offset {
                    let off = self.v.make_long(*off);
                    ptr = ptr.bvadd(&off);
                }

                assert!(ptr.get_size() % 8 == 0);
                let bytes = (ptr.get_size() / 8) as u64;

                self.mem.store_bitvector(cur_addr.clone(), ptr);
                cur_addr = cur_addr.bvadd(&self.v.make_long(bytes));
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
                    let bv = self.v.from_ext_i64(*ty, *n);
                    let size = bv.get_size() as u64;
                    self.mem.store_bitvector(cur_addr.clone(), bv);

                    assert!(size % 8 == 0);
                    cur_addr = cur_addr.bvadd(&self.v.make_long(size / 8));
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

    pub fn get_ptr(&self, name: &str) -> Option<BV<'ctx>> {
        // TODO: Check based on end pointer which map we need to consult.
        match self.data.get(name) {
            Some((addr, _)) => Some(addr.clone()),
            None => match self.func.get(name) {
                Some((addr, _)) => Some(addr.clone()),
                None => None,
            },
        }
    }

    pub fn get_func(&mut self, name: &str) -> Option<&'src FuncDef> {
        Some(self.func.get(name)?.1)
    }

    pub fn stack_size(&self) -> usize {
        self.stck.len()
    }

    pub fn stack_alloc(&mut self, align: u64, size: u64) -> BV<'ctx> {
        assert!(self.stck.len() != 0);

        // (addr - (addr % alignment)) + alignment
        let aligned_addr = self
            .stkptr
            .bvsub(&self.stkptr.bvurem(&self.v.make_long(align)))
            .bvadd(&self.v.make_long(align));
        self.stkptr = aligned_addr.bvadd(&self.v.make_long(size));

        assert!(aligned_addr.get_size() == LONG_SIZE);
        aligned_addr.clone()
    }

    /////
    // Function-local operations
    /////

    pub fn push_func(&mut self, func: &'src FuncDef) {
        let blocks = func.body.iter().map(|blk| (blk.label.as_str(), blk));
        let state = FuncState {
            labels: HashMap::from_iter(blocks),
            local: HashMap::new(),
            stkptr: self.stkptr.clone(),
        };

        self.stck.push(state);
    }

    pub fn get_block(&self, name: &str) -> Option<&'src Block> {
        let func = self.stck.last().unwrap();
        func.labels.get(name).map(|b| *b)
    }

    pub fn add_local(&mut self, name: &'src str, value: BV<'ctx>) {
        let func = self.stck.last_mut().unwrap();
        func.local.insert(name, value);
    }

    pub fn get_local(&self, name: &str) -> Option<BV<'ctx>> {
        let func = self.stck.last().unwrap();
        // BV should be an owned object modeled on a C++
        // smart pointer. Hence the clone here is cheap
        func.local.get(name).cloned()
    }

    pub fn pop_func(&mut self) {
        let func = self.stck.pop().unwrap();
        self.stkptr = func.stkptr;
    }

    // TODO: Remove this
    pub fn dump_locals(&self) {
        let func = self.stck.last().unwrap();

        let mut v: Vec<_> = func.local.iter().collect();
        v.sort_by_key(|a| a.0);

        for (key, value) in v.iter() {
            println!("\t{} = {}", key, value.simplify());
        }
    }
}
