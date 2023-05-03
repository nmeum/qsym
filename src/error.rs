#[derive(Debug)]
pub enum Error {
    UnknownLabel(String),
    UnknownFunction(String),
    UnknownVariable(String),
    InvalidSubtyping,
}
