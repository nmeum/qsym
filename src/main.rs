mod environment;
mod error;
mod interpreter;

use qbe_reader as qbe;
use std::env;
use z3::{Config, Context};

use interpreter::*;

fn run_qbe(fname: &str, source: Vec<qbe::Definition>) {
    let mut cfg = Config::new();
    cfg.set_model_generation(true);
    let ctx = Context::new(&cfg);

    let mut interp = Interpreter::new(&ctx, &source);
    interp.exec_func(&fname.to_string()).unwrap();
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
