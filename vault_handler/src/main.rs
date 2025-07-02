use socket_stdinout::{
    self as sock,
    ERR_LOG_DIR_NAME,
    debug::debug_err_append,
};
use tokio::{self, io};

pub const INITIAL_STATE_W_FLAG: bool = false;
const DEBUG_FNAME: &str = "Main";

#[tokio::main]
async fn main() {
    let (stdin, stdout) = (
        io::stdin(),  
        io::stdout(),
    );

    let listener = {
        let res = sock::SockStream::get_auth_stream().await;
        debug_err_append(
            &res,
            DEBUG_FNAME,
            ERR_LOG_DIR_NAME,
        );
        res.expect("Error: Failed sock::SockStream::get_auth_sock")
    };

    let con_res = listener.handle_connections(
        INITIAL_STATE_W_FLAG,
        stdout, 
        stdin,
    ).await;

    debug_err_append(
        &con_res,
        DEBUG_FNAME,
        ERR_LOG_DIR_NAME,
    );

    con_res.expect("Error: handle_connections returned");
}

