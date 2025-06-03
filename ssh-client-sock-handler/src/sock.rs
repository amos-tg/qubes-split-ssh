mod msg_header;

use msg_header::{
    MsgHeader,
    HEADER_LEN,
};

use crate::{
    DynError,
    DynFutError,
    ERR_LOG_DIR_NAME,
};

use std::{
    env,
    path,
    time::Duration,
    ops::{Deref, DerefMut},
    io::ErrorKind::WouldBlock,
    sync::{
        atomic::{AtomicBool, Ordering::*},
        Arc,
    },
};

use tokio::{
    task,
    net::{
        UnixListener,
        UnixStream, 
        unix::{OwnedReadHalf, OwnedWriteHalf},
    },
    io::{
        AsyncReadExt,
        AsyncWriteExt,
    },
    time,
    fs,
    sync::Mutex,
    process::{ChildStdin, ChildStdout},
};

use anyhow::anyhow;

const MAX_TIMEOUT: u16 = 1200; 
const SLEEP_TIME_MILLIS: u64 = 100; 
const WRITABLE: bool = true; 
const UNWRITABLE: bool = false;

async fn debug_buf_to_file(buf: &Vec<u8>) -> DynFutError<()> {
    let dir = log::get_xdg_state_dir(ERR_LOG_DIR_NAME).expect(
        "Debug fn debug_buf_to_file failed."
    );

    let path = format!(
        "{}/log.debug",
        dir, 
    );
    
    let contents = match str::from_utf8(buf) {
        Err(e) => return Err(Box::new(e)),
        Ok(cont) => cont,
    };

    if let Err(e) = fs::write(&path, contents).await { return Err(Box::new(e)); }

    return Ok(());
}

/// forwards socket information from local VM 
/// socket to remote VM socket over qrexec-client-vm, 
/// Xen vchan when enabled by dom0 RPC policy.
type TaskHandle = task::JoinHandle<Result<(), Box<dyn std::error::Error + Send>>>;
pub struct InterVMSocketCon([TaskHandle; 4]);

impl InterVMSocketCon {
    pub async fn handler(
        stream: UnixStream,
        initial_write_flag_state: bool,
        qrexec_stdin: Arc<Mutex<ChildStdin>>,
        qrexec_stdout: Arc<Mutex<ChildStdout>>,
    ) -> DynFutError<()> {
        let write_flag = Arc::new(
            AtomicBool::new(initial_write_flag_state)
        );

        let (read_half, write_half) = stream.into_split();

        let (read_half, write_half) = (
            Arc::new(read_half),
            Arc::new(Mutex::new(write_half)),
        );

        let sockr_fdw_buf = Arc::new(Mutex::new(Vec::new()));

        let sockw_fdr_buf = Arc::new(
            Mutex::new(
                Vec::new()
            )
        );

        let sock_reader = {
            let sock_reader = SockReader {
                read: read_half.clone(),
                buf: sockr_fdw_buf.clone(),
                write_flag: write_flag.clone(),
            }.new();
            task::spawn(sock_reader)
        };

        let sock_writer = {
            let sock_writer = SockWriter {
                written: write_half.clone(),
                buf: sockw_fdr_buf.clone(),
            }.new();
            task::spawn(sock_writer)
        };

        let fd_reader = { 
            let fd_reader = FdReader {
                read: qrexec_stdout.clone(),
                buf: sockw_fdr_buf.clone(),
                write_flag: write_flag.clone(),
            }.new();
            task::spawn(fd_reader)
        };


        let fd_writer = {
            let fd_writer = FdWriter {
                written: qrexec_stdin.clone(),
                buf: sockr_fdw_buf.clone(),
                write_flag: write_flag.clone(),
            }.new();
            task::spawn(fd_writer)
        };

        return Self([
            fd_writer,
            fd_reader,
            sock_writer,
            sock_reader,
        ]).check().await;
    }

    async fn check(&mut self) -> DynFutError<()> {
        loop { 
            for handle in &mut self.0 {
                if handle.is_finished() { break }
                time::sleep(Duration::from_secs(3)).await;
            }

            for handle in &mut self.0 {
                handle.abort();
            }
        }
    }
}

struct FdWriter<U: AsyncWriteExt + Unpin> {
    written: Arc<Mutex<U>>,
    buf: Arc<Mutex<Vec<u8>>>,
    write_flag: Arc<AtomicBool>,
}

impl<U: AsyncWriteExt + Unpin> FdWriter<U> {
    pub async fn new(self) -> DynFutError<()> {
        loop {
            match self.write_flag.load(SeqCst) { 
                WRITABLE => (),
                UNWRITABLE => time::sleep(
                    Duration::from_millis(SLEEP_TIME_MILLIS)
                ).await,
            }

            let mut buf;
            let mut buf_len;
            loop {
                buf = self.buf.lock().await; 
                buf_len = buf.len();
                if buf_len == 0 {
                    drop(buf);
                    time::sleep(
                        Duration::from_millis(SLEEP_TIME_MILLIS)
                    ).await;
                    continue; 
                } else {
                    break;
                }
            }

            let mut written = self.written.lock().await;
            let mut bytes = 0;

            while bytes < buf_len {
                match written.write(&buf[bytes..]).await {
                    Ok(n_bytes) => bytes += n_bytes,
                    Err(ref e) if e.kind() == WouldBlock => {
                        time::sleep(
                            Duration::from_millis(SLEEP_TIME_MILLIS)
                        ).await;
                        continue;
                    }
                    Err(e) => return Err(Box::new(e)),
                }
            }

            self.write_flag.store(UNWRITABLE, SeqCst);

            if let Err(e) = written.flush().await { return Err(Box::new(e)); }
            buf.clear();
        }
    } 
}

struct FdReader<T: AsyncReadExt + Unpin> {
    read: Arc<Mutex<T>>, 
    buf: Arc<Mutex<Vec<u8>>>,
    write_flag: Arc<AtomicBool>,
}

impl<T: AsyncReadExt + Unpin> FdReader<T> {
    pub async fn new(self) -> DynFutError<()> {
        loop {
            let mut buf = self.buf.lock().await;
            if !buf.is_empty() { 
                drop(buf);
                time::sleep(
                    Duration::from_millis(SLEEP_TIME_MILLIS)
                ).await;
                continue; 
            }

            let mut read = self.read.lock().await;
            let mut num_bytes = 0;
            let msg_len;

            loop {
                match read.read(&mut buf).await {
                    Ok(bytes) => num_bytes += bytes, 
                    Err(ref e) if e.kind() == WouldBlock => { 
                        if num_bytes >= 8 {
                            break;
                        }
                    }
                    Err(e) => return Err(Box::new(e)),
                }
            }

            msg_len = MsgHeader::len(&buf[..8]);
            num_bytes -= 8;

            if num_bytes as u64 != msg_len {
                loop {
                    match read.read(&mut buf).await {
                        Ok(bytes) => num_bytes += bytes,
                        Err(ref e) if e.kind() == WouldBlock => {
                            if num_bytes as u64 == msg_len {
                                break;
                            }
                        }
                        Err(e) => return Err(Box::new(e)),
                    }
                }
            }

            self.write_flag.store(WRITABLE, SeqCst);

            #[cfg(debug_assertions)]
            debug_buf_to_file(&buf).await?;
        }
    }
}

struct SockWriter {
    written: Arc<Mutex<OwnedWriteHalf>>,
    buf: Arc<Mutex<Vec<u8>>>,
}

impl SockWriter {
    pub async fn new(self) -> DynFutError<()> {
        loop {
            let mut written = self.written.lock().await;
            let mut buf = self.buf.lock().await;

            if buf.is_empty() {
                drop(buf);
                time::sleep(
                    Duration::from_millis(SLEEP_TIME_MILLIS)
                ).await;
                continue;
            }

            let mut bytes = 0;
            let msg_len = MsgHeader::len(&buf[..8]);

            while bytes < msg_len {
                match written.write(&mut buf).await {
                    Ok(num_bytes) => bytes += num_bytes as u64,
                    Err(ref e) if e.kind() == WouldBlock => {
                        time::sleep(Duration::from_millis(SLEEP_TIME_MILLIS)).await;
                        continue;
                    }
                    Err(e) => return Err(Box::new(e)),
                } 
            }

            buf.clear();
        }
    }
}

struct SockReader {
    read: Arc<OwnedReadHalf>,
    buf: Arc<Mutex<Vec<u8>>>,
    write_flag: Arc<AtomicBool>,
}

impl SockReader {
    pub async fn new(self) -> DynFutError<()> {
        let mut int_buf: Vec<u8> = Vec::new();
        loop {
            let mut buf = self.buf.lock().await;
            let mut num_bytes = 0; 

            if !buf.is_empty() { 
                drop(buf);
                time::sleep(
                    Duration::from_millis(SLEEP_TIME_MILLIS)
                ).await;
                continue; 
            }

            loop {
                match self.write_flag.load(SeqCst) {
                    WRITABLE => break, 
                    UNWRITABLE => {
                        time::sleep(
                            Duration::from_millis(SLEEP_TIME_MILLIS)
                        ).await;
                        continue;   
                    }
                }
            }

            let mut block_count = 0u8;
            loop {
                match self.read.try_read_buf(&mut int_buf) {
                    Ok(num_read) => num_bytes += num_read,
                    Err(ref e) if e.kind() == WouldBlock => {
                        block_count += 1;
                        if block_count == 3 {
                            break;
                        }
                    }
                    Err(e) => return Err(Box::new(e)),
                };
            }

            buf.extend_from_slice(&MsgHeader::new(int_buf.len() as u64).0);
            buf.extend_from_slice(&int_buf[..num_bytes]);
            buf.truncate(num_bytes + HEADER_LEN);

            #[cfg(debug_assertions)] 
            debug_buf_to_file(&buf).await?;
        }
    }
}

pub struct SockStream {
    sock: UnixListener,
    path: path::PathBuf,
}

impl Deref for SockStream {
    type Target = UnixListener;
    fn deref(&self) -> &Self::Target {
        return &self.sock;
    }
}

impl DerefMut for SockStream {
    fn deref_mut(&mut self) -> &mut Self::Target {
        return &mut self.sock;
    }
}

impl Drop for SockStream {
    fn drop(&mut self) {
        if std::fs::exists(&self.path).unwrap() {
            std::fs::remove_file(&self.path).unwrap(); 
        }
    } 
}

const SOCK_VAR: &str = "SSH_AUTH_SOCK";
pub fn get_auth_sock() -> DynError<SockStream> {
    let path = env::var(SOCK_VAR)?.into();
    let sock = if std::fs::exists(&path)? {
        return Err(anyhow!(
            "Error: The auth sock is already bound."
        ).into())
    } else {
        UnixListener::bind(&path)?
    };

    return Ok(SockStream {
        sock,
        path,
    });
}
