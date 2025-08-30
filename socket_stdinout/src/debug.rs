use crate::types::DynError;
use std::{
    fs,
    io::Write,
};

const GET_XDG_DIR_ERR: &str = 
    "Error: debug_append, xdg_dir";
const FS_EXISTS_ERR: &str = 
    "Error: debug_append, failed dir exists";
const CREATE_DIR_ALL_ERR: &str = 
    "Error: debug_append, failed dir creation";
const OO_FOPEN_ERR: &str = 
    "Error: debug_append, failed file open";
const WRITE_ALL_ERR: &str = 
    "Error: debug_append, file write_all";
const WRITE_FMT_ERR: &str = 
    "Error: debug_err_append, write_fmt";

#[macro_export]
macro_rules! err {
    ($err:expr) => {
        match $err {
            Err(e) => return Err(Box::new(e)),
            Ok(thing) => thing,
        }
    };
}

fn get_xdg_state_dir(
    dir_name: impl std::fmt::Display
) -> DynError<String> {
    const XDG_VAR: &str = "XDG_STATE_HOME";     
    const DEFAULT_VAR: &str = "HOME";
    const DEFAULT_POSTFIX: &str = /*$HOME*/".local/state";

    let dir;
    if let Ok(xdg_dir) = std::env::var(XDG_VAR) {
        dir = format!("{}/{}", xdg_dir, dir_name);
    } else {
        dir = format!(
            "{}/{}/{}",
            std::env::var(DEFAULT_VAR)?,
            DEFAULT_POSTFIX,
            dir_name,
        );
    };

    return Ok(dir);
}

#[cfg(debug_assertions)]
pub fn debug_append(
    buf: impl AsRef<[u8]>, 
    fname: impl AsRef<str>,
    dir_name: impl AsRef<str>,
) {

    let dir = get_xdg_state_dir(dir_name.as_ref())
        .expect(GET_XDG_DIR_ERR);

    if !std::fs::exists(&dir).expect(FS_EXISTS_ERR)
    {
        std::fs::create_dir_all(&dir)
            .expect(CREATE_DIR_ALL_ERR);
    }

    let path = format!("{dir}/{}.log", fname.as_ref());
    let mut file = fs::OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(path)
        .expect(OO_FOPEN_ERR);

    let _ = &mut file.write_all(buf.as_ref())
        .expect(WRITE_ALL_ERR);
}

#[cfg(debug_assertions)]
pub fn debug_err_append<'a, T, E: std::fmt::Display>(
    error: &Result<T, E>,
    fname: &str,
    dir_name: &str,
) {
    if let Err(err) = error {
        let dir = get_xdg_state_dir(dir_name)
            .expect(GET_XDG_DIR_ERR);

        let path = format!("{dir}/{}", fname);
        if !fs::exists(&path)
            .expect(FS_EXISTS_ERR)
        {
            fs::create_dir_all(&dir)
                .expect(CREATE_DIR_ALL_ERR);
        }

        let mut file = fs::OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(path)
            .expect(OO_FOPEN_ERR);

        let err_msg = err.to_string();
        let _ = &mut file.write_all(
            err_msg.as_ref()).expect(WRITE_FMT_ERR);
    }
}  
