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
const WRITABLE: bool = true; 
const UNWRITABLE: bool = false;

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
        initial_write_flag_state: bool,
        std_written: Arc<Mutex<impl AsyncWriteExt + Unpin + Send + 'static>>,
        std_read: Arc<Mutex<impl AsyncReadExt + Unpin + Send + 'static>>,
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
        let sockw_fdr_buf = Arc::new(Mutex::new(Vec::new()));

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
                read: std_read.clone(),
                buf: sockw_fdr_buf.clone(),
                write_flag: write_flag.clone(),
            }.new();
            task::spawn(fd_reader)
        };


        let fd_writer = {
            let fd_writer = FdWriter {
                written: std_written.clone(),
                buf: sockr_fdw_buf.clone(),
                write_flag: write_flag.clone(),
            }.new();
            task::spawn(fd_writer)
        };

        return Self {
            fd_writer,
            fd_reader,
            sock_writer,
            sock_reader,
        }.handler().await;
    }

    async fn handler(self) -> DynFutError<()> {
        const HNDLER_ERR: &str = "finished with an impossible return val.";
        loop { 
            if self.fd_writer.is_finished() { 
                self.fd_reader.abort(); 
                self.sock_writer.abort();
                self.sock_reader.abort();
                wield_err!(self.fd_writer.await)?;
                unreachable!("Error: fd_writer {HNDLER_ERR}");
            }

            if self.fd_reader.is_finished() {
                self.fd_writer.abort();
                self.sock_writer.abort();
                self.sock_reader.abort();
                wield_err!(self.fd_reader.await)?;
                unreachable!("Error: fd_reader {HNDLER_ERR}");
            }

            if self.sock_writer.is_finished() {
                self.fd_writer.abort();
                self.fd_reader.abort();
                self.sock_reader.abort();
                wield_err!(self.sock_writer.await)?;
                unreachable!("Error: sock_writer {HNDLER_ERR}");
            }

            if self.sock_reader.is_finished() {
                self.fd_writer.abort();
                self.fd_reader.abort();
                self.sock_writer.abort();
                wield_err!(self.sock_reader.await)?;
                unreachable!("Error: sock_reader {HNDLER_ERR}");
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
    write_flag: Arc<AtomicBool>,
}

impl<U: AsyncWriteExt + Unpin> FdWriter<U> {
    const DEBUG_FNAME: &str = "FdWriter"; 
    pub async fn new(self) -> DynFutError<()> {
        loop {
            debug_append(
                "Startup\n",
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME,
            );

            loop {
                match self.write_flag.load(SeqCst) { 
                    WRITABLE => break,
                    UNWRITABLE => time::sleep(
                        Duration::from_millis(SLEEP_TIME_MILLIS)
                    ).await,
                }
            }

            debug_append(
                "Got WRITABLE flag state...\n",
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

            debug_append(
                "Wrote everything...\n",
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME,
            );
        }
    } 
}

struct FdReader<T: AsyncReadExt + Unpin> {
    read: Arc<Mutex<T>>, 
    buf: Arc<Mutex<Vec<u8>>>,
    write_flag: Arc<AtomicBool>,
}

impl<T: AsyncReadExt + Unpin> FdReader<T> {
    const DEBUG_FNAME: &str = "FdReader";
    pub async fn new(self) -> DynFutError<()> {
        loop {
            debug_append(
                "Startup...",
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

            self.write_flag.store(WRITABLE, SeqCst);

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
                "starting...\n",
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

            let mut bytes = 0;
            let msg_len = MsgHeader::len(&buf[..8]);

            debug_append(
                format!("starting {} length write...\n", msg_len),
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME,
            );

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
    write_flag: Arc<AtomicBool>,
}

impl SockReader {
    const DEBUG_FNAME: &str = "SockReader";
    pub async fn new(self) -> DynFutError<()> {
        let mut int_buf: Vec<u8> = Vec::new();
        loop {
            debug_append(
                "Starting SockReader...\n",
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME,
            );


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

            debug_append(
                "starting read...\n",
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME,
            );

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

            debug_append(
                &*buf,
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
        initial_write_flag_state: bool,
        std_written: impl AsyncWriteExt + Unpin + Send + 'static,
        std_read: impl AsyncReadExt + Unpin + Send + 'static,
    ) -> DynFutError<()> {
        let (std_written, std_read) = (
            Arc::new(Mutex::new(std_written)),
            Arc::new(Mutex::new(std_read)),
        );

        SockStdInOutCon::spawn(
            self.0,
            initial_write_flag_state,
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
        initial_write_flag_state: bool,
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
                initial_write_flag_state,
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
