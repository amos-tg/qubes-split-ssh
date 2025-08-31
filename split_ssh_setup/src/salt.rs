use std::{
    fs,
    process::{Command, Stdio},
    str::from_utf8,
    collections::HashMap,
};
use crate::{
    DynRes,
    err,
    STATE_DIR,
    msgs::salt::*,
};
use anyhow::anyhow;

/// verifies that a directory is in the salt file_roots and returns it with
/// the proper salt pathing for file formatting. 
/// 
/// example:  
///
/// passed in : /srv/salt/file/configuration-file.txt
///
/// if /srv/salt/file is in the file roots then 
///
/// passed out : salt://file/configuration-file.txt
pub fn parse_verify_file(
    file_path: String,
    file_roots: &Vec<String>,
) -> DynRes<String> {
    let path = verify_roots(&file_path, file_roots)?;
    return Ok(["salt:/", &path].join(""));
}  

/// verifies that a state.sls is in the salt file_roots and returns it 
/// with the proper salt pathing for usage in salt calls example:
/// 
/// passed in : /srv/salt/states/test.sls 
///
/// passed out : states.test
pub fn parse_verify_state(
    file_path: String,
    file_roots: &Vec<String>,
) -> DynRes<String> {
    const FSLASH_PREFIX_ERR: &str = 
        "Error: path does not start with /";
    const STRIP_ERR: &str = 
        "Error: path.strip_suffix(\".sls\"), returned None";

    let path = verify_roots(&file_path, file_roots)?;

    if !path.starts_with('/') {
        err!(FSLASH_PREFIX_ERR);
    }

    let path = path
            .replace('/', ".")
            .strip_suffix(".sls").ok_or(anyhow!(STRIP_ERR))?
            .to_string();

    return Ok(path);
}

pub fn get_file_roots() -> DynRes<Vec<String>> {
    const FILE_ROOTS_ERR: &str = 
        "Error: Failed to pull the file roots";
    
    let file_roots = Command::new("qubesctl")
        .args(["config.get", "file_roots"])
        .output()?;

    if !from_utf8(&file_roots.stderr)?.is_empty() {
        err!(FILE_ROOTS_ERR);
    }

    let file_roots = from_utf8(&file_roots.stdout)?
        .lines()
        .filter(|line| line.trim().starts_with("- /"))
        .map(|line| {
            let delim = line
                .find('-')
                .unwrap()
                +1;

            let tformed = line.split_at(delim).1
                .trim()
                .split('\u{1b}')
                .collect::<Vec<&str>>()[0]
                .to_string();

            return tformed;
        }).collect::<Vec<String>>();

    return Ok(file_roots);
}

/// err's if the file is not in file roots.
/// and if it is, returns the file with the roots removed.
fn verify_roots(
    file_path: &String,
    file_roots: &Vec<String>,
) -> DynRes<String> {
    const FROOT_ERR: &str =  
        "Error: file_path did not start with a file_root";

    for file_root in file_roots {
        if file_path.starts_with(file_root) {
            return Ok(
                file_path
                    .split(file_root)
                    .last()
                    .unwrap()
                    .to_string());
        } 
    }

    err!(FROOT_ERR);
}

pub struct SlsVmComplement {
    pub target_vm: String,
    pub states: Vec<String>, 
}

impl SlsVmComplement {
    pub fn execute_as_top(
        complements: Vec<Self>,
        states_dir: &Option<String>,
        file_roots: &Vec<String>,
    ) -> DynRes<()> {
        const STATES_MISSING_DOM0: &str = 
            "Error: states hashmap didn't have dom0 in it.";
        const TOP_ENABLE_ERR: &str = 
            "Error: failed to enable the top file of \
            aggregated states: stderr: {stderr}";

        let mut collection = HashMap::new();
        let mut file_cont = String::from(
"# Don't edit this file it will be overwritten\nbase:\n"); 

        for set in complements {
            if !collection.contains_key(&set.target_vm) {
                let _ = collection.insert(
                    set.target_vm.clone(), Vec::new());
            }   

            if let Some(col_mut) = collection
                .get_mut(&set.target_vm) {
                for state in set.states {
                    col_mut.push(state);
                }
            } 
        }

        for set in collection.iter() {
            file_cont.push_str(
                &format!(
                    "  '{}':\n",
                    set.0)); 

            for state in set.1 {
                file_cont.push_str(
                    &format!( 
                        "    - {state}\n"));
            }
        } 

        let mut top_path;
        if let Some(dir) = states_dir {
            top_path = format!("{dir}/split-ssh/split-ssh.top");
        } else {
            top_path = format!("{STATE_DIR}/split-ssh/split-ssh.top");
        }

        fs::write(&top_path, file_cont)?;
        top_path = parse_verify_state(top_path, file_roots)?
            .split(".top")
            .collect::<Vec<&str>>()[0]
            .to_string();

        let out = Command::new("qubesctl")
            .args([
                "top.enable",
                &top_path,
            ])
            .output()?;

        let stderr = from_utf8(&out.stderr)?;
        if !stderr.is_empty() {
            err!(
                format!(
                    "{}:\n {}",
                    TOP_ENABLE_ERR,
                    stderr));
        }

        println!("{}", SALT_EXEC_MSG);

        let _ = collection.remove("dom0").ok_or(
            anyhow!(STATES_MISSING_DOM0))?;

        Command::new("qubesctl")
            .arg("state.apply")
            .stdout(Stdio::inherit())
            .output()?;

        for key in collection.into_keys() {
            Command::new("qubesctl")
                .args([
                    "--target",
                    &key,
                    "--skip-dom0",
                    "state.apply",
                ])
                .stdout(Stdio::inherit())
                .output()?;
        }

        return Ok(());
    }
}
