mod qrexec;

use crate::qrexec::QRExecProc;
use socket_stdinout::{
    self as sock,
    types::DynError,
    debug::debug_err_append,
    ERR_LOG_DIR_NAME,
};

const DEBUG_FNAME: &str = "Main";

fn main() -> DynError<()> {
    let qrexec = {
        let qrexec_res = QRExecProc::new();
        debug_err_append(
            &qrexec_res,
            DEBUG_FNAME,
            ERR_LOG_DIR_NAME);
        qrexec_res?
    };

    let stream = {
        let stream_res = sock::SockListener::new();
        debug_err_append(
            &stream_res,
            DEBUG_FNAME,
            ERR_LOG_DIR_NAME);
        stream_res?
    };

    let conn_res = stream.handle_connections(
        qrexec.stdin,
        qrexec.stdout);
    debug_err_append(
        &conn_res,
        DEBUG_FNAME,
        ERR_LOG_DIR_NAME);
    conn_res?;

    return Ok(());
}
