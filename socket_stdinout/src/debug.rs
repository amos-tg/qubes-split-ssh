use crate::types::DynError;
use std::{
    fs,
    io::Write,
};

#[macro_export]
macro_rules! wield_err {
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
    const ALT_VAR: &str = "HOME";
    const ALT_POSTFIX: &str = /*$HOME*/".local/state";

    let dir = if let Ok(dir) = std::env::var(XDG_VAR) {
        dir
    } else {
        format!(
            "{}/{ALT_POSTFIX}/{}",
            std::env::var(ALT_VAR)?,
            dir_name,
        )
    };

    if !std::fs::exists(&dir)? {
        std::fs::create_dir_all(&dir)?;
    }

    return Ok(dir);
}

#[cfg(debug_assertions)]
pub fn debug_append(
    buf: impl AsRef<[u8]>, 
    fname: impl AsRef<str>,
    dir_name: impl AsRef<str>,
) {
    let dir = get_xdg_state_dir(dir_name.as_ref())
        .expect("Error: debug_append, xdg_dir");

    if !std::fs::exists(&dir)
        .expect("Error: debug_append, failed dir exists")
    {
        std::fs::create_dir_all(&dir)
            .expect("Error: debug_append, failed dir creation")
        ;
    }

    let path = format!("{dir}/{}.log", fname.as_ref());
    let mut file = fs::OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(path)
        .expect("Error: debug_append, failed file open")
    ;

    let _ = &mut file.write_all(buf.as_ref())
        .expect("Error: debug_append, file write_all")
    ;
}

#[cfg(debug_assertions)]
pub fn debug_err_append<'a, T, E: std::fmt::Display>(
    error: &Result<T, E>,
    fname: &str,
    dir_name: &str,
) {
    if let Err(_err) = error {
        let dir = get_xdg_state_dir(dir_name)
            .expect("Error: debug_err_append, xdg_dir")
        ;

        let path = format!("{dir}/{}", fname);
        if !fs::exists(&path)
            .expect("Error: debug_err_append, file exists")
        {
            fs::create_dir_all(&dir)
                .expect("Error: debug_err_append, create_dir_all")
            ;
        }

        let mut file = fs::OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(path)
            .expect("Error: debug_err_append, file open")
        ;

        let err_msg = stringify!(err);
        let _ = &mut file.write_all(
            err_msg.as_ref()
        ).expect("Error: debug_err_append, write_fmt");
    }
}  
