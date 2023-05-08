use libc::{c_int, fork, waitpid};
use qbe_reader::types::*;
use qbe_reader::Definition;
use z3::ast;
use z3::ast::Ast;
use z3::Context;

use crate::error::*;
use crate::state::*;

//const BYTE_SIZE: u32 = 8;
//const HALF_SIZE: u32 = 16;
const WORD_SIZE: u32 = 32;
const LONG_SIZE: u32 = 64;

pub struct Interp<'ctx, 'src> {
    ctx: &'ctx Context, // The Z3 context
    state: State<'ctx, 'src>,
    solver: z3::Solver<'ctx>,
}

struct Path<'ctx, 'src>(Option<z3::ast::Bool<'ctx>>, &'src Block);

enum FuncReturn<'ctx, 'src> {
    Jump(Path<'ctx, 'src>),
    CondJump(Path<'ctx, 'src>, Path<'ctx, 'src>),
    Return(Option<ast::BV<'ctx>>),
}

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

impl<'ctx, 'src> Interp<'ctx, 'src> {
    pub fn new(
        ctx: &'ctx Context,
        source: &'src Vec<Definition>,
    ) -> Result<Interp<'ctx, 'src>, Error> {
        let state = State::new(&ctx, source)?;
        Ok(Interp {
            ctx: ctx,
            state: state,
            solver: z3::Solver::new(&ctx),
        })
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

    fn get_func_param(&self, func: &FuncDef, param: &FuncParam) -> ast::BV<'ctx> {
        match param {
            FuncParam::Regular(ty, name) => self.get_type(func.name.to_string() + ":" + name, ty),
            FuncParam::Env(_) => panic!("env parameters not supported"),
            FuncParam::Variadic => panic!("varadic functions not supported"),
        }
    }

    // Extend a bitvector of a SubWordType to a word, i.e. 32-bit.
    // The extended bits are treated as unconstrained symbolic.
    pub fn extend_subword(&self, bv: ast::BV<'ctx>) -> ast::BV<'ctx> {
        assert!(bv.get_size() < 32);
        let rem = 32 - bv.get_size();

        let uncons = ast::BV::fresh_const(self.ctx, "undef-msb", rem);
        bv.concat(&uncons)
    }

    fn lookup_params(&self, params: &Vec<FuncParam>) -> Result<Vec<ast::BV<'ctx>>, Error> {
        let mut vec: Vec<ast::BV<'ctx>> = Vec::new();
        for param in params.iter() {
            match param {
                FuncParam::Regular(ty, name) => {
                    let mut val = self
                        .state
                        .get_local(name)
                        .ok_or(Error::UnknownVariable(name.to_string()))?;

                    // Calls with a sub-word return type define a temporary of
                    // base type `w` with its most significant bits unspecified.
                    if let Type::SubWordType(_) = ty {
                        val = self.extend_subword(val)
                    }

                    vec.push(val);
                }
                FuncParam::Env(_) => panic!("env parameters not supported"),
                FuncParam::Variadic => panic!("varadic functions not supported"),
            };
        }

        Ok(vec)
    }

    fn get_const(&self, constant: &Const) -> Result<ast::BV<'ctx>, Error> {
        match constant {
            Const::Number(n) => Ok(ast::BV::from_i64(self.ctx, *n, LONG_SIZE)),
            Const::Global(v) => self
                .state
                .get_ptr(v)
                .ok_or(Error::UnknownVariable(v.to_string())),
            Const::SFP(_) => panic!("single precision floating points not supported"),
            Const::DFP(_) => panic!("double precision floating points not supported"),
        }
    }

    fn get_dyn_const(&self, dconst: &DynConst) -> Result<ast::BV<'ctx>, Error> {
        match dconst {
            DynConst::Const(c) => self.get_const(c),
            DynConst::Thread(_) => panic!("thread-local constants not supported"),
        }
    }

    fn get_value(&self, dest_ty: Option<BaseType>, value: &Value) -> Result<ast::BV<'ctx>, Error> {
        let bv = match value {
            Value::LocalVar(var) => self
                .state
                .get_local(var)
                .ok_or(Error::UnknownVariable(var.to_string())),
            Value::Const(dconst) => Ok(self.get_dyn_const(dconst)?),
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
        // XXX: This instruction simulator assumes that the instructions are
        // well-typed. If not, this causes dubious assertion failures everywhere.
        match inst {
            Instr::Add(v1, v2) => {
                let bv1 = self.get_value(dest_ty, v1)?;
                let bv2 = self.get_value(dest_ty, v2)?;
                Ok(bv1.bvadd(&bv2))
            }
            Instr::LoadWord(v) => {
                let addr = self.get_value(None, v)?;
                Ok(self.state.mem.load_word(addr.simplify()))
            }
            _ => todo!(),
        }
    }

    fn exec_stat(&mut self, stat: &Statement) -> Result<(), Error> {
        match stat {
            Statement::Assign(dest, base, inst) => {
                let result = self.exec_inst(Some(*base), &inst)?;
                self.state.add_local(dest.to_string(), result);
            }
            Statement::Call(dest, _ty, fname, params) => {
                let values = self.lookup_params(params)?;
                let func = self
                    .state
                    .get_func(fname)
                    .ok_or(Error::UnknownFunction(fname.to_string()))?;

                let result = self.exec_func(func, values)?;
                if let Some(ret_val) = result {
                    self.state.add_local(dest.to_string(), ret_val);
                }
            }
        }

        Ok(())
    }

    fn get_block(&self, label: &str) -> Result<&'src Block, Error> {
        self.state
            .get_block(label)
            .ok_or(Error::UnknownLabel(label.to_string()))
    }

    fn exec_jump(&self, instr: &JumpInstr) -> Result<FuncReturn<'ctx, 'src>, Error> {
        match instr {
            JumpInstr::Jump(label) => {
                let path = Path(None, self.get_block(label)?);
                Ok(FuncReturn::Jump(path))
            }
            JumpInstr::Jnz(value, nzero_label, zero_label) => {
                let bv = self.get_value(Some(BaseType::Word), value)?;
                let is_zero = bv._eq(&ast::BV::from_u64(self.ctx, 0, bv.get_size()));

                let nzero_path = Path(Some(is_zero.clone().not()), self.get_block(nzero_label)?);
                let zero_path = Path(Some(is_zero), self.get_block(zero_label)?);

                let zero_feasible = zero_path.feasible(&self.solver);
                if zero_feasible && nzero_path.feasible(&self.solver) {
                    Ok(FuncReturn::CondJump(nzero_path, zero_path))
                } else if zero_feasible {
                    Ok(FuncReturn::Jump(zero_path))
                } else {
                    Ok(FuncReturn::Jump(nzero_path))
                }
            }
            JumpInstr::Return(opt_val) => match opt_val {
                Some(x) => Ok(FuncReturn::Return(Some(self.get_value(None, x)?))),
                None => Ok(FuncReturn::Return(None)),
            },
            JumpInstr::Halt => {
                println!("Halting executing");
                Err(Error::HaltExecution)
            }
        }
    }

    #[inline]
    fn explore_path(&mut self, path: &Path) -> Result<Option<ast::BV<'ctx>>, Error> {
        println!("[jnz] Exploring path for label '{}'", path.1.label);

        if let Some(c) = &path.0 {
            self.solver.assert(c);
        }
        self.exec_block(path.1)
    }

    fn exec_block(&mut self, block: &Block) -> Result<Option<ast::BV<'ctx>>, Error> {
        for stat in block.inst.iter() {
            self.exec_stat(stat)?;
        }

        let targets = self.exec_jump(&block.jump)?;
        match targets {
            // For conditional jumps, we fork(3) the entire interpreter process.
            // This is, obviously, horribly inefficient and will lead to memory
            // explosion issues for any somewhat complex program. In the future,
            // the State module should be modified to allow efficient copies of
            // the state by leveraging a copy-on-write mechanism.
            FuncReturn::CondJump(path1, path2) => unsafe {
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
            FuncReturn::Jump(path) => self.explore_path(&path),
            FuncReturn::Return(value) => Ok(value),
        }
    }

    pub fn exec_func(
        &mut self,
        func: &'src FuncDef,
        params: Vec<ast::BV<'ctx>>,
    ) -> Result<Option<ast::BV<'ctx>>, Error> {
        self.state.push_func(func);

        if func.params.len() != params.len() {
            return Err(Error::InvalidCall);
        }
        for i in 0..func.params.len() {
            let name = func.params[i].get_name().unwrap();
            let bv = params[i].clone();
            self.state.add_local(name.to_string(), bv);
        }

        for block in func.body.iter() {
            match self.exec_block(block) {
                Err(Error::HaltExecution) => {
                    self.dump();
                    return Ok(None);
                }
                Err(x) => return Err(x),
                Ok(x) => {
                    self.state.pop_func();
                    return Ok(x);
                }
            }
        }

        unreachable!();
    }

    // TODO: Reduce code duplication with exec_func
    pub fn exec_symbolic(&mut self, name: &String) -> Result<(), Error> {
        let func = self
            .state
            .get_func(name)
            .ok_or(Error::UnknownFunction(name.to_string()))?;

        let params = func
            .params
            .iter()
            .map(|p| self.get_func_param(func, p))
            .collect();
        self.exec_func(func, params)?;

        Ok(())
    }

    // XXX: Just a hack to see stuff right now.
    pub fn dump(&self) {
        self.solver.check();

        println!("Local variables:");
        for (key, value) in self.state.get_locals().iter() {
            println!("\t{} = {}", key, value.simplify());
        }

        let model = self.solver.get_model();
        match model {
            None => panic!("Couldn't generate a Z3 model"),
            Some(m) => {
                let out = format!("{}", m);
                println!("Symbolic variable values:");
                println!("\t{}", out.replace("\n", "\n\t"));
            }
        };
    }
}
