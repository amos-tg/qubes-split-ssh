use crate::{
    DynError,
    ERR_LOG_DIR_NAME,
};
use socket_stdinout::debug::debug_err_append;

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
        Command,
    },
};

use anyhow::anyhow;

const DEBUG_FNAME: &str = "Qrexec";

#[derive(Debug)]
pub struct DropChild(Child); 

impl Drop for DropChild {
    fn drop(&mut self) {
        let id = self.0.id(); 
        self.0.kill().expect(
            &format!(
                "Error failed to kill qrexec-client-vm during cleanup drop \
                execution: PID for manual kill = {}",
                id,
            )
        );
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

#[derive(Debug)]
pub struct QRExecProc {
    _child: DropChild,
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
}

// I am aware that qrexec-client-vm takes a local program argument
// unfortunately rust does not impl io::Write for io::Stdin, but it 
// does for process::ChildStdin. 

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

        let mut child = DropChild(Command::new("qrexec-client-vm")
            .args([
                &remote_vm, 
                Self::RPC_SERVICE_NAME,
            ])
            .stdin(Stdio::piped()) 
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?
        );

        let stdin = child.stdin.take().ok_or(
            anyhow!(
                "Error: failed to produce a stdin for qrexec child proc."
            )
        )?;

        let stdout = child.stdout.take().ok_or(
            anyhow!(
                "Error: failed to produce a for qrexec child proc."
            )
        )?;

        return Ok(Self {
            _child: child, 
            stdin,
            stdout,
        });
    }
}
