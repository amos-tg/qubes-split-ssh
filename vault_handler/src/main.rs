use socket_stdinout as sock;
use log::log_err_append;
use tokio::{self, io};

pub const INITIAL_STATE_W_FLAG: bool = false;

#[tokio::main]
async fn main() {
    let (stdin, stdout) = (
        io::stdin(),  
        io::stdout(),
    );

    let mut listener = {
        let res = sock::SockStream::get_auth_sock().await;
        log_err_append!(&res, sock::ERR_LOG_DIR_NAME);
        res.expect("Error: Failed sock::SockStream::get_auth_sock")
    };

    let con_res = listener.handle_connections(
        INITIAL_STATE_W_FLAG,
        stdout, 
        stdin,
    ).await;
    
    log_err_append!(&con_res, sock::ERR_LOG_DIR_NAME);
    con_res.expect("Error: handle_connections returned");
}

