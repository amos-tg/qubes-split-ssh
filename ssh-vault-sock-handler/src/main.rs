use std::{
    fs::metadata,
    os::unix::{
        net::UnixStream,
        fs::FileTypeExt,
    },
    fs,
    env, error,
    io,
    io::{
        Stdin,
        Stdout,
        Read,
        Write,
    },
    thread,
    time::Duration,
};

use anyhow::anyhow;
use log::log_err_append;

type DynError<T> = Result<T, Box<dyn error::Error>>;

const SLEEP_TIME: u64 = 80;
const ERR_LOG_DIR_NAME: &str = "ssh-vault-sock-handler";
const SOCK_VAR: &str = "SSH_AUTH_SOCK";

fn main() -> DynError<()> {
    let mut auth_sock = {
        let res = get_auth_sock();
        log_err_append!(&res, ERR_LOG_DIR_NAME);
        res?
    };

    let (mut stdin, mut stdout) = (
        io::stdin(),
        io::stdout(),
    );

    let (mut input, mut output) = (
        Vec::new(), 
        Vec::new(),
    );

    let mut err_count = 0u8;

    loop {
        let res = runtime(
            &mut stdin, 
            &mut stdout,
            &mut auth_sock,
            &mut input,
            &mut output,
        );

        log_err_append!(&res, ERR_LOG_DIR_NAME);

        if res.is_err() {
            err_count += 1;
        } else {
            err_count = 0;
        } 

        if err_count > 5 {
            panic!("Aborting due to consistent failures, errors are logged."); 
        }

        thread::sleep(Duration::from_millis(SLEEP_TIME));
    }
}

fn get_auth_sock() -> DynError<UnixStream> {
    let path = env::var(SOCK_VAR)?;
    let sock = if fs::exists(&path)? {
        let file_type = metadata(&path)?.file_type(); 
        if file_type.is_socket() {
            UnixStream::connect(&path)?
        } else {
            return Err(anyhow!(
                "Error: the file SSH_AUTH_SOCK points to exists, but is not a socket."
            ).into());
        }
    } else {
        return Err(anyhow!(
            "Error: the ssh-agent hasn't produced a socket yet."
        ).into());
    };
    return Ok(sock);
} 

#[inline(always)] 
fn runtime(
    stdin: &mut Stdin,
    stdout: &mut Stdout,
    auth_sock: &mut UnixStream,
    input: &mut Vec<u8>,
    output: &mut Vec<u8>,
) -> DynError<()> {
    let mut buf = [0u8; 8096];
    let mut bytes;

    loop {
        let bytes = stdin.read(&mut bytes)?;  

    #[cfg(debug_assertions)] {
        let path = format!(
            "{}/log.debug",
            log::get_xdg_state_dir(ERR_LOG_DIR_NAME)?,
        );
        fs::write(
            &path,
            format!(
                "Input:\n{}\n\n",
                str::from_utf8(&input)?,
            )
        )?;
    }

    if bytes != 0 {
        auth_sock.write_all(input)?;
    } else {
        return Ok(());
    }

    loop {
        let bytes = auth_sock.read_to_end(output)?;

        #[cfg(debug_assertions)] {
            let path = format!(
                "{}/log.debug",
                log::get_xdg_state_dir(ERR_LOG_DIR_NAME)?,
            );
            fs::write(
                &path,
                format!(
                    "Output:\n{}\n\n",
                    str::from_utf8(&input)?,
                )
            )?;
        }
        
        if bytes != 0 {
            stdout.write_all(output)?;
            break;
        }

        thread::sleep(Duration::from_millis(80));
    }

    dbg!(std::str::from_utf8(output)?);

    input.clear();
    output.clear();
    return Ok(());
} 
