mod qrexec;

use crate::qrexec::QRExecProc;
use socket_stdinout::{
    self as sock,
    types::DynError,
    debug::debug_err_append,
    ERR_LOG_DIR_NAME,
};

const DEBUG_FNAME: &str = "Main";

#[tokio::main]
async fn main() {
    let qrexec = {
        let res = QRExecProc::new();
        debug_err_append(
            &res,
            DEBUG_FNAME,
            ERR_LOG_DIR_NAME,
        );
        res.expect("Error: Failed to get qrexec proc..")
    };

    let stream = {
        let res = sock::SockListener::new();
        debug_err_append(
            &res,
            DEBUG_FNAME,
            ERR_LOG_DIR_NAME,
        );
        res.expect("Error: failed to get_auth_sock")
    };

    let con_res = stream.handle_connections(
        qrexec.stdin,
        qrexec.stdout,
    ).await;

    debug_err_append(
        &con_res,
        DEBUG_FNAME,
        ERR_LOG_DIR_NAME,
    );
    con_res.expect("Error: handle_connections returned");
}
