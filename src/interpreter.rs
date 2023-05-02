use z3::ast;
use z3::Context;

use crate::environment::*;
use qbe_reader::types::*;
use std::collections::HashMap;

pub struct Interpreter<'ctx, 'src> {
    ctx: &'ctx Context, // The Z3 context
    env: Env<'ctx, 'src>,
}

impl<'ctx, 'src> Interpreter<'ctx, 'src> {
    pub fn new(
        ctx: &'ctx Context,
        source: &'src Vec<qbe_reader::Definition>,
    ) -> Interpreter<'ctx, 'src> {
        let globals = source.iter().filter_map(|x| match x {
            qbe_reader::Definition::Func(f) => Some((f.name.clone(), GlobalValue::Func(f))),
            _ => None, // TODO: Global data declarations
        });

        Interpreter {
            ctx: ctx,
            env: Env::new(HashMap::from_iter(globals)),
        }
    }

    fn get_base_type(&self, name: String, ty: &BaseType) -> ast::BV<'ctx> {
        match ty {
            BaseType::Word => ast::BV::new_const(self.ctx, name, 32),
            BaseType::Long => ast::BV::new_const(self.ctx, name, 64),
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
            Const::Number(n) => ast::BV::from_i64(self.ctx, *n, 32),
            Const::Global(s) => panic!("global variables not supported"),
            Const::SFP(_) => panic!("single precision floating points not supported"),
            Const::DFP(_) => panic!("double precision floating points not supported"),
        }
    }

    fn get_dyn_const(&self, dconst: &DynConst) -> ast::BV<'ctx> {
        match dconst {
            DynConst::Const(c)  => self.get_const(c),
            DynConst::Thread(_) => panic!("thread-local constants not supported"),
        }
    }

    fn get_value(&self, value: &Value) -> Option<ast::BV<'ctx>> {
        match value {
            Value::LocalVar(var) => self.env.get_local(var),
            Value::Const(dconst) => Some(self.get_dyn_const(dconst)),
        }
    }

    fn exec_inst(&self, inst: &Instr) -> Option<ast::BV<'ctx>> {
        match inst {
            Instr::Add(v1, v2) => {
                let bv1 = self.get_value(v1)?;
                let bv2 = self.get_value(v2)?;
                Some(bv1.bvadd(&bv2))
            }
        }
    }

    fn exec_stat(&mut self, stat: &Statement) -> Option<()> {
        match stat {
            Statement::Assign(dest, base, inst) => {
                // TODO: Implement subtyping for base
                let result = self.exec_inst(&inst)?;
                self.env.add_local(dest.to_string(), result);
            }
        }

        Some(())
    }

    pub fn exec_func(&mut self, name: &String) -> Option<u32> {
        let func = self.env.get_func(name)?;
        for param in func.params.iter() {
            let (name, bv) = self.get_func_param(func, param);
            self.env.add_local(name, bv);
        }

        let mut num_blocks = 0;
        for block in func.body.iter() {
            num_blocks += 1;
            for stat in block.inst.iter() {
                self.exec_stat(stat)?;
            }
        }

        Some(num_blocks)
    }

    // XXX: Just a hack to see stuff right now.
    pub fn dump(&self) {
        for (key, value) in self.env.local.iter() {
            println!("{} = {}", key, value);
        }
    }
}
