use crate::{
    DynError,
    ERR_LOG_DIR_NAME,
};

use socket_stdinout::debug::append;
use std::{
    ops::{
        Deref,
        DerefMut,
    },
    env,
    process::{
        Stdio,
        Child,
        ChildStdout,
        ChildStdin, 
        ChildStderr,
        Command,
    },
};
use anyhow::anyhow;

const DEBUG_FNAME: &str = "Qrexec";

#[derive(Debug)]
pub struct DropChild(Child); 

impl Drop for DropChild {
    fn drop(&mut self) {
        const QRX_KILL_ERR: &str = 
            "Error failed to kill qrexec-client-vm during cleanup drop"; 

        self.0.kill()
            .expect(QRX_KILL_ERR);
    }
}

impl Deref for DropChild {
    type Target = Child;
    fn deref(&self) -> &Self::Target {
        &self.0 
    }
}

impl DerefMut for DropChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct QRExecProc {
    _child: DropChild,
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
    pub stderr: ChildStderr,
}

impl QRExecProc {
    const VAULT_VM_NAME_ENV: &str = "SSH_VAULT_VM";
    const RPC_SERVICE_NAME: &str = "qubes.SshAgent";
    const STDIN_ERR: &str = 
        "Error: failed to produce a stdin for qrexec child proc.";
    const STDOUT_ERR: &str = 
        "Error: failed to produce a stdout for qrexec child proc.";
    const STDERR_ERR: &str = 
        "Error: failed to produce a stderr for qrexec child proc.";

    pub fn new() -> DynError<Self> { 
        let remote_vm = match env::var(Self::VAULT_VM_NAME_ENV) {
            Ok(name) => name,
            Err(e) => {
                append(
                    &e.to_string(),
                    DEBUG_FNAME,
                    ERR_LOG_DIR_NAME);
                return Err(e.into());
            }
        };

        let mut child = DropChild(Command::new("qrexec-client-vm")
            .args([
                &remote_vm, 
                Self::RPC_SERVICE_NAME,
            ])
            .stdin(Stdio::piped()) 
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?);

        let stdin = child.stdin.take().ok_or(
            anyhow!(Self::STDIN_ERR))?;
        let stdout = child.stdout.take().ok_or(
            anyhow!(Self::STDOUT_ERR))?;
        let stderr = child.stderr.take().ok_or(
            anyhow!(Self::STDERR_ERR))?;
        return Ok(Self {
            _child: child, 
            stdin,
            stdout,
            stderr,
        });
    }
}
