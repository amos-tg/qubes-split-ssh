use std::fs;

use crate::{
    DynRes,
    SALT_FILES_DIR,
    salt::{parse_verify_file, parse_verify_state},
    STATE_DIR,
    TMP_DIR,
    salt::SlsVmComplement,
    make_vm::VmNames,
};

/// generates new and overwrites all existing split-ssh related files for dom0.
pub fn maint_files(
    vm_names: &VmNames,
    files_dir: &Option<String>,
    file_roots: &Vec<String>,
) -> DynRes<SlsVmComplement> {
    const SSH_POLICY_PATH: &str = 
        "/etc/qubes/policy.d/10-split-ssh.policy";

    let mut content = format!(
        "qubes.SshAgent * {} {} ask default_target={}",
        vm_names.dvm_client,
        vm_names.server_appvm,
        vm_names.server_appvm);

    let mut path;
    if let Some(dir) = files_dir {
        path = format!("{dir}/split-ssh/10-split-ssh.policy");
        fs::write(&path, content)?;
    } else {
        path = format!("{SALT_FILES_DIR}/split-ssh/10-split-ssh.policy");
        fs::write(&path, content)?;
    }

    path = parse_verify_file(
        path,
        file_roots)?;

    content = format!(
r#"dom0-split-ssh-policy:
  file.managed:
    - name: {SSH_POLICY_PATH}
    - source: {path}
    - user: root
    - group: root
    - mode: 644"#);

    let state_path = format!(
        "{STATE_DIR}/split-ssh/split-ssh-policy.sls");
    fs::write(&state_path, content)?;

    return Ok(SlsVmComplement { 
        target_vm: String::from("dom0"),
        states: vec![
            parse_verify_state(
                state_path,
                file_roots)?
        ],
    });
} 

/// makes sure that TMP_DIR, file_dir, and state_dir exist in the filesystem.
pub fn init_dirs(
    file_dir: &Option<String>,
    state_dir: &Option<String>,
) -> DynRes<()> {
    const DIR_NAME: &str = "split-ssh";

    fs::create_dir_all(TMP_DIR)?;
    let mut dir;
    if let Some(d) = file_dir {
        dir = d.to_string();
    } else {
        dir = SALT_FILES_DIR.to_string();
    }

    dir = format!("{dir}/{DIR_NAME}");
    fs::create_dir_all(&dir)?;

    if let Some(d) = state_dir {
        dir = d.to_string();
    } else {
        dir = STATE_DIR.to_string();
    }

    dir = format!("{dir}/{DIR_NAME}");
    fs::create_dir_all(&dir)?;

    return Ok(());
}
