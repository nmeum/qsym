#[derive(Debug)]
pub enum Error {
    UnknownFunction(String),
    UnknownVariable(String),
    InvalidSubtyping,
}
