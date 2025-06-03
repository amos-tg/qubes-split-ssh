mod qrexec;
mod sock;

use std::error;
use log::log_err_append;
use tokio::process::{ChildStdin, ChildStdout}; 
use crate::{
    qrexec::QRExecProc,
    sock::InterVMSocketCon,
};

type DynError<T> = Result<T, Box<dyn error::Error>>; 
type DynFutError<T> = Result<T, Box<dyn error::Error + 'static + Send>>;

const ERR_LOG_DIR_NAME: &str = "ssh-client-sock-handler";

const SLEEP_TIME: u64 = 80;
#[tokio::main]
async fn main() -> DynFutError<()> {
    let mut qrexec = {
        let res = QRExecProc::new();
        res.expect("Error: Failed to get qrexec proc..")
    };

    let (cld_stdin, cld_stdout) = (
        ChildStdin::from_std(qrexec.stdin).expect(
            "Error: Failed to produce async qrexec stdin."
        ),
        ChildStdout::from_std(qrexec.stdout).expect(
            "Error: Failed to produce async qrexec stdout."
        ),
    );

    InterVMSocketCon::handler(
        
    ).await?;

    return Ok(());
}
