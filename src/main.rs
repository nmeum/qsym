use z3::ast;
use z3::{Config, Context};

use std::env;
use qbe_reader as qbe;
use qbe::types::*;
use std::collections::HashMap;

type Env<'ctx> = HashMap<String, ast::BV<'ctx>>;

fn get_base_type<'ctx>(ctx: &'ctx Context, name: String, ty: &BaseType) -> ast::BV<'ctx> {
    match ty {
        BaseType::Word   => ast::BV::new_const(&ctx, name.clone(), 32),
        BaseType::Long   => panic!("longs not supported"),
        BaseType::Single => panic!("singles not supported"),
        BaseType::Double => panic!("doubles not supported"),
    }
}

fn get_value<'ctx>(_ctx: &'ctx Context, env: &'ctx Env, value: &Value) -> &'ctx ast::BV<'ctx> {
    match value {
        Value::LocalVar(var) => {
            env.get(var).unwrap()
        },
        Value::GlobalVar(_) => panic!("not implemented"),
        Value::Const(_) => panic!("not implemented"),
    }
}

fn get_type<'ctx>(ctx: &'ctx Context, name: String, ty: &Type) -> ast::BV<'ctx> {
    match ty {
        Type::Base(x) => get_base_type(ctx, name, x),
        _             => panic!("not implemented"),
    }
}

fn get_params<'ctx>(ctx: &'ctx Context, params: &Vec<FuncParam>) -> Env<'ctx> {
    let vec = params
        .iter()
        .map(|p| match p {
            FuncParam::Regular(ty, name) => {
                (name.to_owned(), get_type(ctx, name.to_owned(), ty))
            },
            _ => panic!("not implemented"),
        });

    HashMap::from_iter(vec)
}

fn exec_assign<'ctx>(ctx: &'ctx Context, env: &'ctx Env, dest: &String, _ty: &BaseType, inst: &Instr) {
    match inst {
        Instr::Add(v1, v2) => {
            let bv1 = get_value(&ctx, env, v1);
            let bv2 = get_value(&ctx, env, v2);
            println!("{} = {}", dest, bv1.bvadd(bv2));
        },
    }
}

fn run_blocks<'ctx>(ctx: &'ctx Context, env: &'ctx Env, blocks: &Vec<Block>) {
    for block in blocks.iter() {
        println!("Executing block: {}", block.label);
        for stat in block.inst.iter() {
            match stat {
                Statement::Assign(dest, base, inst) => {
                    exec_assign(ctx, env, dest, base, inst);
                },
            }
        }
    }
}

fn run_qbe(fname: &str, source: Vec<qbe_reader::Definition>) {
    // TODO: initialize data in memory

    let cfg = Config::new();
    let ctx = Context::new(&cfg);

    let funcs = source
        .iter()
        .filter_map(
            |x| match x {
                qbe::Definition::Func(f) => Some((f.name.clone(), f)),
                _ => None,
            }
        );
    let func_map: HashMap<String, &FuncDef> = HashMap::from_iter(funcs);

    let func = func_map.get(fname).unwrap();
    let params = get_params(&ctx, &func.params);

    run_blocks(&ctx, &params, &func.body);
}

fn main() {
    let mut args = env::args();
    let prog = args.next().unwrap();

    if args.len() <= 1 {
        eprintln!("Usage: {} FILE FUNC", prog);
    } else {
        let path = args.next().unwrap();
        let func = args.next().unwrap();

        let defs = qbe::parse_file(path).unwrap();
        run_qbe(&func, defs);
    }
}
