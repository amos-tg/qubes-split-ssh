use crate::DynRes;
use std::io::{Stdout, Stdin, Write};

pub fn pull_stdout(
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

pub fn get_custom_dir(
    mut stdout: &mut Stdout,
    stdin: &Stdin,
    msg: impl std::fmt::Display,
) -> DynRes<Option<String>> {
    let custom_dir = pull_stdout(
        &mut stdout,
        &stdin,
        &msg,
    )?;

    if custom_dir == "" {
        return Ok(None);
    }

    match pull_stdout(
        &mut stdout,
        &stdin,
        format!(
            "\n\nPath: {custom_dir}\n\nConfirm this is \\
            the intended directory (and that there is no trailing /) y/n: "),
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

pub mod main {
    pub const STATES_DIR_MSG: &str = 
"\n\nDo you want to use a custom directory to store your VM Salt states (it needs to be inside file_roots with no trailing /)?\n\nLeave blank for no otherwise insert the absolute path; If you are unsure leave this blank: ";
    pub const FILES_DIR_MSG: &str = 
"\n\nDo you want to use a custom directory to store your files managed by salt (it needs to be inside file_roots with no trailing /)?\n\n Leave blank for no otherwise insert the absolute path; If you are unsure leave this blank: ";
    pub const PROTO_QUERY: &str =
"Do you want to use the experimental rust split-ssh protocol or the time tested socat protocol?: rust/socat";
    pub const INVALID_RESPONSE: &str = 
        "not a valid response try again...";
} 

pub mod make_vm {
    pub const VAULT_PKG_NAME: &str = "vault_handler";
    pub const CLIENT_PKG_NAME: &str = "CLIENT_HANDLER";
    pub const C_T_NAME_PROMPT: &str = 
        "[Input the SSH Client Template Name]: ";
    pub const DVM_C_T_NAME_PROMPT: &str = 
        "\n[Input Name for SSH Client Disposable Template]: ";
    pub const DVM_C_NAME_PROMPT: &str = 
        "\n[Input Name for Disposable SSH Client VM]: ";
    pub const S_T_NAME_PROMPT: &str = 
         "\n[Input Server Template VM Name]: ";
    pub const S_T_SOURCE_PROMPT: &str =
        "\n[Input the TemplateVM for the SSH Server Template]: ";
    pub const S_A_NAME_PROMPT: &str = 
         "\n[Input Server AppVM Name]: ";
    pub const BUILD_VM_QUERY: &str = 
        "\n[Input pre-existing BuildVM Name for qubes-split-ssh]: ";
    pub const QSS_SRC_QUERY: &str =
        "\n[Input Absolute Path of Qubes Split SSH Source Directory]\n \
            (On the BuildVM): ";
}

pub mod salt {
    pub const SALT_EXEC_MSG: &str =  
"Salt is executing state.apply on all templates and it's going to take a long time; I would do something else in the meantime.\n\n If you have anything that might be erased or mangled by state.apply like changed etc config for your templates not saved in your current salt states I would reccomend exiting the program.";
}
