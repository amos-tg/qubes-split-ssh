use std::io;

use socket_stdinout::{
    self as sock,
    ERR_LOG_DIR_NAME,
    debug::append,
    types::DynError,
};

const DEBUG_FNAME: &str = "Main";

fn main() -> DynError<()> {
    let (stdin, stdout) = (
        io::stdin(),  
        io::stdout());

    let listener = {
        let sock_res = sock::SockStream::new();
        append(
            &sock_res.to_string(),
            DEBUG_FNAME,
            ERR_LOG_DIR_NAME);
        sock_res?
    };

    let conn_res = listener.handle_connections(
        stdout, 
        stdin);

    append(
        &conn_res.to_string(),
        DEBUG_FNAME,
        ERR_LOG_DIR_NAME);

    conn_res?;

    return Ok(());
}
