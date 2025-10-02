use std::{
    io::{Stdin, Stdout, Read, self},
    process::{Command, Stdio},
    str::from_utf8,
    time::Duration,
    path::Path,
    thread,
    fs,
};
use crate::{
    DynRes,
    err,
    msgs::pull_stdout,
    make_vm::VmNames,
};
use anyhow::anyhow;

// qvm-run does not always initiate
// the /var/run/qubes/qrexec.* file.
// My get_user function randomly 
// fails and succeeds because of this  
// despite the docs assurances (man qvm-run). 
/// this function is dedicated to 
/// making sure the qrexec file for 
/// a vm exists.
/// the err_msg does not need to include 
/// a Error: at the beginning.
pub fn assure_qrexec(
    vm_name: &str,
) -> DynRes<()> {
    const QRX_TIMEOUT_ERR: &str = 
        "Error: Timed out waiting for the qrexec file in \
        /var/run/qubes";

    start_vm(vm_name)?;
    let path = format!("/var/run/qubes/qrexec.{vm_name}");

    let mut count = 0u8;
    while !fs::exists(&path)? && count <= 20 {
        thread::sleep(Duration::from_secs(1));
        count += 1;
    }

    if count == 20 {
        err!(QRX_TIMEOUT_ERR);
    }

    return Ok(());
}

/// asks the user what their username is after showing a 
/// list of usernames on the VM for the passed in vm_name
pub fn get_user(
    mut stdout: &mut Stdout,
    stdin: &Stdin,
    vm_name: &str,
) -> DynRes<String> {
    const STDERR_ERR: &str = 
        "Error: failed to grab stderr on get_user call";
    const GET_USER_ERR: &str = 
        "Error: Failed to get user; qvm-run failure";
    const STDOUT_ERR: &str = 
        "Error: failed to grab stdout on get_user call"; 

    assure_qrexec(vm_name)?;

    let mut out = Command::new("qvm-run")
        .args([
            "--no-color-output",
            "--no-color-stderr",
            "--pass-io",
            "--user=root",
            vm_name,
            "--",
            "ls", 
            "/home", 
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    loop {
        thread::sleep(Duration::from_secs(1));
        if let Some(status) = out.try_wait()? {
            if status.success() {
                break; 
            } else {
                let child_stderr = &mut out.stderr.ok_or(
                    anyhow!(STDERR_ERR))?;

                let mut stderr = String::new();
                let _ = child_stderr.read_to_string(&mut stderr)?;

                err!(GET_USER_ERR);
            }
        };
    }

    let child_stdout = &mut out.stdout.ok_or(anyhow!(STDOUT_ERR))?;

    let mut ls_stdout = String::new();
    let _ = child_stdout.read_to_string(&mut ls_stdout)?;
    
    let lines = ls_stdout
        .lines()
        .collect::<Vec<&str>>();
    
    return Ok(if lines.len() > 1 {
        println!("{ls_stdout}");
        pull_stdout(
            &mut stdout,
            &stdin,
            &format!(
                "[Which user should be configured for {}?]:",
                vm_name,
            ))?
    } else {
        ls_stdout.trim().to_string()
    });

}

fn start_vm(vm_name: &str) -> DynRes<()> {
    const START_VM_ERR: &str = 
        "Error: failed to start vm";

    let out = Command::new("qvm-start")
        .arg(vm_name)
        .output()?;

    let stderr = from_utf8(&out.stderr)?;
    if stderr.contains("rror") {
        err!(START_VM_ERR);
    }

    return Ok(());
}

pub fn shutdown_vm(vm_name: &str) -> DynRes<()> {
    const SHUTDOWN_ERR: &str = 
        "Error: failed to shutdown vm";

    let out = Command::new("qvm-shutdown")
        .arg(vm_name)
        .output()?;

    let stderr = from_utf8(&out.stderr)?;
    if stderr.contains("rror") {
        err!(SHUTDOWN_ERR);
    }
 
    return Ok(());
}

/// WARNING using this function with an existing file at the 
/// fout_path parameters value will overwrite the file at
/// fout_path.
pub fn qvm_copy(
    fpaths: &[&str],
    to_vm: &str,
) -> DynRes<()> {
    const QVM_COPY_ERR: &str = 
        "Error: qvm-copy-to-vm in qvm_copy failed";
    
    assure_qrexec(to_vm)?;

    let copy_out = Command::new("qvm-copy-to-vm")
        .arg(to_vm)
        .args(fpaths)
        .output()?;

    if !from_utf8(&copy_out.stderr)?.is_empty() {
        return Err(anyhow!(QVM_COPY_ERR).into()); 
    }

    return Ok(());
}
