use std::{
    fs,
    io::{Stdin, Stdout},
};

use crate::{
    DynRes,
    make_vm::VmNames,
    qvm::{
        assure_qrexec,
        get_user,
        shutdown_vm,
    },
    salt::{
        parse_verify_state,
        parse_verify_file,
        SlsVmComplement,
    },
    SALT_FILES_DIR,
    STATE_DIR,
};

pub fn maint_files(
    mut stdout: &mut Stdout,
    stdin: &Stdin,
    vm_names: &VmNames,
    files_dir: &Option<String>,
    states_dir: &Option<String>,
    file_roots: &Vec<String>,
) -> DynRes<[SlsVmComplement; 2]> {
    assure_qrexec(&vm_names.server_appvm.name)?;
    let user = get_user(
        &mut stdout,
        &stdin,
        &vm_names.server_appvm.name)?;
    shutdown_vm(&vm_names.server_appvm.name)?;

    let saf = ssh_add_file(
        &vm_names,
        &user,
        files_dir,
        states_dir,
        file_roots)?;

    let agf = agent_script_file(
        files_dir,
        &vm_names,
        &user,
        states_dir,
        file_roots)?;
    
    return Ok([saf, agf]);
}

fn ssh_add_file(
    vm_names: &VmNames,
    user: &str,
    files_dir: &Option<String>,
    states_dir: &Option<String>,
    file_roots: &Vec<String>,
) -> DynRes<SlsVmComplement> {
    let key_desktop_path = format!(
        "/home/{user}/.config/autostart/ssh-add.desktop");

    let key_adder_desktop_cont = format!(
r#"[Desktop Entry]
Name=ssh-add
Exec=ssh-add
Terminal=true
Type=Application"#);

    let mut key_adder_desktop_path; 
    if let Some(dir) = files_dir {
        key_adder_desktop_path = format!(
            "{dir}/split-ssh/key-adder-vault-desktop");

        fs::write(
            &key_adder_desktop_path, key_adder_desktop_cont)?;
    } else {
        key_adder_desktop_path = format!(
            "{SALT_FILES_DIR}/split-ssh/key-adder-vault-desktop");

        fs::write(
            &key_adder_desktop_path, key_adder_desktop_cont)?;
    }

    key_adder_desktop_path = parse_verify_file(
        key_adder_desktop_path,
        file_roots)?;

    let key_adder_desktop_sls_cont = format!(
r#"key-adder-vault-desktop:
  file.managed:
    - name: {key_desktop_path}
    - source: {key_adder_desktop_path}
    - mode: 755
    - group: {user}
    - user: {user}
    - makedirs: True"#);

    let key_adder_desktop_sls_path;
    if let Some(dir) = states_dir {
        key_adder_desktop_sls_path = format!(
            "{dir}/split-ssh/key-adder-vault-desktop.sls");
    } else {
        key_adder_desktop_sls_path = format!(
            "{STATE_DIR}/split-ssh/key-adder-vault-service.sls");
    }

    fs::write(
        &key_adder_desktop_sls_path, key_adder_desktop_sls_cont)?;

    return Ok(SlsVmComplement {
        target_vm: vm_names.server_appvm.name.clone(),
        states: vec![parse_verify_state(
            key_adder_desktop_sls_path,
            file_roots)?],
    });
} 

fn agent_script_file(
    files_dir: &Option<String>,
    vm_names: &VmNames,
    user: &str,
    states_dir: &Option<String>,
    file_roots: &Vec<String>,
) -> DynRes<SlsVmComplement> {
    const SCRIPT_PATH: &str = 
        "/etc/qubes-rpc/qubes.SshAgent";

    let script_content = 
r#"#!/bin/sh
# Qubes App Split SSH Script
# safeguard - Qubes Notification bubble for each ssh request
notify-send "[$(qubesdb-read /name)] SSH agent access from: $QREXEC_REMOTE_DOMAIN"

# ssh connection
socat - "UNIX-CONNECT:$SSH_AUTH_SOCK""#;

    let mut file_path; 
    if let Some(dir) = files_dir {
        file_path = format!("{dir}/split-ssh/qubes.SshAgent");
        fs::write(&file_path, script_content)?;
    } else {
        file_path = format!(
            "{SALT_FILES_DIR}/split-ssh/qubes.SshAgent");
        fs::write(&file_path, script_content)?;
    }

    file_path = parse_verify_file(
        file_path, 
        file_roots)?;

    let sls_content = format!(
r#"vault-qubes-SshAgent-script:
  file.managed:
    - name: {SCRIPT_PATH}
    - source: {file_path}
    - mode: 744
    - group: {user}
    - user: {user}"#);

    let sls_path;
    if let Some(dir) = states_dir {
        sls_path = format!(
            "{dir}/split-ssh/qubes-SshAgent-vault.sls");
        fs::write(&sls_path, sls_content)?;
    } else {
        sls_path = format!(
            "{STATE_DIR}/split-ssh/qubes-SshAgent-vault.sls");
        fs::write(&sls_path, sls_content)?;
    }

    return Ok(SlsVmComplement {
        target_vm: vm_names.server_template.name.to_string(),
        states: vec![parse_verify_state(
            sls_path,
            file_roots)?],
    });
}

