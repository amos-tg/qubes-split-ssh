pub mod make_vm;
pub mod dom0_files; 
pub mod client_files;
pub mod server_files;
pub mod hefty_misc;

use std::{
    collections::HashMap, fs,
    error::Error,
    io,
    io::{Write, Stdout, Stdin},
    process::{Command, Stdio},
    str::from_utf8,
};

use crate::make_vm::{
    gen_vms,
    deps, 
};

use anyhow::anyhow;

pub type DynRes<T> = Result<T, Box<dyn Error>>;

const TMP_DIR: &str = "/tmp/split-ssh-1820912";
const STATE_DIR: &str = "/srv/salt";
const SALT_FILES_DIR: &str = "/srv/salt/files";

/// since all the files are going in the same dir,
/// at the end of setup add all to a top file then
/// execute at once after top.enable'ment.
fn main() -> DynRes<()> {
    let stdin = io::stdin(); 
    let mut stdout = io::stdout();

    let file_roots = get_file_roots()?; 

    let states_dir = get_custom_dir(
        &mut stdout,
        &stdin,
        "\n\nDo you want to use a custom directory to store your VM Salt states (it needs to be inside file_roots with no trailing /)?\n\nLeave blank for no otherwise insert the absolute path; If you are unsure leave this blank: ",
    )?;

    let files_dir = get_custom_dir(
        &mut stdout,
        &stdin,
        "\n\nDo you want to use a custom directory to store your files managed by salt (it needs to be inside file_roots with no trailing /)?\n\n Leave blank for no otherwise insert the absolute path; If you are unsure leave this blank: ",
    )?;

    init_dirs(
       &files_dir, 
       &states_dir
    )?;

    let vm_names = gen_vms(
        &stdin,
        &mut stdout,
    )?;

    let mut states = Vec::new();

    for state in deps(
        &states_dir,
        &vm_names,
        &file_roots
    )? {
        states.push(state);
    }

    states.push(dom0_files::maint_files(
        &vm_names,
        &files_dir,
        &file_roots,
    )?);

    states.push(client_files::maint_files(
        &mut stdout,
        &stdin,
        &vm_names,
        &files_dir,
        &states_dir,
        &file_roots,
    )?);

    for state in server_files::maint_files(
        &mut stdout,
        &stdin,
        &vm_names,
        &files_dir,
        &states_dir,
        &file_roots,
    )? {
        states.push(state);
    }

    SlsVmComplement::execute_as_top(
        states,
        &states_dir,
        &file_roots,
    )?;

    return Ok(());
}

fn pull_stdout(
    stdout: &mut Stdout,
    stdin: &Stdin,
    msg: impl std::fmt::Display,
) -> DynRes<String> {
    print!("{msg}");
    stdout.flush()?;

    let mut client_name = String::new(); 
    stdin.read_line(&mut client_name)?;

    return Ok(client_name
        .trim()
        .to_string()
    );
}

fn get_custom_dir(
    mut stdout: &mut Stdout,
    stdin: &Stdin,
    msg: impl std::fmt::Display,
) -> DynRes<Option<String>> {
    let custom_dir = dbg!(pull_stdout(
        &mut stdout,
        &stdin,
        &msg,
    )?);

    if custom_dir == "" {
        return Ok(None);
    }

    match pull_stdout(
        &mut stdout,
        &stdin,
        format!("\n\nPath: {custom_dir}\n\nConfirm this is the intended directory (and that there is no trailing /) y/n: "),
    )?.as_str() {
        "y" => return Ok(Some(custom_dir)),
        "Y" => return Ok(Some(custom_dir)),
        "yes" => return Ok(Some(custom_dir)), 
        "Yes" => return Ok(Some(custom_dir)),
        "n" => return get_custom_dir(&mut stdout, &stdin, msg),
        "N" => return get_custom_dir(&mut stdout, &stdin, msg),
        "no" => return get_custom_dir(&mut stdout, &stdin, msg),
        "No" => return get_custom_dir(&mut stdout, &stdin, msg),
        _ => {
            println!("\n\ninvalid response, try again.\n\n");
            return get_custom_dir(&mut stdout, &stdin, msg);
        },
    }
}

/// verifies that a directory is in the salt file_roots and returns it with
/// the proper salt pathing for file formatting example:  
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
    return Ok(dbg!(["salt:/", &path].join("")));
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
    let path = verify_roots(&file_path, file_roots)?;

    if path.find('/').ok_or(anyhow!(
        "Error: this should start with a / and it doesn't."
    ))? != 0 { 
        err!("Error: this should start with a / and it doesn't.");
    } 

    return Ok(dbg!(path[1..]
            .replace("/", ".")
            .split(".sls")
            .next()
            .ok_or(anyhow!(
"Error: file_path {file_path}, returned None from split(\".sls\").next()"
            ))?
            .to_string()
    ));
}

fn get_file_roots() -> DynRes<Vec<String>> {
    let file_roots = Command::new("qubesctl")
        .args([
            "config.get",
            "file_roots",
        ])
        .output()?
    ;

    if !from_utf8(&file_roots.stderr)?.is_empty() {
        err!("Error: Failed to pull the file roots");
    }

    let file_roots = from_utf8(&file_roots.stdout)?
        .lines()
        .filter(|line| line.contains("- /"))
        .map(|line| line
            .split_at(
            line
                .find('-')
                .unwrap()
                +1
            )
            .1
            .trim()
            .split('\u{1b}')
            .collect::<Vec<&str>>()[0]
            .to_string()
        )
        .collect::<Vec<String>>()
    ;

    return Ok(dbg!(file_roots));
}

/// err's if the file is not in file roots.
/// and if it is, returns the file with the roots removed.
fn verify_roots(
    file_path: &String,
    file_roots: &Vec<String>,
) -> DynRes<String> {
    for file_root in file_roots {
        if file_path.contains(file_root) {
            return Ok(file_path
                .split(file_root)
                .last()
                .ok_or(anyhow!(
                    "Error: file_path: {file_path} was unsplittable at {file_root}"
                ))?
                .to_string()
            );
        } 
    }

    err!(format!("Error file_path: {file_path} was not in the salt file_roots!"));
}

pub fn init_dirs(
    file_dir: &Option<String>,
    state_dir: &Option<String>,
) -> DynRes<()> {
    if !fs::exists(TMP_DIR)? {
        fs::create_dir(TMP_DIR)?;
    }

    let mut storage_dir;
    if let Some(dir) = file_dir {
        if !fs::exists(dir)? {
            fs::create_dir(dir)?;
        }
        storage_dir = dir.to_string();
    } else {
        if !fs::exists(SALT_FILES_DIR)? {
            fs::create_dir(SALT_FILES_DIR)?;
        }
        storage_dir = SALT_FILES_DIR.to_string();
    }

    let file_storage_dir = format!("{storage_dir}/split-ssh");
    if !fs::exists(&file_storage_dir)? {
        fs::create_dir(&file_storage_dir)?;
    }

    if let Some(dir) = state_dir {
        if !fs::exists(dir)? {
            fs::create_dir(dir)?;
        }
        storage_dir = dir.to_string();
    } else {
        if !fs::exists(STATE_DIR)? {
            fs::create_dir(STATE_DIR)?;
        }
        storage_dir = STATE_DIR.to_string();
    }

    let file_storage_dir = format!("{storage_dir}/split-ssh");
    if !fs::exists(&file_storage_dir)? {
        fs::create_dir(&file_storage_dir)?;
    }

    return Ok(());
}

#[macro_export]
macro_rules! err {
    ($msg:expr) => {
        #[cfg(debug_assertions)]
        return Err(anyhow!(
            "{}, backtrace: {}", 
            $msg, 
            std::backtrace::Backtrace::capture(),
        ).into());
        
        #[cfg(not(debug_assertions))]
        return Err(anyhow!(
            "{}", $msg
        ).into());
    } 
}

pub struct SlsVmComplement {
    pub target_vm: String,
    pub states: Vec<String>, 
}

impl SlsVmComplement {
    fn execute_as_top(
        complements: Vec<Self>,
        states_dir: &Option<String>,
        file_roots: &Vec<String>,
    ) -> DynRes<()> {
        let mut file_cont = String::from("# Don't edit this file it will be overwritten\nbase:\n"); 
        let mut collection = HashMap::new();

        for set in complements {
            if !collection.contains_key(&set.target_vm) {
                let _ = collection.insert(set.target_vm.clone(), Vec::new());
            }   

            if let Some(col_mut) = collection.get_mut(&set.target_vm) {
                for state in set.states {
                    col_mut.push(state);
                }
            } 
        }

        for set in collection.iter() {
            file_cont.push_str(&format!(
                "  '{}':\n",
                set.0,
            )); 

            for state in set.1 {
                file_cont.push_str(&format!( 
                    "    - {state}\n",
                ));
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
            .to_string()
        ;

        let out = Command::new("qubesctl")
            .args([
                "top.enable",
                &top_path,
            ])
            .output()?
        ;

        let stderr = from_utf8(&out.stderr)?;
        if !stderr.is_empty() {
            err!(format!(
                "Error: failed to enable the top file of aggregated states: stderr: {stderr}",
            ));
        }

        println!("Salt is executing state.apply on all templates and it's going to take a long time; I would do something else in the meantime.\n\n If you have anything that might be erased or mangled by state.apply like changed etc config for your templates not saved in your current salt states I would reccomend exiting the program.");
        // --targets never works consistently for me.
        // if someone wants to drop a working pr I will 
        // gladly test, merge, and make this cmd faster.
        //
        // feature: ask for concurrency num or add it to args or cfg file
        // I always run 1 because I have 16gb ram but others probably 
        // don't have this issue.
        let _ = collection.remove("dom0")
            .ok_or(anyhow!(
                "Error: states hashmap didn't have dom0 in it." 
            ))?
        ;

        Command::new("qubesctl")
            .arg("state.apply")
            .stdout(Stdio::inherit())
            .output()?
        ;

        for key in collection.into_keys() {
            Command::new("qubesctl")
                .args([
                    "--target",
                    &key,
                    "--skip-dom0",
                    "state.apply",
                ])
                .stdout(Stdio::inherit())
                .output()?
            ;
        }

        return Ok(());
    }
}
