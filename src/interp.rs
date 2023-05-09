use libc::{c_int, fork, waitpid};
use qbe_reader::types::*;
use qbe_reader::Definition;

use z3::{
    ast::{Ast, Bool, BV},
    Context,
};

use crate::error::*;
use crate::state::*;
use crate::value::*;

pub struct Interp<'ctx, 'src> {
    v: ValueFactory<'ctx>,
    state: State<'ctx, 'src>,
    solver: z3::Solver<'ctx>,
}

struct Path<'ctx, 'src>(Option<Bool<'ctx>>, &'src Block);

enum FuncReturn<'ctx, 'src> {
    Jump(Path<'ctx, 'src>),
    CondJump(Path<'ctx, 'src>, Path<'ctx, 'src>),
    Return(Option<BV<'ctx>>),
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
            v: ValueFactory::new(ctx),
            state: state,
            solver: z3::Solver::new(&ctx),
        })
    }

    fn get_type(&self, name: String, ty: &Type) -> BV<'ctx> {
        match ty {
            Type::Base(ty) => self.v.from_base(*ty, name),
            _ => panic!("not implemented"),
        }
    }

    fn get_func_param(&self, func: &FuncDef, param: &FuncParam) -> BV<'ctx> {
        match param {
            FuncParam::Regular(ty, name) => self.get_type(func.name.to_string() + ":" + name, ty),
            FuncParam::Env(_) => panic!("env parameters not supported"),
            FuncParam::Variadic => panic!("varadic functions not supported"),
        }
    }

    fn lookup_params(&self, params: &Vec<FuncParam>) -> Result<Vec<BV<'ctx>>, Error> {
        let mut vec: Vec<BV<'ctx>> = Vec::new();
        for param in params.iter() {
            match param {
                FuncParam::Regular(ty, name) => {
                    let mut val = self
                        .state
                        .get_local(name)
                        .ok_or(Error::UnknownVariable(name.to_string()))?;

                    // Calls with a sub-word return type define a temporary of
                    // base type `w` with its most significant bits unspecified.
                    if let Type::SubWordType(swty) = ty {
                        val = self.v.extend_subword(*swty, val)
                    }

                    vec.push(val);
                }
                FuncParam::Env(_) => panic!("env parameters not supported"),
                FuncParam::Variadic => panic!("varadic functions not supported"),
            };
        }

        Ok(vec)
    }

    fn get_const(&self, constant: &Const) -> Result<BV<'ctx>, Error> {
        match constant {
            Const::Number(n) => Ok(self.v.from_base_i64(BaseType::Long, *n)),
            Const::Global(v) => self
                .state
                .get_ptr(v)
                .ok_or(Error::UnknownVariable(v.to_string())),
            Const::SFP(_) => panic!("single precision floating points not supported"),
            Const::DFP(_) => panic!("double precision floating points not supported"),
        }
    }

    fn get_dyn_const(&self, dconst: &DynConst) -> Result<BV<'ctx>, Error> {
        match dconst {
            DynConst::Const(c) => self.get_const(c),
            DynConst::Thread(_) => panic!("thread-local constants not supported"),
        }
    }

    fn get_value(&self, dest_ty: Option<BaseType>, value: &Value) -> Result<BV<'ctx>, Error> {
        let bv = match value {
            Value::LocalVar(var) => self
                .state
                .get_local(var)
                .ok_or(Error::UnknownVariable(var.to_string())),
            Value::Const(dconst) => Ok(self.get_dyn_const(dconst)?),
        }?;

        // See https://c9x.me/compile/doc/il-v1.1.html#Subtyping
        if let Some(x) = dest_ty {
            if x == BaseType::Word && bv.get_size() == LONG_SIZE {
                let lsb = bv.extract(31, 0); // XXX
                assert!(lsb.get_size() == WORD_SIZE);
                return Ok(lsb);
            } else if x == BaseType::Word && bv.get_size() != WORD_SIZE {
                return Err(Error::InvalidSubtyping);
            }
        }

        Ok(bv)
    }

    fn exec_inst(&self, dest_ty: Option<BaseType>, inst: &Instr) -> Result<BV<'ctx>, Error> {
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

    fn exec_stat(&mut self, stat: &'src Statement) -> Result<(), Error> {
        match stat {
            Statement::Assign(dest, base, inst) => {
                let result = self.exec_inst(Some(*base), &inst)?;
                self.state.add_local(dest, result);
            }
            Statement::Call(dest, _ty, fname, params) => {
                let values = self.lookup_params(params)?;
                let func = self
                    .state
                    .get_func(fname)
                    .ok_or(Error::UnknownFunction(fname.to_string()))?;

                let result = self.exec_func(func, values)?;
                if let Some(ret_val) = result {
                    self.state.add_local(dest, ret_val);
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

                assert!(bv.get_size() == WORD_SIZE);
                let is_zero = bv._eq(&self.v.make_word(0));

                let nzero_path = Path(Some(is_zero.not()), self.get_block(nzero_label)?);
                let zero_path = Path(Some(is_zero.clone()), self.get_block(zero_label)?);

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
    fn explore_path(&mut self, path: &Path<'ctx, 'src>) -> Result<Option<BV<'ctx>>, Error> {
        println!("[jnz] Exploring path for label '{}'", path.1.label);

        if let Some(c) = &path.0 {
            self.solver.assert(c);
        }
        self.exec_block(path.1)
    }

    fn exec_block(&mut self, block: &'src Block) -> Result<Option<BV<'ctx>>, Error> {
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
        params: Vec<BV<'ctx>>,
    ) -> Result<Option<BV<'ctx>>, Error> {
        self.state.push_func(func);

        if func.params.len() != params.len() {
            return Err(Error::InvalidCall);
        }
        for i in 0..func.params.len() {
            let name = func.params[i].get_name().unwrap();
            let bv = params[i].clone();
            self.state.add_local(name, bv);
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
        self.state.dump_locals();

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
