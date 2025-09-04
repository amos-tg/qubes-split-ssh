use std::{
    fs,
    io::{Stdin, Stdout},
    process::Command,
};
use crate::{
    DynRes,
    make_vm::VmNames,
    SALT_FILES_DIR,
    salt::{
        parse_verify_file,
        parse_verify_state, 
        SlsVmComplement,
    },
    qvm::{
        assure_qrexec,
        shutdown_vm,
        get_user,
    },
    TMP_DIR,
    STATE_DIR,
};

pub fn maint_files_rust(
    vm_names: &VmNames, 
    stdout: &mut Stdout,
    stdin: &Stdin,
) -> DynRes<SlsVmComplement> {
    let tuser = get_user(vm_names.client_template)?; 

    todo!();
}

pub fn maint_files_socat(
    mut stdout: &mut Stdout,
    stdin: &Stdin,
    vm_names: &VmNames, 
    files_dir: &Option<String>,
    states_dir: &Option<String>,
    file_roots: &Vec<String>,
) -> DynRes<SlsVmComplement> {
    let mut states = SlsVmComplement {
        target_vm: String::from(&vm_names.client_template),
        states: vec![],
    };

    assure_qrexec(&vm_names.dvm_client)?;
    let user = get_user(
        &mut stdout,
        &stdin,
        &vm_names.dvm_client)?;
    shutdown_vm(&vm_names.dvm_client)?;

    states.states.push(
        agent_service_file(
            vm_names,
            files_dir,
            states_dir,
            &user,
            file_roots)?);

    states.states.push(
        global_bashrc_file(
            &vm_names.server_appvm,
            files_dir,
            states_dir,
            file_roots)?);

    return Ok(states);
}

fn agent_service_file(
    vm_names: &VmNames,
    files_dir: &Option<String>,
    states_dir: &Option<String>,
    user: &str,
    file_roots: &Vec<String>,
) -> DynRes<String> {
    const SCRIPT_PATH: &str = 
        "/opt/split-ssh/socket-script.sh";
    const DESKTOP_PATH: &str = 
        "/etc/xdg/autostart/split-ssh-socket.desktop";
    
    let sock_script_cont = format!(
r#"#! /bin/bash
#
# Split SSH Configuration
TMP_DIR="{TMP_DIR}"

if [ ! -d $TMP_DIR ]; then
  mkdir -p $TMP_DIR 
fi

SSH_VAULT_VM="{}"

export SSH_SOCK="$TMP_DIR/SSH_AGENT_$SSH_VAULT_VM"

sudo -u user /bin/sh -c "umask 177 && exec socat 'UNIX-LISTEN:$SSH_SOCK,fork' 'EXEC:qrexec-client-vm $SSH_VAULT_VM qubes.SshAgent'" &"#,
        vm_names.server_appvm);

    let desktop_cont = format!(
r#"[Desktop Entry]
Name=Split SSH Socket Startup
Exec={SCRIPT_PATH}
Terminal=false
Type=Application"#);

    let mut sock_path; 
    let mut desktop_path;
    if let Some(dir) = files_dir {
        sock_path = format!(
            "{dir}/split-ssh/socket-script.sh");
        desktop_path = format!(
            "{dir}/split-ssh/split-ssh-socket.desktop");
    } else {
        sock_path = format!(
            "{SALT_FILES_DIR}/split-ssh/socket-script.sh");
        desktop_path = format!(
            "{SALT_FILES_DIR}/split-ssh/split-ssh-socket.desktop");
    } 

    fs::write(&sock_path, sock_script_cont)?;
    fs::write(&desktop_path, desktop_cont)?;

    sock_path = parse_verify_file(
        sock_path, 
        file_roots)?;

    desktop_path = parse_verify_file(
        desktop_path,
        file_roots)?;

    let state_cont = format!(
r#"client-socket-script:
  file.managed:
    - source: {sock_path}
    - name: {SCRIPT_PATH}
    - mode: 744
    - user: {user}
    - group: {user}
    - makedirs: True

client-socket-script-xdg-desktop:
  file.managed:
    - source: {desktop_path}
    - name: {DESKTOP_PATH}
    - mode: 644
    - user: root
    - group: root"#);

    drop(sock_path);

    let state_path;
    if let Some(dir) = states_dir {
        state_path = format!(
            "{dir}/split-ssh/client-files.sls");
    } else {
        state_path = format!(
            "{STATE_DIR}/split-ssh/client-files.sls");
    }

    fs::write(&state_path, state_cont)?;
    
    return Ok(
        parse_verify_state(
            state_path,
            file_roots)?);
}

fn global_bashrc_file(
    server_appvm_name: &str,
    file_dir: &Option<String>,
    state_dir: &Option<String>,
    file_roots: &Vec<String>,
) -> DynRes<String> {
    const AUTH_SOCK_PROFD_PATH: &str = 
        "/etc/profile.d/split-ssh-auth-sock.sh";

    let auth_sock_profd_cont = format!(
r#"#! /bin/bash
#
# Qubes Split SSH - sets the SSH_AUTH_SOCK for the ssh-client
#
SSH_VAULT_VM="{server_appvm_name}"
export SSH_AUTH_SOCK="{TMP_DIR}/SSH_AGENT_$SSH_VAULT_VM""#);

    let mut file_path;
    if let Some(dir) = file_dir {
        file_path = format!(
            "{dir}/split-ssh/split-ssh-auth-sock.sh");
    } else {
        file_path = format!(
            "{SALT_FILES_DIR}/split-ssh/split-ssh-auth-sock.sh");
    }

    fs::write(&file_path, auth_sock_profd_cont)?;
    file_path = parse_verify_file(
        file_path,
        file_roots)?;

    let auth_sock_sls_cont = format!(
r#"global-bashrc-split-ssh-append:
  file.managed:
    - name: {AUTH_SOCK_PROFD_PATH}
    - source: {file_path}
    - user: root
    - group: root
    - mode: 755"#);

    let state_path;
    if let Some(dir) = state_dir {
        state_path = format!(
            "{dir}/split-ssh/auth-sock-profd.sls");
    } else {
        state_path = format!(
            "{STATE_DIR}/split-ssh/auth-sock-profd.sls");
    }

    fs::write(&state_path, auth_sock_sls_cont)?;

    return Ok(
        parse_verify_state(
            state_path,
            file_roots)?);
}
