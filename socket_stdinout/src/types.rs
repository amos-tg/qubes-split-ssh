use std::error::Error;
pub type DynFutError<T> = Result<T, Box<dyn Error + Send + 'static>>; 
pub type DynError<T> = Result<T, Box<dyn Error>>;
