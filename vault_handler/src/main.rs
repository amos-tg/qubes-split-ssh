use std::io;

use socket_stdinout::{
    self as sock,
    ERR_LOG_DIR_NAME,
    debug::append,
    types::DynError,
};

const DEBUG_FNAME: &str = "Main";

fn main() -> DynError<()> {
    let (stdin, stdout) = (io::stdin(), io::stdout());

    let listener = match sock::SockStream::new() {
        Ok(listener) => listener,
        Err(e) => {
            append(
                &e.to_string(),
                DEBUG_FNAME,
                ERR_LOG_DIR_NAME);
            return Err(e);
        }
    };

    if let Err(e) = listener.handle_connections(stdout, stdin) {
        append(
            &e.to_string(),
            DEBUG_FNAME,
            ERR_LOG_DIR_NAME);
        return Err(e.into());
    }

    return Ok(());
}
