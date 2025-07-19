mod msg_header;
pub mod debug;
pub mod types;

use types::DynError;
use debug::{debug_append, debug_err_append};
use msg_header::{
    MsgHeader,
    HEADER_LEN,
};

use std::{
    fs,
    env,
    time::Duration,
    ops::{Deref, DerefMut},
    io::{self, Read, Write},
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

type Thread = JoinHandle<()>;

pub struct SockStdInOutCon {
    kill: Arc<AtomicBool>,
    broken_pipe: Arc<AtomicBool>,
    sock_reader_fd_writer: Thread,
    sock_writer_fd_reader: Thread,
}

impl<'a> SockStdInOutCon {
    fn spawn<T, U>(
        stream: Arc<Mutex<UnixStream>>,
        written: Arc<Mutex<T>>,
        read: Arc<Mutex<U>>,
    ) -> Self where
        T: Write + Send + Sync + 'static,
        U: Read + Send + Sync + 'static,
    {
        let kill = Arc::new(AtomicBool::new(false));
        let broken_pipe = Arc::new(AtomicBool::new(false));

        let sock_reader_fd_writer = {
            let srfw = SockReaderFdWriter {
                stream: stream.clone(),
                fd: written.clone(),
                kill: kill.clone(),
                broken_pipe: broken_pipe.clone(),
            };
            thread::spawn(move || { srfw.spawn() })  
        };

        let sock_writer_fd_reader = {
            let swfr = SockWriterFdReader {
                stream: stream.clone(),
                fd: read.clone(),
                kill: kill.clone(),
                broken_pipe: broken_pipe.clone(),
            };
            thread::spawn(move || { swfr.spawn() })
        };

        Self {
            kill,
            broken_pipe,
            sock_reader_fd_writer, 
            sock_writer_fd_reader,
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
    kill: Arc<AtomicBool>,   
    broken_pipe: Arc<AtomicBool>,
}

impl<T: Write> SockReaderFdWriter<T> {
    const DEBUG_FNAME: &str = "SockReaderFdWriter";
    pub fn spawn(self) { 
        let mut int_buf = Vec::new();
        let mut buf = Vec::new();
        let mut buf_len: usize;
        let mut cursor = 0usize;
        'reconn: loop {
            if self.kill.load(SeqCst) { panic!() }

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
                match stream.read(&mut int_buf[cursor..]) {
                    Ok(num_read) => if num_read != 0 {
                        cursor += num_read;
                        break;
                    } else {
                        cursor = 0;
                        int_buf.clear();
                        continue 'reconn;
                    }

                    Err(err) if err.kind() == 
                        io::ErrorKind::Interrupted => continue 'reconn,
                    
                    Err(err) if err.kind() == 
                        io::ErrorKind::TimedOut => continue 'reconn,

                    Err(err) if err.kind() == 
                        io::ErrorKind::BrokenPipe => {
                        cursor = 0;
                        int_buf.clear(); 
                        self.broken_pipe.store(true, SeqCst);
                        continue 'reconn; 
                    }

                    Err(err) => {
                        debug_err_append(
                            &Err::<(), io::Error>(err),
                            Self::DEBUG_FNAME,
                            ERR_LOG_DIR_NAME);
                        cursor = 0;
                        int_buf.clear();
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
            let _ = fd.flush();
            
        }
    }
}

struct SockWriterFdReader<T: Read> {
    stream: Arc<Mutex<UnixStream>>,
    fd: Arc<Mutex<T>>, 
    kill: Arc<AtomicBool>,
    broken_pipe: Arc<AtomicBool>,
}

impl<T: Read> SockWriterFdReader<T> {
    const DEBUG_FNAME: &str = "SockWriterFdReader";
    const MSG_LEN_UNEQ: &str = "Error: the msg_len != cursor\n"; 
    pub fn spawn(self) {
        let mut buf = Vec::new();
        let mut msg_len;
        let mut cursor;
        let mut header = [0u8; HEADER_LEN];
        let mut stream;
        let mut stream_res;
        'reconn: loop {
            if self.kill.load(SeqCst) { panic!() }

            stream_res = self.stream.lock();
            debug_err_append(
                &stream_res,
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME);
            stream = stream_res.unwrap();

            let fd = self.fd.lock();
            debug_err_append(
                &fd,
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME);
            let mut fd = fd.unwrap();

            loop {
                match fd.read_exact(&mut buf[..HEADER_LEN]) {
                    Ok(_) => (),

                    Err(err) if err.kind() == 
                        io::ErrorKind::Interrupted => continue, 

                    Err(err) if err.kind() == 
                        io::ErrorKind::UnexpectedEof => {
                        debug_err_append(
                            &Err::<(), io::Error>(err),
                            Self::DEBUG_FNAME,
                            ERR_LOG_DIR_NAME);
                        buf.clear();
                        continue 'reconn; 
                    }

                    Err(err) if err.kind() == 
                        io::ErrorKind::BrokenPipe => {
                        buf.clear();
                        self.broken_pipe.store(true, SeqCst);
                        continue 'reconn;
                    }

                    Err(err) => {
                        debug_err_append(
                            &Err::<(), io::Error>(err),
                            Self::DEBUG_FNAME,
                            ERR_LOG_DIR_NAME);
                        buf.clear();
                        continue 'reconn;
                    }
                }

                header.clone_from_slice(&buf[..HEADER_LEN]);
                buf.clear();
                msg_len = MsgHeader::len(header); 

                cursor = 0;
                while (cursor as u64) < msg_len {
                    match fd.read(&mut buf[cursor..]) {
                        Ok(n_bytes) => cursor += n_bytes,

                        Err(err) if err.kind() ==
                            io::ErrorKind::Interrupted => continue,
                        
                        Err(err) if err.kind() == 
                            io::ErrorKind::UnexpectedEof => {
                            debug_err_append(
                                &Err::<(), io::Error>(err),
                                Self::DEBUG_FNAME,
                                ERR_LOG_DIR_NAME);
                            buf.clear();
                            continue 'reconn; 
                        }

                        Err(err) => {
                            debug_err_append(
                                &Err::<(), io::Error>(err),
                                Self::DEBUG_FNAME,
                                ERR_LOG_DIR_NAME);
                            buf.clear();
                            continue 'reconn;
                        }
                    }
                }

                #[cfg(debug_assertions)]
                if (cursor as u64) != msg_len {
                    debug_append(
                        Self::MSG_LEN_UNEQ, 
                        Self::DEBUG_FNAME,
                        ERR_LOG_DIR_NAME);
                }

                cursor = 0;
                while (cursor as u64) < msg_len {
                    match stream.write(&buf[cursor..]) {
                        Ok(n_bytes) => cursor += n_bytes,

                        Err(err) if err.kind() == 
                            io::ErrorKind::TimedOut => {
                            drop(stream);
                            stream_res = self.stream.lock();
                            debug_err_append(
                                &stream_res,
                                Self::DEBUG_FNAME,
                                ERR_LOG_DIR_NAME);
                            stream = stream_res.unwrap();
                            continue;
                        }

                        Err(err) if err.kind() == 
                            io::ErrorKind::Interrupted => {
                            stream_res = self.stream.lock();
                            debug_err_append(
                                &stream_res,
                                Self::DEBUG_FNAME,
                                ERR_LOG_DIR_NAME);
                            stream = stream_res.unwrap();
                            continue;
                        }

                        Err(err) if err.kind() ==
                            io::ErrorKind::UnexpectedEof => {
                            debug_err_append(
                                &Err::<(), io::Error>(err),
                                Self::DEBUG_FNAME,
                                ERR_LOG_DIR_NAME);
                            buf.clear();
                            continue 'reconn;
                        }

                        Err(err) => {
                            debug_err_append(
                                &Err::<(), io::Error>(err),
                                Self::DEBUG_FNAME,
                                ERR_LOG_DIR_NAME);
                            buf.clear();
                            continue 'reconn;
                        }
                    }
                } 

                buf.clear();
            }
        }
    }
}

#[inline(always)]
fn stream_and_touts(
    listener: &UnixListener,
) -> Result<UnixStream, io::Error> {
    let stream = listener.accept()?.0;
    touts(&stream)?;
    return Ok(stream);
}

#[inline(always)]
fn touts(stream: &UnixStream) -> Result<(), io::Error> {
    const TOUT_SECS: Duration = Duration::from_secs(2);
    stream.set_read_timeout(
        Some(TOUT_SECS))?; 
    stream.set_write_timeout(
        Some(TOUT_SECS))?;
    return Ok(());
}

const SOCK_VAR: &str = "SSH_AUTH_SOCK";
const SLEEP_DURATION_CTRL: Duration = Duration::from_secs(3);
const DBG_FNAME_MAIN: &str = "Main";
const MUTEX_ERR: &str = "Error: poisened UnixStream Mutex...";
const THREAD_ERR: &str = "Error: at least one of the threads failed";

fn finish_check(conn: &SockStdInOutCon) -> bool {
    if conn.sock_reader_fd_writer.is_finished() { return true }
    if conn.sock_writer_fd_reader.is_finished() { return true }
    return false;
}

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

        touts(&sock)?;
        return Ok(Self(sock));
    }
    
    pub fn handle_connections<T, U>(
        self,
        written: T,
        read: U,
    ) -> Result<(), anyhow::Error> where
        T: Write + Send + Sync + 'static,
        U: Read + Send + Sync + 'static, 
    {
        let (written, read) = (
            Arc::new(Mutex::new(written)),
            Arc::new(Mutex::new(read)));
        let ctrl = SockStdInOutCon::spawn(
            Arc::new(Mutex::new(self.0)),
            written,
            read);

        loop {
            thread::sleep(SLEEP_DURATION_CTRL);

            if finish_check(&ctrl) { 
                let thread_err = Err(anyhow!(THREAD_ERR));
                debug_err_append(
                    &thread_err,
                    DBG_FNAME_MAIN,
                    ERR_LOG_DIR_NAME);
                return thread_err;
            }
        } 
    }  
}

pub struct SockListener(UnixListener);

impl SockListener {
    pub fn get_auth_sock() -> DynError<Self> {
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
        sock.set_nonblocking(true)?;
        return Ok(Self(sock));
    }

    pub fn handle_connections<T, U>(
        self,
        written: T,
        read: U,
    ) -> DynError<()> where
        T: Write + Send + Sync + 'static,
        U: Read + Send + Sync + 'static, 
    {
        let (written, read) = (
            Arc::new(Mutex::new(written)),
            Arc::new(Mutex::new(read)));
        let stream_ctrl = Arc::new(Mutex::new(stream_and_touts(&self.0)?));
        let thread_ctrl = SockStdInOutCon::spawn(
            stream_ctrl.clone(),
            written.clone(),
            read.clone());

        loop { 
            thread::sleep(SLEEP_DURATION_CTRL);

            if finish_check(&thread_ctrl) {
                let thread_err = Err(anyhow!(THREAD_ERR).into()); 
                debug_err_append(
                    &thread_err,
                    DBG_FNAME_MAIN,
                    ERR_LOG_DIR_NAME);
                return thread_err;
            }

            if thread_ctrl.broken_pipe.load(SeqCst) {
                let conn = match self.0.accept() {
                    Ok(conn) => conn.0,
                    Err(err) if err.kind() == 
                        io::ErrorKind::WouldBlock => continue,
                    Err(err) => return Err(err.into()),
                };

                let Ok(mut stream_lock) = stream_ctrl.lock() else {
                    return Err(anyhow!(
                        MUTEX_ERR).into());
                }; 

                *stream_lock = conn;
                thread_ctrl.broken_pipe.store(false, SeqCst); 
            }
        };
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
