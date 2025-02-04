#[derive(thiserror::Error, Debug)]
pub enum RuntimeError {
    #[error("Invalid character found")]
    UnexpectedChar,
}
