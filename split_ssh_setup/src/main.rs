pub mod salt;
pub mod make_vm;
pub mod dom0_files; 
pub mod client_files;
pub mod server_files;
pub mod qvm;
pub mod msgs;

use std::{
    error::Error,
    io::{self, Write},
};
use crate::{
    make_vm::{
        gen_vms,
        deps, 
        VmNames,
        Binaries,
    },
    msgs::{*, main::*},
    salt::*,
    dom0_files::*,
};

pub type DynRes<T> = Result<T, Box<dyn Error>>;

const TMP_DIR: &str = "/tmp/split-ssh-1820912"; 
const STATE_DIR: &str = "/srv/salt";
const SALT_FILES_DIR: &str = "/srv/salt/files";

fn main() -> DynRes<()> {
    let stdin = io::stdin(); 
    let mut stdout = io::stdout();

    let file_roots = get_file_roots()?; 
    let states_dir = get_custom_dir(
        &mut stdout,
        &stdin,
        STATES_DIR_MSG)?;

    let files_dir = get_custom_dir(
        &mut stdout,
        &stdin,
        FILES_DIR_MSG)?;

    init_dirs(
       &files_dir, 
       &states_dir)?;

    let vm_names = gen_vms(
        &stdin,
        &mut stdout)?;

    let mut states = Vec::new();

    choose_proto(
        &states_dir,
        &files_dir,
        &vm_names,
        &file_roots,
        &mut states,
        &mut stdout,
        &stdin)?;

    SlsVmComplement::execute_as_top(
        states,
        &states_dir,
        &file_roots)?;

    return Ok(());
}

fn choose_proto(
    states_dir: &Option<String>,
    files_dir: &Option<String>,
    vm_names: &VmNames,
    file_roots: &Vec<String>,
    states: &mut Vec<SlsVmComplement>,
    stdout: &mut io::Stdout,
    stdin: &io::Stdin,
) -> DynRes<()> { 
    match pull_stdout(
        stdout,
        stdin,
        PROTO_QUERY)? 
        .as_str()
    {
        "rust" => rust_p_main(
            states_dir, 
            files_dir,
            vm_names,
            file_roots,
            states,
            stdout,
            stdin),

        "socat" => socat_p_main(
            states_dir, 
            files_dir,
            vm_names,
            file_roots,
            states,
            stdout,
            stdin),

        _ => {
            print!("{}", INVALID_RESPONSE);
            stdout.flush()?;
            choose_proto(
                states_dir, 
                files_dir,
                vm_names,
                file_roots,
                states,
                stdout,
                stdin)
        }
    }
}

fn socat_p_main(
    states_dir: &Option<String>,
    files_dir: &Option<String>,
    vm_names: &VmNames,
    file_roots: &Vec<String>,
    states: &mut Vec<SlsVmComplement>,
    stdout: &mut io::Stdout,
    stdin: &io::Stdin,
) -> DynRes<()> {
    for state in deps(
        states_dir,
        vm_names,
        file_roots)? 
    {
        states.push(state);
    }

    states.push(
        dom0_files::maint_files(
            vm_names,
            files_dir,
            file_roots)?);

    states.push(
        client_files::maint_files_socat(
            stdout,
            stdin,
            vm_names,
            files_dir,
            states_dir,
            file_roots)?);

    for state in server_files::maint_files(
        stdout,
        stdin,
        vm_names,
        files_dir,
        states_dir,
        file_roots)? 
    {
        states.push(state);
    }

    return Ok(());
}

/* things to do
 *
 * server side:
     * ssh-agent with a specific path using xdg-autostart
     * user dir; hardcode the path in /etc/environment 
     *
     * move the binary into /usr/bin and symlink with
     * /etc/qubes-rpc
     *
     * with the proper hardcoded service name and perms
     
 * client side:
     * Move the binary into usr/bin and setup xdg-autostart
     * with desktop file, 
     *
     * ensure SSH_AUTH_SOCK and SSH_VAULT_VM are hardcoded
     * somewhere using the VmNames value, 
     *
     * ensure the SSH_AUTH_SOCK value is in a path with 
     * directories setup. 
     
*/
fn rust_p_main(
    states_dir: &Option<String>,
    files_dir: &Option<String>,
    vm_names: &VmNames,
    file_roots: &Vec<String>,
    states: &mut Vec<SlsVmComplement>,
    stdout: &mut io::Stdout,
    stdin: &io::Stdin,
) -> DynRes<()> {
    let bins = Binaries::build(
        stdout,
        stdin,
        vm_names)?;

    states.push(
        dom0_files::maint_files(
            vm_names, 
            files_dir,
            file_roots)?);
    
    states.push(
        client_files::maint_files_rust(
            &bins, vm_names, stdout, stdin)?);

    states.push(
        server_files::maint_files_rust()?);

    return Ok(());
}

#[macro_export]
macro_rules! err {
    ($msg:expr) => {
        return Err(anyhow!(
            "{}", $msg
        ).into());
    } 
}

