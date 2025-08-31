pub mod salt;
pub mod make_vm;
pub mod dom0_files; 
pub mod client_files;
pub mod server_files;
pub mod qvm;
pub mod msgs;

use std::{
    error::Error,
    io,
};
use crate::{
    make_vm::{
        gen_vms,
        deps, 
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

    for state in deps(
        &states_dir,
        &vm_names,
        &file_roots)? 
    {
        states.push(state);
    }

    states.push(
        dom0_files::maint_files(
            &vm_names,
            &files_dir,
            &file_roots)?);

    states.push(
        client_files::maint_files(
            &mut stdout,
            &stdin,
            &vm_names,
            &files_dir,
            &states_dir,
            &file_roots)?);

    for state in server_files::maint_files(
        &mut stdout,
        &stdin,
        &vm_names,
        &files_dir,
        &states_dir,
        &file_roots)? 
    {
        states.push(state);
    }

    SlsVmComplement::execute_as_top(
        states,
        &states_dir,
        &file_roots)?;

    return Ok(());
}

#[macro_export]
macro_rules! err {
    ($msg:expr) => {
        #[cfg(debug_assertions)]
        return Err(anyhow!(
            "{}\n\nbacktrace: {}", 
            $msg, 
            std::backtrace::Backtrace::capture(),
        ).into());
        
        #[cfg(not(debug_assertions))]
        return Err(anyhow!(
            "{}", $msg
        ).into());
    } 
}
