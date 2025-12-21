mod qrexec;

use crate::qrexec::QRExecProc;
use socket_stdinout::{
    self as sock,
    types::DynError,
    debug::append,
    ERR_LOG_DIR_NAME,
};

const DEBUG_FNAME: &str = "Main";

fn main() -> DynError<()> {
    let qrexec = match QRExecProc::new() {
        Ok(qrexec) => qrexec,
        Err(e) => {
            append(
                &e.to_string(),
                DEBUG_FNAME,
                ERR_LOG_DIR_NAME);
            return Err(e);   
        }
    };

    let stream = match sock::SockListener::new() {
        Ok(stream) => stream, 
        Err(e) => {
            append(
                &e.to_string(),
                DEBUG_FNAME,
                ERR_LOG_DIR_NAME);
            return Err(e);
        }
    };

    if let Err(e) = stream.handle_connections(qrexec.stdin, qrexec.stdout) {
        append(
            &e.to_string(),
            DEBUG_FNAME,
            ERR_LOG_DIR_NAME);
        return Err(e);
    }

    return Ok(());
}
