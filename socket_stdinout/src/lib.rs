mod msg_header;
pub mod debug;
pub mod types;

use types::{DynError, DynFutError};
use debug::debug_append;
use msg_header::{
    MsgHeader,
    HEADER_LEN,
};

use std::{
    env,
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
    sync::Mutex,
};

use anyhow::anyhow;

pub const ERR_LOG_DIR_NAME: &str = "split-ssh";
const SLEEP_TIME_MILLIS: u64 = 100; 

type TaskHandle = 
    task::JoinHandle<Result<(), Box<dyn std::error::Error + Send>>>
;

pub struct SockStdInOutCon {
    fd_writer: TaskHandle,
    fd_reader: TaskHandle,
    sock_writer: TaskHandle,
    sock_reader: TaskHandle,
}

impl<'a> SockStdInOutCon {
    pub async fn spawn(
        stream: UnixStream,
        std_written: Arc<Mutex<impl AsyncWriteExt + Unpin + Send + 'static>>,
        std_read: Arc<Mutex<impl AsyncReadExt + Unpin + Send + 'static>>,
    ) -> DynFutError<()> {
        let timeout = Arc::new(
            AtomicBool::new(false)
        );

        let (read_half, write_half) = stream.into_split();
        let (read_half, write_half) = (
            Arc::new(read_half),
            Arc::new(Mutex::new(write_half)),
        );

        let sockr_fdw_buf = Arc::new(Mutex::new(Vec::new()));
        let sockw_fdr_buf = Arc::new(Mutex::new(Vec::new()));

        let sock_reader = {
            let sock_reader = SockReader {
                read: read_half.clone(),
                buf: sockr_fdw_buf.clone(),
                timeout: timeout.clone(),
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
                read: std_read.clone(),
                buf: sockw_fdr_buf.clone(),
            }.new();
            task::spawn(fd_reader)
        };

        let fd_writer = {
            let fd_writer = FdWriter {
                written: std_written.clone(),
                buf: sockr_fdw_buf.clone(),
            }.new();
            task::spawn(fd_writer)
        };

        return Self {
            fd_writer,
            fd_reader,
            sock_writer,
            sock_reader,
        }.handler(timeout).await;
    }

    async fn handler(self, timeout: Arc<AtomicBool>) -> DynFutError<()> {
        const HNDLER_ERR: &str = "finished with an impossible return val.";
        // you should make this an argument to the program so people with diff. requirements
        // can use it with a bigger timeout. 
        const T_OUT_MAX: u8 = 100;
        let mut t_out_counter = 0u8;
        loop { 
            macro_rules! taskerr {
                ($err:expr, $task:literal) => {
                    match $err {
                        Err(e) => {
                            return Err(anyhow!(
                                format!(
                                    "Error: task={} : {}",
                                    $task,
                                    e.to_string()
                                )
                            ).into())
                        },
                        Ok(thing) => thing,
                    }
                }
            }

            match timeout.load(SeqCst) { 
                SockReader::ACTIVE => t_out_counter = 0,
                SockReader::INACTIVE => t_out_counter += 1,
            }

            if self.fd_writer.is_finished() { 
                self.fd_reader.abort(); 
                self.sock_writer.abort();
                self.sock_reader.abort();
                taskerr!(
                    wield_err!(self.fd_writer.await),
                    "FdWriter"
                );
                unreachable!("Error: fd_writer {HNDLER_ERR}");
            }

            if self.fd_reader.is_finished() {
                self.fd_writer.abort();
                self.sock_writer.abort();
                self.sock_reader.abort();
                taskerr!(
                    wield_err!(self.fd_reader.await),
                    "FdReader"
                );
                unreachable!("Error: fd_reader {HNDLER_ERR}");
            }

            if self.sock_writer.is_finished() {
                self.fd_writer.abort();
                self.fd_reader.abort();
                self.sock_reader.abort();
                taskerr!(
                    wield_err!(self.sock_writer.await),
                    "SockWriter"
                );
                unreachable!("Error: sock_writer {HNDLER_ERR}");
            }

            if self.sock_reader.is_finished() {
                self.fd_writer.abort();
                self.fd_reader.abort();
                self.sock_writer.abort();
                taskerr!(
                    wield_err!(self.sock_reader.await),
                    "SockReader"
                );
                unreachable!("Error: sock_reader {HNDLER_ERR}");
            }

            if t_out_counter == T_OUT_MAX {
                break Ok(());
            }

            time::sleep(
                Duration::from_millis(SLEEP_TIME_MILLIS)
            ).await;
        }
    }
}

struct FdWriter<U: AsyncWriteExt + Unpin> {
    written: Arc<Mutex<U>>,
    buf: Arc<Mutex<Vec<u8>>>,
}

impl<U: AsyncWriteExt + Unpin> FdWriter<U> {
    const DEBUG_FNAME: &str = "FdWriter"; 
    pub async fn new(self) -> DynFutError<()> {
        loop {
            debug_append(
                "Starting iteration... \n",
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME,
            );

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

            debug_append(
                "got a buffer with content, starting write...\n", 
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME,
            );

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

            wield_err!(written.flush().await);

            buf.clear();

            debug_append(
                "Wrote everything from buf...\n",
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME,
            );
        }
    } 
}

struct FdReader<T: AsyncReadExt + Unpin> {
    read: Arc<Mutex<T>>, 
    buf: Arc<Mutex<Vec<u8>>>,
}

impl<T: AsyncReadExt + Unpin> FdReader<T> {
    const DEBUG_FNAME: &str = "FdReader";
    pub async fn new(self) -> DynFutError<()> {
        loop {
            debug_append(
                "Starting iteration...\n",
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME,
            );

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

            debug_append(
                "starting read...\n",
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME,
            );

            loop {
                match read.read(&mut buf[num_bytes..]).await {
                    Ok(bytes) => {
                        num_bytes += bytes; 
                        if num_bytes >= HEADER_LEN {
                            break;
                        }
                    }
                    Err(ref e) if e.kind() == WouldBlock => { 
                        if num_bytes >= HEADER_LEN {
                            break;
                        }
                    }
                    Err(e) => return Err(Box::new(e)),
                }
            }

            let mut header_cln: [u8; 8] = [0u8; 8];
            header_cln.clone_from_slice(&buf[..8]);

            msg_len = MsgHeader::len(header_cln);
            num_bytes -= 8;

            debug_append(
                format!("Message length: {}\n", msg_len), 
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME,
            );

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

            debug_append(
                format!(
                    "What was read: {}\n",
                    wield_err!(str::from_utf8(&*buf.clone())),
                ),
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME,
            );
        }
    }
}

struct SockWriter {
    written: Arc<Mutex<OwnedWriteHalf>>,
    buf: Arc<Mutex<Vec<u8>>>,
}

impl SockWriter {
    const DEBUG_FNAME: &str = "SockWriter";
    pub async fn new(self) -> DynFutError<()> {
        loop {
            debug_append(
                "starting iteration...\n",
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME,
            );

            let mut written = self.written.lock().await;
            let mut buf = self.buf.lock().await;

            if buf.is_empty() {
                drop(buf);
                time::sleep(
                    Duration::from_millis(SLEEP_TIME_MILLIS)
                ).await;
                continue;
            }

            let mut bytes = 0usize;
            let mut header_cln = [0u8; 8];
            header_cln.clone_from_slice(&buf[..8]);
            let msg_len = MsgHeader::len(header_cln) as usize;

            debug_append(
                format!("starting {} length write...\n", msg_len),
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME,
            );

            while bytes < msg_len {
                match written.write(&mut buf[bytes..msg_len]).await {
                    Ok(num_bytes) => bytes += num_bytes,
                    Err(ref e) if e.kind() == WouldBlock => {
                        time::sleep(Duration::from_millis(SLEEP_TIME_MILLIS)).await;
                        continue;
                    }
                    Err(e) => return Err(Box::new(e)),
                } 
            }

            debug_append(
                "finished write\n",
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME,
            );

            buf.clear();
        }
    }
}

struct SockReader {
    read: Arc<OwnedReadHalf>,
    buf: Arc<Mutex<Vec<u8>>>,
    timeout: Arc<AtomicBool>,
}

impl SockReader {
    const ACTIVE: bool = true;
    const INACTIVE: bool = false;
    const DEBUG_FNAME: &str = "SockReader";
    pub async fn new(self) -> DynFutError<()> {
        let mut int_buf: Vec<u8> = Vec::new();
        loop {
            debug_append(
                "Starting SockReader iteration...\n",
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME,
            );

            let mut buf = self.buf.lock().await;

            if !buf.is_empty() { 
                drop(buf);
                time::sleep(
                    Duration::from_millis(SLEEP_TIME_MILLIS)
                ).await;
                continue; 
            }

            debug_append(
                "starting read...\n",
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME,
            );

            let mut cursor = 0usize;
            loop {
                wield_err!(self.read.readable().await);
                match self.read.try_read(&mut int_buf[cursor..]) {
                    Ok(num_read) => {
                        if num_read == 0 {
                            self.timeout.store(Self::INACTIVE, SeqCst);
                            break;
                        } else { 
                            self.timeout.store(Self::ACTIVE, SeqCst);
                            cursor += num_read;
                        }
                    }

                    Err(ref e) if e.kind() == WouldBlock => continue,
                    Err(e) => return Err(Box::new(e)),
                };
            }

            let buf_len = int_buf.len() as u64; 
            if buf_len == 0 {
                continue;
            }

            buf.extend_from_slice(&MsgHeader::new(buf_len).0);
            buf.extend_from_slice(&int_buf[..cursor]);
            buf.truncate(cursor + HEADER_LEN);

            debug_append(
                &format!(
                    "{} <- Read buffer\n",
                    wield_err!(
                        str::from_utf8(&buf)
                    ),
                ),
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME,
            );
        }
    }
}

const SOCK_VAR: &str = "SSH_AUTH_SOCK";

pub struct SockStream(UnixStream);

impl SockStream {
    pub async fn get_auth_stream() -> DynError<Self> {
        let path = env::var(SOCK_VAR)?;
        let sock = if std::fs::exists(&path)? {
            wield_err!(UnixStream::connect(&path).await)
        } else {
            return Err(anyhow!(
                "Error: the socket doesn't exist to connect to"
            ).into());
        };

        return Ok(Self(sock));
    }
    
    pub async fn handle_connections(
        self,
        std_written: impl AsyncWriteExt + Unpin + Send + 'static,
        std_read: impl AsyncReadExt + Unpin + Send + 'static,
    ) -> DynFutError<()> {
        let (std_written, std_read) = (
            Arc::new(Mutex::new(std_written)),
            Arc::new(Mutex::new(std_read)),
        );

        SockStdInOutCon::spawn(
            self.0,
            std_written,
            std_read,
        ).await?;

        return Ok(());
    }  
}

pub struct SockListener(UnixListener);

impl SockListener {
    pub async fn get_auth_sock() -> DynError<Self> {
        let path = env::var(SOCK_VAR)?;
        let sock = if std::fs::exists(&path)? {
            return Err(anyhow!(
                "Error: The auth sock is already bound."
            ).into())
        } else {
            UnixListener::bind(&path)?
        };

        return Ok(Self(sock))
    }

    pub async fn handle_connections(
        &self,
        std_written: impl AsyncWriteExt + Unpin + Send + 'static,
        std_read: impl AsyncReadExt + Unpin + Send + 'static,
    ) -> DynFutError<()> {
        let (std_written, std_read) = (
            Arc::new(Mutex::new(std_written)),
            Arc::new(Mutex::new(std_read)),
        );

        loop {
            SockStdInOutCon::spawn(
                wield_err!(self.0.accept().await).0,
                std_written.clone(),
                std_read.clone(),
            ).await?;
        }
    } 
}

impl Deref for SockListener {
    type Target = UnixListener;
    fn deref(&self) -> &Self::Target {
        return &self.0;
    }
}

impl DerefMut for SockListener {
    fn deref_mut(&mut self) -> &mut Self::Target {
        return &mut self.0;
    }
}

impl Drop for SockListener {
    fn drop(&mut self) {
        let addr = self.local_addr().unwrap();
        let Some(path) = addr.as_pathname() else {
            return;
        };

        if std::fs::exists(&path).unwrap() {
            std::fs::remove_file(path).unwrap(); 
        }
    } 
}
