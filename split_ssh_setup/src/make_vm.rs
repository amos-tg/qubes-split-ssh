use std::{
    fs,
    io::{Stdin, Stdout},
    process::Command,
    str::from_utf8,
};

use anyhow::anyhow;
use crate::{
    err,
    DynRes,
    STATE_DIR,
    salt::SlsVmComplement,
    salt::parse_verify_state,
    msgs::make_vm::*,
};
use crate::msgs::pull_stdout;

pub fn gen_vms(
    stdin: &Stdin,
    mut stdout: &mut Stdout,
) -> DynRes<VmNames> {
    const SKIP_MSG: &str = 
        "[Skipping VM Configuration & Creation for Existing Client Template]";
    const QVM_C_T_ERR: &str =
        "Error: failed qvm-clone for client-template";
    const QVM_C_DVM_T_ERR: &str = 
        "Error: failed dvm-client-template creation";
    const QVM_C_DVM_ERR: &str = 
        "Error: failed to make client_dvm";
    const QVM_S_T_ERR: &str = 
        "Error: failed server-template creation";
    const QVM_S_A_ERR: &str = 
        "Error: failed server-appvm creation";

    println!("[VM Creation]:\n");

    let client_template_name = Ident::exists(
        pull_stdout(
            &mut stdout,
            &stdin,  
            C_T_NAME_PROMPT)?)?;

    if let Ident::New(name) = &client_template_name { 
        let source_template = pull_stdout(
            &mut stdout,
            &stdin,
            format!("\n[Input Source Template for {}]: ", &name))?;

        let client_template_clone_out = Command::new("qvm-clone")
            .args([
                "-C", 
                "TemplateVM",
                &source_template,
                &name,
            ]) .output()?;

        let stderr = from_utf8(&client_template_clone_out.stderr)?;
        if stderr.contains("qvm-clone: error:") {
            err!(QVM_C_T_ERR);
        }

        /* for some reason qvm-prefs doesn't allow setting more than 
           one attribute at a time so...  */ 
        qvm_prefs(&[&name, "netvm", "none"])?;
        qvm_prefs(&[&name, "label", "black"])?;
    } else {
        println!("\n{SKIP_MSG}");
    }

    let dvm_client_template_name = Ident::exists(
        pull_stdout(
            &mut stdout,
            &stdin,
            DVM_C_T_NAME_PROMPT)?)?;

    if let Ident::New(name) = &dvm_client_template_name {
        let dvm_client_template_create_out = Command::new("qvm-create")
            .args([
                "-C",
                "AppVM",
                "--prop=netvm=",
                "--prop=template_for_dispvms=true",
                "--prop=label=black",  
                "-t", 
                client_template_name.as_ref(),
                &name,
            ]).output()?;

        let stderr = from_utf8(
            &dvm_client_template_create_out.stderr)?;

        if !stderr.is_empty() {
            err!(QVM_C_DVM_T_ERR);
        }
    } else {
        println!("\n{SKIP_MSG}");
    } 

    let client_dvm_name = Ident::exists(pull_stdout(
        &mut stdout,
        &stdin,
        DVM_C_NAME_PROMPT)?)?;

    if let Ident::New(name) = &client_dvm_name {
        let dvm_client_create_out = Command::new("qvm-create")
            .args([
                "-C", 
                "DispVM",
                "-t", 
                &dvm_client_template_name.as_ref(),
                "--prop=label=red",
                &name]).output()?; 

        let stderr = from_utf8(&dvm_client_create_out.stderr)?;
        if !stderr.is_empty() {
            err!(QVM_C_DVM_ERR);
        }
    } else {
        println!("\n{SKIP_MSG}");
    }

    drop(dvm_client_template_name);

    let server_template_name = Ident::exists(
        pull_stdout(
            &mut stdout,
            &stdin,
            S_T_NAME_PROMPT)?)?;

    if let Ident::New(name) = &server_template_name {
        let template = Ident::exists(
            pull_stdout(
                &mut stdout,
                &stdin,
                S_T_SOURCE_PROMPT)?)?;

        let server_template_clone_out = Command::new("qvm-clone")
            .args([
                "-C", 
                "TemplateVM",
                template.as_ref(),
                &name,
            ]).output()?; 

        let stderr = from_utf8(&server_template_clone_out.stderr)?;
        if stderr.contains("qvm-clone: error:") {
            err!(QVM_S_T_ERR);
        }

        qvm_prefs(&[&name, "label",  "black"])?;
        qvm_prefs(&[&name, "netvm",  "none"])?;
    } else {
        println!("\n{SKIP_MSG}");
    }

    let server_appvm_name = Ident::exists(
        pull_stdout(
            &mut stdout,
            &stdin,
            S_A_NAME_PROMPT)?)?;

    if let Ident::New(name) = &server_appvm_name {
        let server_appvm_create_out = Command::new("qvm-create")
            .args([
                "-C",  
                "AppVM",
                "--prop=netvm=",
                "--prop=label=gray",
                "-t", 
                server_template_name.as_ref(), 
                &name,
            ]).output()?;

        let stderr = from_utf8(&server_appvm_create_out.stderr)?;
        if !stderr.is_empty() {
            err!(QVM_S_A_ERR);
        }
    } else {
        println!("\n{SKIP_MSG}");
    }

    return Ok(VmNames {
        client_template: client_template_name.into(),
        dvm_client: client_dvm_name.into(),
        server_template: server_template_name.into(),
        server_appvm: server_appvm_name.into(),
    });
}

fn qvm_prefs(
    args: &[&str],
) -> DynRes<()> {
    const QVM_PREFS_ERR: &str = 
        "Error: failed qvm-prefs";

    let prefs_out = Command::new("qvm-prefs")
        .args(args)
        .output()?
    ;

    let stderr = from_utf8(&prefs_out.stderr)?;
    if !stderr.is_empty() {
        err!(QVM_PREFS_ERR);
    }

    return Ok(());
}

#[derive(Debug)]
enum Ident {
    Existing(String),
    New(String),
}

impl Ident {
    fn exists(vm: String) -> DynRes<Self> {
        let out = Command::new("qvm-ls")
            .args([
                "--raw-list",
                &vm,
            ]).output()?;

        let (stdout, stderr) = (
            from_utf8(&out.stdout)?,
            from_utf8(&out.stderr)?);

        if stdout.contains(&vm) &&
            !stderr.contains("no such domain:") {
            return Ok(Ident::Existing(vm));
        } else {
            return Ok(Ident::New(vm));
        } 
    }
}

impl std::convert::Into<String> for Ident {
    fn into(self) -> String {
        match self {
            Self::Existing(x) => x,
            Self::New(x) => x,
        }
    }
} 

impl std::convert::AsRef<str> for Ident {
    fn as_ref(&self) -> &str {
        match self {
            Self::Existing(x) => &x,
            Self::New(x) => &x,
        }
    }
}

pub struct VmNames {
    pub client_template: String,
    pub dvm_client: String,
    pub server_template: String,
    pub server_appvm: String,
}


/// returns the absolute path of the
/// generated files so they can
/// be added to a top file.
pub fn deps(
    custom_dir: &Option<String>,
    vm_names: &VmNames,
    file_roots: &Vec<String>,
) -> DynRes<[SlsVmComplement; 2]> {
    const CLIENT_SLS: &str = 
r#"{% set os = salt['grains.get']('os') %}
client_deps: 
  pkg.installed: 
    - pkgs: 
    {% if os == "Fedora" %}
      - openssh 
      - socat
    {% elif os == "Debian" %}
      - openssh-client
      - socat
    {% endif %}"#;

    const SERVER_SLS: &str = 
r#"{% set os = salt['grains.get']('os') %}
server_deps:
  pkg.installed:
    - pkgs:
    {% if os == "Fedora" %}
      - openssh
      - socat
      - openssh-askpass
      - libnotify
    {% elif os == "Debian" %}
      - openssh-client
      - socat
      - ssh-askpass
      - libnotify-bin
    {% endif %}"#;

    let state_dir;
    if let Some(dir) = custom_dir {
        state_dir = format!("{dir}/split-ssh");
    } else {
        state_dir = format!("{STATE_DIR}/split-ssh");
    }

    let client_path = format!("{state_dir}/ssh-client-deps.sls"); 
    let vault_path = format!("{state_dir}/ssh-vault-deps.sls");

    fs::write(&client_path, CLIENT_SLS)?;
    fs::write(&vault_path, SERVER_SLS)?;

    return Ok([
        SlsVmComplement {
            target_vm: vm_names.client_template.to_string(),
            states: vec![
                parse_verify_state(client_path, file_roots)?
            ],
        },
        SlsVmComplement {
            target_vm: vm_names.server_template.to_string(),
            states: vec![
                parse_verify_state(vault_path, file_roots)?
            ],
        },
    ]);
}
