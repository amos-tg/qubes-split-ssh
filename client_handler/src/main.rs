mod qrexec;

use crate::qrexec::QRExecProc;
use tokio::process::{ChildStdin, ChildStdout}; 
use log::log_err_append;
use socket_stdinout::{
    self as sock,
    DynError,
    ERR_LOG_DIR_NAME,
};

const INITIAL_STATE_W_FLAG: bool = true;
#[tokio::main]
async fn main() {
    let qrexec = {
        let res = QRExecProc::new();
        log_err_append!(&res, ERR_LOG_DIR_NAME);
        res.expect("Error: Failed to get qrexec proc..")
    };

    let (qchild_stdin, qchild_stdout) = (
        ChildStdin::from_std(qrexec.stdin).expect(
            "Error: Failed to produce async qrexec stdin."
        ),
        ChildStdout::from_std(qrexec.stdout).expect(
            "Error: Failed to produce async qrexec stdout."
        ),
    );

    let mut stream = {
        let res = sock::SockStream::get_auth_sock().await;
        log_err_append!(&res, ERR_LOG_DIR_NAME);
        res.expect("Error: failed to get_auth_sock")
    };

    let con_res = stream.handle_connections(
        crate::INITIAL_STATE_W_FLAG,
        qchild_stdin,
        qchild_stdout,
    ).await;

    log_err_append!(&con_res, ERR_LOG_DIR_NAME);
    con_res.expect("Error: handle_connections returned");
}
