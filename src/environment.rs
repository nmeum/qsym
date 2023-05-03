use qbe_reader as qbe;
use std::collections::HashMap;
use z3::ast;

pub enum GlobalValue<'ctx, 'src> {
    Func(&'src qbe::types::FuncDef),
    Data(ast::BV<'ctx>),
}

pub struct Env<'ctx, 'src> {
    global: HashMap<String, GlobalValue<'ctx, 'src>>,
    labels: Option<HashMap<&'src str, &'src qbe::types::Block>>,
    pub local: HashMap<String, ast::BV<'ctx>>,
}

impl<'ctx, 'src> Env<'ctx, 'src> {
    pub fn new(globals: HashMap<String, GlobalValue<'ctx, 'src>>) -> Env<'ctx, 'src> {
        Env {
            global: globals,
            labels: None,
            local: HashMap::new(),
        }
    }

    pub fn set_func(&mut self, name: &String) -> Option<&'src qbe::types::FuncDef> {
        let elem = self.global.get(name)?;
        let func = match elem {
            GlobalValue::Func(x) => Some(x),
            GlobalValue::Data(_) => None,
        }?;

        let blocks = func.body.iter().map(|blk| (blk.label.as_str(), blk));
        self.labels = Some(HashMap::from_iter(blocks));

        Some(func)
    }

    pub fn get_block(&self, name: &str) -> Option<&'src qbe::types::Block> {
        match &self.labels {
            Some(m) => m.get(name).map(|b| *b),
            None => None,
        }
    }

    pub fn add_local(&mut self, name: String, value: ast::BV<'ctx>) {
        self.local.insert(name, value);
    }

    pub fn get_local(&self, name: &String) -> Option<ast::BV<'ctx>> {
        // ast::BV should be an owned object modeled on a C++
        // smart pointer. Hence the clone here is cheap
        self.local.get(name).cloned()
    }

    // TODO: Requires a stack of hash maps.
    // pub fn pop_func(&mut self) {
    //     self.local = HashMap::new();
    // }
}
