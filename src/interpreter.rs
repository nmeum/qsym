use libc::{c_int, fork, waitpid};
use qbe_reader::types::*;
use qbe_reader::Definition;
use std::collections::HashMap;
use std::process::exit;
use z3::ast;
use z3::ast::Ast;
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
    solver: z3::Solver<'ctx>,
}

struct Path<'ctx, 'src>(Option<z3::ast::Bool<'ctx>>, &'src Block);

impl<'ctx, 'src> Path<'ctx, 'src> {
    pub fn feasible(&self, solver: &z3::Solver<'ctx>) -> bool {
        let cond = match &self.0 {
            Some(x) => x,
            None => return true,
        };

        let r = solver.check_assumptions(&[cond.clone()]);
        match r {
            z3::SatResult::Unsat => false,
            z3::SatResult::Sat => true,
            z3::SatResult::Unknown => panic!("unknown SAT result"),
        }
    }
}

impl<'ctx, 'src> Interpreter<'ctx, 'src> {
    pub fn new(ctx: &'ctx Context, source: &'src Vec<Definition>) -> Interpreter<'ctx, 'src> {
        let globals = source.iter().filter_map(|x| match x {
            Definition::Func(f) => Some((f.name.clone(), GlobalValue::Func(f))),
            _ => None, // TODO: Global data declarations
        });

        Interpreter {
            ctx: ctx,
            env: Env::new(HashMap::from_iter(globals)),
            solver: z3::Solver::new(&ctx),
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

    fn get_block(&self, label: &str) -> Result<&'src Block, Error> {
        self.env
            .get_block(label)
            .ok_or(Error::UnknownLabel(label.to_string()))
    }

    fn exec_jump(
        &self,
        instr: &JumpInstr,
    ) -> Result<(Path<'ctx, 'src>, Option<Path<'ctx, 'src>>), Error> {
        match instr {
            JumpInstr::Jump(label) => Ok((Path(None, self.get_block(label)?), None)),
            JumpInstr::Jnz(value, nzero_label, zero_label) => {
                let bv = self.get_value(Some(BaseType::Word), value)?;
                let is_zero = bv._eq(&ast::BV::from_u64(self.ctx, 0, bv.get_size()));

                let nzero_path = Path(Some(is_zero.clone().not()), self.get_block(nzero_label)?);
                let zero_path = Path(Some(is_zero), self.get_block(zero_label)?);

                let zero_feasible = zero_path.feasible(&self.solver);
                if zero_feasible && nzero_path.feasible(&self.solver) {
                    Ok((nzero_path, Some(zero_path)))
                } else if zero_feasible {
                    Ok((zero_path, None))
                } else {
                    // non-zero
                    Ok((nzero_path, None))
                }
            }
            JumpInstr::Return(_) => {
                panic!("Return instruction not implemented");
            }
            JumpInstr::Halt => {
                println!("Halting executing");
                Err(Error::HaltExecution)
            }
        }
    }

    #[inline]
    fn explore_path(&mut self, path: &Path) -> Result<(), Error> {
        println!("Exploring path for label {}", path.1.label);

        if let Some(c) = &path.0 {
            self.solver.assert(c);
        }
        self.exec_block(path.1)
    }

    fn exec_block(&mut self, block: &Block) -> Result<(), Error> {
        for stat in block.inst.iter() {
            self.exec_stat(stat)?;
        }

        let targets = self.exec_jump(&block.jump)?;
        match targets {
            (path1, Some(path2)) => unsafe {
                let pid = fork();
                match pid {
                    -1 => Err(Error::ForkFailed),
                    0 => self.explore_path(&path1),
                    _ => {
                        let mut status = 0 as c_int;
                        if waitpid(pid, &mut status as *mut c_int, 0) == -1 {
                            Err(Error::WaitpidFailed)
                        } else {
                            self.explore_path(&path2)
                        }
                    }
                }
            },
            (path, None) => self.explore_path(&path),
        }
    }

    pub fn exec_func(&mut self, name: &String) -> Result<(), Error> {
        let func = self
            .env
            .set_func(name)
            .ok_or(Error::UnknownFunction(name.to_string()))?;

        for param in func.params.iter() {
            let (name, bv) = self.get_func_param(func, param);
            self.env.add_local(name, bv);
        }

        for block in func.body.iter() {
            match self.exec_block(block) {
                Err(Error::HaltExecution) => {
                    self.dump();
                    exit(0)
                }
                Err(x) => return Err(x),
                Ok(x) => x,
            }
        }

        Ok(())
    }

    // XXX: Just a hack to see stuff right now.
    pub fn dump(&self) {
        for (key, value) in self.env.local.iter() {
            println!("\t{} = {}", key, value.simplify());
        }
    }
}
