mod msg_header;
pub mod debug;
pub mod types;

use types::{DynError, DTErr};
use debug::{debug_append, debug_err_append};
use msg_header::{
    MsgHeader,
    HEADER_LEN,
};

use std::{
    fs,
    env,
    error::Error,
    time::Duration,
    ops::{Deref, DerefMut},
    io::{self, Read, Write, ErrorKind::WouldBlock},
    sync::{
        atomic::{AtomicBool, Ordering::*},
        Arc,
        Mutex,
    },
    os::unix::{
        net::{UnixListener, UnixStream},
        fs::PermissionsExt,
    },
    thread::{JoinHandle, self},
};

use anyhow::anyhow;

pub const ERR_LOG_DIR_NAME: &str = "split-ssh";
const SLEEP_TIME_MILLIS: u64 = 100; 

type Thread = 
    JoinHandle<Result<(), Box<dyn Error + 'static + Send>>>;

pub struct SockStdInOutCon {
    kill: Arc<AtomicBool>,
    sockreader_fdwriter: Thread,
    sockwriter_fdreader: Thread,
}

impl<'a> SockStdInOutCon {
    #[inline(always)]
    pub fn stream_and_touts(listener: &UnixListener) -> DynError<Arc<Mutex<UnixStream>>> {
        const READ_TOUT_SECS: u64 = 5;
        const WRITE_TOUT_SECS: u64 = 5;
        let stream = listener.accept()?.0;
        stream.set_read_timeout(
            Some(Duration::from_secs(READ_TOUT_SECS)))?; 
        stream.set_write_timeout(
            Some(Duration::from_secs(WRITE_TOUT_SECS)))?;
        return Ok(Arc::new(Mutex::new(stream)));
    }

    pub fn spawn(
        listener: UnixListener,
        written: Arc<Mutex<impl Write>>,
        read: Arc<Mutex<impl Read>>,
    ) -> DynError<()> {
        let kill = Arc::new(AtomicBool::new(false));
        let timeout = Arc::new(
            AtomicBool::new(false));
        let sockr_fdw_buf = Arc::new(Mutex::new(Vec::new()));
        let sockw_fdr_buf = Arc::new(Mutex::new(Vec::new()));
        let stream = Self::stream_and_touts(&listener)?;

        let sockr_fdw = thread::spawn( || {
            SockReaderFdWriter {
                stream: stream.clone(),
                fd: written.clone(),
                timeout: timeout.clone(),
                kill: kill.clone(),
            }.spawn() 
        });

        let sockw_fdr = thread::spawn( || {
            SockWriterFdReader {
                stream: stream.clone(),
                fd: read.clone(),
                timeout: timeout.clone(),
                kill: kill.clone(),
            }.spawn()
        });

        return Self {
            kill,
            sockreader_fdwriter: sockr_fdw,
            sockwriter_fdreader: sockw_fdr,
        }.handler(timeout);
    }

    fn handler(self, timeout: Arc<AtomicBool>) -> DynError<()> {
        const HNDLER_ERR: &str = "finished with an impossible return val.";
        // you should make this an argument to the program so people with diff. requirements
        // can use it with a bigger timeout. 
        const T_OUT_MAX: u8 = 100;
        let mut t_out_counter = 0u8;
        loop { 
            macro_rules! taskerr {
                ($err:expr, $task:literal) => {
                    match $err {
                        Err(err) => {
                            return Err(anyhow!(
                                format!(
                                    "Error: task={} : {}",
                                    $task,
                                    err.to_string()
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

            if self.sock_reader.is_finished() {
                self.fd_writer.abort();
                self.fd_reader.abort();
                self.sock_writer.abort();
                taskerr!(
                    self.sock_reader,
                    "SockReader"
                );
                unreachable!("Error: sock_reader {HNDLER_ERR}");
            }

            if self.fd_writer.is_finished() { 
                self.fd_reader.abort(); 
                self.sock_writer.abort();
                self.sock_reader.abort();
                taskerr!(
                    self.fd_writer,
                    "FdWriter"
                );
                unreachable!("Error: fd_writer {HNDLER_ERR}");
            }

            if self.fd_reader.is_finished() {
                self.fd_writer.abort();
                self.sock_writer.abort();
                self.sock_reader.abort();
                taskerr!(
                    wield_err!(self.fd_reader),
                    "FdReader"
                );
                unreachable!("Error: fd_reader {HNDLER_ERR}");
            }

            if self.sock_writer.is_finished() {
                self.fd_writer.abort();
                self.fd_reader.abort();
                self.sock_reader.abort();
                taskerr!(
                    self.sock_writer,
                    "SockWriter"
                );
                unreachable!("Error: sock_writer {HNDLER_ERR}");
            }


            if t_out_counter == T_OUT_MAX {
                break Ok(());
            }

            thread::sleep(
                Duration::from_millis(SLEEP_TIME_MILLIS)
            );
        }
    }
}

impl Drop for SockStdInOutCon {
    fn drop(&mut self) {
        self.kill.store(true, SeqCst); 
    }
}

struct SockReaderFdWriter<T: Write> {
    stream: Arc<Mutex<UnixStream>>,
    fd: Arc<Mutex<T>>,
    timeout: Arc<AtomicBool>,
    kill: Arc<AtomicBool>,
}

impl<T: Write> SockReaderFdWriter<T> {
    const ACTIVE: bool = true;
    const INACTIVE: bool = false;
    const DEBUG_FNAME: &str = "SockReader";
    pub fn spawn(self) {
        let mut int_buf = Vec::new();
        let mut buf = Vec::new();
        let mut buf_len: usize;
        let mut cursor;
        'reconn: loop {
            // make sure not to block indefinitely by panic'ing in main 
            // while holding the stream mutex guard
            let stream = self.stream.lock();
            debug_err_append(
                &stream,
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME);
            let mut stream = stream.unwrap();

            let fd = self.fd.lock();
            debug_err_append(
                &fd,
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME);
            let mut fd = fd.unwrap();

            loop {
                loop {
                    if self.kill.load(SeqCst) { panic!() }
                    match stream.read(&mut int_buf[cursor..]) {
                        Ok(num_read) => if num_read != 0 {
                            cursor += num_read;
                            break;
                        } else {
                            cursor = 0;
                            int_buf.clear();
                            buf.clear();
                            continue 'reconn;
                        }

                        Err(err) if err.kind() == 
                            io::ErrorKind::Interrupted => continue,

                        
                        Err(err) if err.kind() == 
                            io::ErrorKind::TimedOut => continue 'reconn,

                        Err(_) => {
                            cursor = 0;
                            int_buf.clear();
                            buf.clear();
                            continue 'reconn;
                        }
                    };
                }

                buf.extend_from_slice(
                    &MsgHeader::new(cursor as u64).0);
                buf.extend_from_slice(&int_buf[..cursor]);
                buf_len = cursor + HEADER_LEN;
                buf.truncate(buf_len);

                int_buf.clear();

                cursor = 0;
                while cursor < buf_len {
                    if self.kill.load(SeqCst) { panic!() }
                    match fd.write(&buf[cursor..]) {
                        Ok(n_bytes) => cursor += n_bytes,

                        Err(err) if err.kind() ==
                            io::ErrorKind::Interrupted => continue,

                        Err(_) => {
                            cursor = 0;
                            buf.clear();
                            continue 'reconn;
                        }
                    }
                }

                cursor = 0;
                buf.clear();
                if fd.flush().is_err() {
                    continue 'reconn;
                }
            }
        }
    }
}

struct FdWriter<U: Write> {
    written: Arc<Mutex<U>>,
    buf: Arc<Mutex<Vec<u8>>>,
    kill: Arc<AtomicBool>,
}

impl<U: Write> FdWriter<U> {
    const DEBUG_FNAME: &str = "FdWriter"; 
    pub async fn new(self) -> DTErr<()> {
        loop {
            let (mut buf, mut buf_len) = loop {
                let buf = self.buf.lock(); 
                debug_err_append(
                    &buf,    
                    Self::DEBUG_FNAME,
                    ERR_LOG_DIR_NAME);
                let buf = buf.unwrap();

                let buf_len = buf.len();
                if buf_len == 0 {
                    drop(buf);
                    thread::sleep(
                        Duration::from_millis(SLEEP_TIME_MILLIS));
                    continue; 
                } else {
                    break (buf, buf_len);
                }
            };

            let mut written = self.written.lock();
            let mut bytes = 0;

            while bytes < buf_len {
                match written.write(&buf[bytes..]).await {
                    Ok(n_bytes) => bytes += n_bytes,
                    Err(ref e) if e.kind() == WouldBlock => {
                        thread::sleep(
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

struct FdReader<T: Read> {
    read: Arc<Mutex<T>>, 
    buf: Arc<Mutex<Vec<u8>>>,
    kill: Arc<AtomicBool>,
}

impl<T: Read> FdReader<T> {
    const DEBUG_FNAME: &str = "FdReader";
    pub async fn new(self) -> DynFutError<()> {
        loop {
            debug_append(
                "Starting iteration...\n",
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME,
            );

            let mut buf = self.buf.lock();
            if !buf.is_empty() { 
                drop(buf);
                thread::sleep(
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
    written: Arc<Mutex<UnixStream>>,
    buf: Arc<Mutex<Vec<u8>>>,
    kill: Arc<AtomicBool>,
}

impl SockWriter {
    const DEBUG_FNAME: &str = "SockWriter";
    pub fn new(self) -> DTErr<()> {
        loop {
            let buf = self.buf.lock();
            debug_err_append(
                &buf,
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME);
            let mut buf = buf.unwrap();
            if buf.is_empty() {
                drop(buf);
                thread::sleep(
                    Duration::from_millis(SLEEP_TIME_MILLIS));
                continue;
            }

            let written = self.written.lock();
            debug_err_append(
                &written,
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME);
            let mut written = written.unwrap();
            let tout = written.set_write_timeout(Some(Duration::from_secs(5)));
            debug_err_append(
                &tout,
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME);
            tout.unwrap();
            
            let buf_len = buf.len();
            let mut cursor = 0usize;
            while cursor < buf_len {
                if self.kill.load(SeqCst) { panic!() }
                match written.write(&mut buf[cursor..buf_len]) {
                    Ok(0) => return Err(anyhow!(
                        "Error: {}: reached EOF", Self::DEBUG_FNAME).into()),
                    Ok(n) => cursor += n,
                    Err(e) => return Err(Box::new(e)),
                } 
            }

            buf.clear();
        }
    }
}

const SOCK_VAR: &str = "SSH_AUTH_SOCK";

pub struct SockStream(UnixStream);

impl SockStream {
    pub fn get_auth_stream() -> DynError<Self> {
        let path = env::var(SOCK_VAR)?;
        let sock = if fs::exists(&path)? {
            UnixStream::connect(&path)?
        } else {
            return Err(anyhow!(
                "Error: the socket doesn't exist to connect to"
            ).into());
        };

        return Ok(Self(sock));
    }
    
    pub fn handle_connections(
        self,
        std_written: impl Write,
        std_read: impl Read,
    ) -> DynFutError<()> {
        let (std_written, std_read) = (
            Arc::new(Mutex::new(std_written)),
            Arc::new(Mutex::new(std_read)),
        );

        SockStdInOutCon::spawn(
            self.0,
            std_written,
            std_read,
        )?;

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

        let mut perms = fs::metadata(&path)?.permissions();
        perms.set_mode(0o777);
        fs::set_permissions(&path, perms)?;

        return Ok(Self(sock))
    }

    pub fn handle_connections(
        &self,
        std_written: impl Write,
        std_read: impl Read,
    ) -> DynError<()> {
        let (std_written, std_read) = (
            Arc::new(Mutex::new(std_written)),
            Arc::new(Mutex::new(std_read)),
        );

        loop {
            SockStdInOutCon::spawn(
                self.0.accept()?.0,
                std_written.clone(),
                std_read.clone(),
            )?;
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
        let Ok(addr) = self.local_addr() else {
            return;
        };

        let Some(path) = addr.as_pathname() else {
            return;
        };

        let Ok(fstat) = std::fs::exists(&path) else {
            return;
        };

        if fstat {
            let _ = std::fs::remove_file(path); 
        }
    } 
}
