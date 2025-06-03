use std::{
    fs,
    io::{Stdin, Stdout, Read},
    process::{Command, Stdio},
    str::from_utf8,
    time::Duration,
    thread,
};

use crate::{
    DynRes,
    err,
    pull_stdout,
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
    start_vm(vm_name)?;
    let path = format!("/var/run/qubes/qrexec.{vm_name}");

    let mut cnt = 0u8;
    while !fs::exists(&path)? && cnt <= 20 {
        thread::sleep(Duration::from_secs(1));
        cnt += 1;
    }

    if cnt == 20 {
        err!(format!(
            "Error: Timed out waiting for the qrexec file in /var/run/qubes for VM {}",
            vm_name,
        ));
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
        .spawn()?
    ;

    loop {
        thread::sleep(Duration::from_secs(1));
        if let Some(status) = out.try_wait()? {
            if status.success() {
                break; 
            } else {
                let child_stderr = &mut out.stderr.ok_or(anyhow!(format!(
                    "Error: failed to grab stderr on get_user call. VM: {}",
                    vm_name,
                )))?;

                let mut stderr = String::new();
                let _ = child_stderr.read_to_string(&mut stderr)?;

                err!(format!(
                    "Error: Failed to get user for VM: {}, qvm-run exited with code {}\n\nStderr: {}",
                    vm_name, status, stderr,
                ));
            }
        };
    }

    let child_stdout = &mut out.stdout.ok_or(anyhow!(format!(
        "Error: failed to grab stdout on get_user call. VM: {}",
        vm_name,
    )))?;

    let mut ls_stdout = String::new();
    let _ = child_stdout.read_to_string(&mut ls_stdout)?;
    
    let lines = ls_stdout
        .lines()
        .collect::<Vec<&str>>()
    ;
    
    return Ok(if lines.len() > 1 {
        println!("{ls_stdout}");
        pull_stdout(
            &mut stdout,
            &stdin,
            &format!(
                "[Which user should be configured for {}?]:",
                vm_name,
            ),
        )?
    } else {
        ls_stdout.trim().to_string()
    });

}

fn start_vm(vm_name: &str) -> DynRes<()> {
    let out = Command::new("qvm-start")
        .arg(vm_name)
        .output()?
    ;

    let stderr = from_utf8(&out.stderr)?;
    if stderr.contains("rror") {
        return Err(anyhow!( 
            "Error: failed to start server template: {}",
            vm_name,
        ).into());
    }

    return Ok(());
}

pub fn shutdown_vm(vm_name: &str) -> DynRes<()> {
    let out = Command::new("qvm-shutdown")
        .arg(vm_name)
        .output()?
    ;

    let stderr = from_utf8(&out.stderr)?;
    if stderr.contains("rror") {
        return Err(anyhow!( 
            "Error: failed to start server template: {}",
            vm_name,
        ).into());
    }
 
    return Ok(());
}
