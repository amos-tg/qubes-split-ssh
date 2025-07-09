use crate::{
    DynError,
    ERR_LOG_DIR_NAME,
};
use socket_stdinout::debug::debug_err_append;
use std::{
    env,
    process::Stdio,
};
use anyhow::anyhow;
use tokio::process::{
    Child,
    ChildStdout,
    ChildStdin,
    Command,
};

const DEBUG_FNAME: &str = "Qrexec";

#[derive(Debug)] pub struct QRExecProc {
    _child: Child,
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
}

impl QRExecProc {
    const VAULT_VM_NAME_ENV_VAR: &str = "SSH_VAULT_VM";
    const RPC_SERVICE_NAME: &str = "qubes.SplitSSHAgent";
    pub fn new() -> DynError<Self> { 
        let remote_vm = {
            let var = env::var(Self::VAULT_VM_NAME_ENV_VAR);
            debug_err_append(
                &var,
                DEBUG_FNAME,
                ERR_LOG_DIR_NAME,
            );
            var?
        };

        let mut child = Command::new("qrexec-client-vm")
            .args([
                &remote_vm, 
                Self::RPC_SERVICE_NAME,
            ])
            .stdin(Stdio::piped()) 
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?
        ;

        let stdin = child.stdin.take().ok_or(anyhow!(
            "Error: failed to produce a stdin for qrexec child proc."
        ))?;

        let stdout = child.stdout.take().ok_or(anyhow!(
            "Error: failed to produce a for qrexec child proc."
        ))?;

        return Ok(Self {
            _child: child, 
            stdin,
            stdout,
        });
    }
}
