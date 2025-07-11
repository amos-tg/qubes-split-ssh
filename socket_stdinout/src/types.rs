use std::error::Error;

// dynamic thread error
pub type DTErr<T> = Result<T, Box<dyn Error + Send + 'static>>; 
pub type DynError<T> = Result<T, Box<dyn Error>>;
