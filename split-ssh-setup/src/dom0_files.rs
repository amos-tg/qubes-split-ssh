use std::fs;

use crate::{
    DynRes,
    SALT_FILES_DIR,
    parse_verify_file,
    parse_verify_state,
    STATE_DIR,
    SlsVmComplement,
};

use super::make_vm::VmNames;

// generates new and overwrites all existing split-ssh related files for dom0.
pub fn maint_files(
    vm_names: &VmNames,
    files_dir: &Option<String>,
    file_roots: &Vec<String>,
) -> DynRes<SlsVmComplement> {
    const SSH_POLICY_PATH: &str = "/etc/qubes/policy.d/10-ssh.policy";

    let mut content = format!(
        "qubes.SshAgent * {} {} ask default_target={}",
        vm_names.dvm_client,
        vm_names.server_appvm,
        vm_names.server_appvm,
    );

    let mut path;
    if let Some(dir) = files_dir {
        path = format!("{dir}/split-ssh/10-ssh.policy");
        fs::write(&path, content)?;
    } else {
        path = format!("{SALT_FILES_DIR}/split-ssh/10-ssh.policy");
        fs::write(&path, content)?;
    }

    path = parse_verify_file(
        path,
        file_roots,
    )?;

    content = format!(
r#"dom0-split-ssh-policy:
  file.managed:
    - name: {SSH_POLICY_PATH}
    - source: {path}
    - user: root
    - group: root
    - mode: 644"#
    );

    let state_path = format!("{STATE_DIR}/split-ssh/ssh-policy.sls");
    fs::write(&state_path, content)?;

    return Ok(SlsVmComplement { 
        target_vm: String::from("dom0"),
        states: vec![
            parse_verify_state(
                state_path,
                file_roots,
            )?
        ],
    });
} 
