use qbe_reader::Definition;
use qbe_reader::types::*;
use std::collections::HashMap;
use z3::ast;
use z3::Context;

use crate::environment::*;
use crate::error::*;

//const BYTE_SIZE: u32 = 8;
//const HALF_SIZE: u32 = 16;
const WORD_SIZE: u32 = 32;
const LONG_SIZE: u32 = 64;

pub struct Interpreter<'ctx, 'src> {
    ctx: &'ctx Context, // The Z3 context
    env: Env<'ctx, 'src>,
}

impl<'ctx, 'src> Interpreter<'ctx, 'src> {
    pub fn new(
        ctx: &'ctx Context,
        source: &'src Vec<Definition>,
    ) -> Interpreter<'ctx, 'src> {
        let globals = source.iter().filter_map(|x| match x {
            Definition::Func(f) => Some((f.name.clone(), GlobalValue::Func(f))),
            _ => None, // TODO: Global data declarations
        });

        Interpreter {
            ctx: ctx,
            env: Env::new(HashMap::from_iter(globals)),
        }
    }

    fn get_base_type(&self, name: String, ty: &BaseType) -> ast::BV<'ctx> {
        match ty {
            BaseType::Word => ast::BV::new_const(self.ctx, name, WORD_SIZE),
            BaseType::Long => ast::BV::new_const(self.ctx, name, LONG_SIZE),
            BaseType::Single => panic!("singles not supported"),
            BaseType::Double => panic!("doubles not supported"),
        }
    }

    fn get_type(&self, name: String, ty: &Type) -> ast::BV<'ctx> {
        match ty {
            Type::Base(x) => self.get_base_type(name, x),
            _ => panic!("not implemented"),
        }
    }

    fn get_func_param(&self, func: &FuncDef, param: &FuncParam) -> (String, ast::BV<'ctx>) {
        match param {
            FuncParam::Regular(ty, name) => {
                let ty = self.get_type(func.name.to_string() + ":" + name, ty);
                (name.to_string(), ty)
            }
            FuncParam::Env(_) => panic!("env parameters not supported"),
            FuncParam::Variadic => panic!("varadic functions not supported"),
        }
    }

    fn get_const(&self, constant: &Const) -> ast::BV<'ctx> {
        match constant {
            Const::Number(n) => ast::BV::from_i64(self.ctx, *n, LONG_SIZE),
            Const::Global(_) => panic!("global variables not supported"),
            Const::SFP(_) => panic!("single precision floating points not supported"),
            Const::DFP(_) => panic!("double precision floating points not supported"),
        }
    }

    fn get_dyn_const(&self, dconst: &DynConst) -> ast::BV<'ctx> {
        match dconst {
            DynConst::Const(c) => self.get_const(c),
            DynConst::Thread(_) => panic!("thread-local constants not supported"),
        }
    }

    fn get_value(&self, dest_ty: Option<BaseType>, value: &Value) -> Result<ast::BV<'ctx>, Error> {
        let bv = match value {
            Value::LocalVar(var) => self
                .env
                .get_local(var)
                .ok_or(Error::UnknownVariable(var.to_string())),
            Value::Const(dconst) => Ok(self.get_dyn_const(dconst)),
        }?;

        // See https://c9x.me/compile/doc/il-v1.1.html#Subtyping
        if let Some(x) = dest_ty {
            if x == BaseType::Word && bv.get_size() == 64 {
                let lsb = bv.extract(31, 0); // XXX
                assert!(lsb.get_size() == 32);
                return Ok(lsb);
            } else if x == BaseType::Word && bv.get_size() != 32 {
                return Err(Error::InvalidSubtyping);
            }
        }

        Ok(bv)
    }

    fn exec_inst(&self, dest_ty: Option<BaseType>, inst: &Instr) -> Result<ast::BV<'ctx>, Error> {
        match inst {
            Instr::Add(v1, v2) => {
                let bv1 = self.get_value(dest_ty, v1)?;
                let bv2 = self.get_value(dest_ty, v2)?;
                Ok(bv1.bvadd(&bv2))
            }
        }
    }

    fn exec_stat(&mut self, stat: &Statement) -> Result<(), Error> {
        match stat {
            Statement::Assign(dest, base, inst) => {
                let result = self.exec_inst(Some(*base), &inst)?;
                self.env.add_local(dest.to_string(), result);
            }
        }

        Ok(())
    }

    pub fn exec_func(&mut self, name: &String) -> Result<u32, Error> {
        let func = self
            .env
            .get_func(name)
            .ok_or(Error::UnknownFunction(name.to_string()))?;

        for param in func.params.iter() {
            let (name, bv) = self.get_func_param(func, param);
            self.env.add_local(name, bv);
        }

        let mut num_inst = 0;
        for block in func.body.iter() {
            for stat in block.inst.iter() {
                self.exec_stat(stat)?;
                num_inst += 1;
            }
        }

        Ok(num_inst)
    }

    // XXX: Just a hack to see stuff right now.
    pub fn dump(&self) {
        for (key, value) in self.env.local.iter() {
            println!("{} = {}", key, value);
        }
    }
}
