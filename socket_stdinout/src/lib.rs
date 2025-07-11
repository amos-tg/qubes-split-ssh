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

type Thread = JoinHandle<()>;

pub struct SockStdInOutCon {
    kill: Arc<AtomicBool>,
    broken_pipe: Arc<AtomicBool>,
    timeout: Arc<AtomicBool>,
}

impl<'a> SockStdInOutCon {
    const DEBUG_FNAME: &'static str = "Main";
    const SLEEP_TIME_SECS: u64 = 15;

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

    pub fn client() {
        
    }

    pub fn vault(
        stream: Arc<Mutex<UnixStream>>,
        written: Arc<Mutex<impl Write>>,
        read: Arc<Mutex<impl Read>>,
    ) {
        let conn = Self::spawn(
            stream,   
            written,
            read);

        loop {
            thread::sleep(Duration::from_secs(Self::SLEEP_TIME_SECS));
        }
    }
  
    fn spawn(
        stream: Arc<Mutex<UnixStream>>,
        written: Arc<Mutex<impl Write>>,
        read: Arc<Mutex<impl Read>>,
    ) -> Self {
        let kill = Arc::new(AtomicBool::new(false));
        let timeout = Arc::new(
            AtomicBool::new(false));
        let broken_pipe = Arc::new(AtomicBool::new(false));

        let sockr_fdw_buf = Arc::new(Mutex::new(Vec::new()));
        let sockw_fdr_buf = Arc::new(Mutex::new(Vec::new()));

        let sockr_fdw = thread::spawn( || {
            SockReaderFdWriter {
                stream: stream.clone(),
                fd: written.clone(),
                timeout: timeout.clone(),
                kill: kill.clone(),
                broken_pipe: broken_pipe.clone(),
            }.spawn() 
        });

        let sockw_fdr = thread::spawn( || {
            SockWriterFdReader {
                stream: stream.clone(),
                fd: read.clone(),
                timeout: timeout.clone(),
                kill: kill.clone(),
                broken_pipe: broken_pipe.clone(),
            }.spawn()
        });

        Self {
            kill,
            broken_pipe,
            timeout,
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
    broken_pipe: Arc<AtomicBool>,
}

impl<T: Write> SockReaderFdWriter<T> {
    const ACTIVE: bool = true;
    const INACTIVE: bool = false;
    const DEBUG_FNAME: &str = "SockReaderFdWriter";
    pub fn spawn(self) {
        let mut int_buf = Vec::new();
        let mut buf = Vec::new();
        let mut buf_len: usize;
        let mut cursor = 0usize;
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

                        Err(err) if err.kind() == 
                            io::ErrorKind::BrokenPipe => {
                            
                        }

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

struct SockWriterFdReader<T: Read> {
    stream: Arc<Mutex<UnixStream>>,
    fd: Arc<Mutex<T>>, 
    kill: Arc<AtomicBool>,
}

impl<T: Read> SockWriterFdReader<T> {
    const DEBUG_FNAME: &str = "SockWriterFdReader";
    pub async fn new(self) {
        let mut buf = Vec::new();
        let mut msg_len;
        let mut cursor = 0;
        let mut header: [u8; HEADER_LEN];
        'reconn: loop {
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
                match fd.read_exact(&mut buf[..HEADER_LEN]) {
                    Ok(_) => cursor += HEADER_LEN, 

                    Err(e) if e.kind() == 
                        io::ErrorKind::UnexpectedEof=> {
                            cursor = 0;
                            continue 'reconn; 
                    }

                    Err(err) => {
                        debug_err_append(
                            &Err(err),
                            Self::DEBUG_FNAME,
                            ERR_LOG_DIR_NAME);
                        cursor = 0;
                        continue 'reconn;
                    }
                }

                header.clone_from_slice(&buf[..HEADER_LEN]);
                msg_len = MsgHeader::len(header);
                cursor -= 8;

                if cursor as u64 != msg_len {
                    loop {
                        match fd.read(&mut buf) {
                            Ok(bytes) => cursor += bytes,
                            Err(ref e) if e.kind() == WouldBlock => {
                                if cursor as u64 == msg_len {
                                    break;
                                }
                            }
                            Err(e) => (),
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
