#[derive(Debug)]
pub enum Error {
    HaltExecution,
    UnknownLabel(String),
    UnknownFunction(String),
    UnknownVariable(String),
    InvalidSubtyping,
    ForkFailed,
    WaitpidFailed,
    UnsupportedStringType,
    InvalidCall,
}
