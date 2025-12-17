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
        append(
            &qrexec_res.to_string(),
            DEBUG_FNAME,
            ERR_LOG_DIR_NAME);
        qrexec_res?
    };

    let stream = {
        let stream_res = sock::SockListener::new();
        append(
            &stream_res.to_string(),
            DEBUG_FNAME,
            ERR_LOG_DIR_NAME);
        stream_res?
    };

    let conn_res = stream.handle_connections(
        qrexec.stdin,
        qrexec.stdout);

    append(
        &conn_res.to_string(),
        DEBUG_FNAME,
        ERR_LOG_DIR_NAME);
    conn_res?;

    return Ok(());
}
