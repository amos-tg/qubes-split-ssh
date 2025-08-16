mod msg_header;
pub mod debug;
pub mod types;

use types::DynError;
use debug::debug_err_append;
use msg_header::{MsgHeader, HEADER_LEN};

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
const KIB64: usize = 65536;

type Thread = JoinHandle<()>;

pub struct SockStdInOutCon {
    kill: Arc<AtomicBool>,
    reset_conn: Arc<AtomicBool>,
    sock_reader_fd_writer: Thread,
    sock_writer_fd_reader: Thread,
}

impl<'a> SockStdInOutCon {
    const SRFW_ERR: &'static str = "Error: SockReaderFdWriter failed to spawn";
    const SWFR_ERR: &'static str = "Error: SockWriterFdReader failed to spawn";

    fn spawn<T, U>(
        stream: Arc<Mutex<UnixStream>>,
        written: Arc<Mutex<T>>,
        read: Arc<Mutex<U>>,
    ) -> Self where
        T: Write + Send + Sync + 'static,
        U: Read + Send + Sync + 'static,
    {
        let kill = Arc::new(AtomicBool::new(false));
        let reset_conn = Arc::new(AtomicBool::new(false));

        let sock_reader_fd_writer = {
            let srfw = SockReaderFdWriter {
                stream: stream.clone(),
                fd: written.clone(),
                kill: kill.clone(),
                reset_conn: reset_conn.clone(),
            };

            thread::Builder::new()
                .name(SockReaderFdWriter::<T>::DEBUG_FNAME.to_string())
                .spawn(move || { srfw.spawn() })  
                .expect(Self::SRFW_ERR)
        };

        let sock_writer_fd_reader = {
            let swfr = SockWriterFdReader {
                stream: stream.clone(),
                fd: read.clone(),
                kill: kill.clone(),
                reset_conn: reset_conn.clone(),
            };

            thread::Builder::new()
                .name(SockWriterFdReader::<U>::DEBUG_FNAME.to_string())
                .spawn(move || { swfr.spawn() })
                .expect(Self::SWFR_ERR)
        };

        Self {
            kill,
            reset_conn,
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
    reset_conn: Arc<AtomicBool>,
}

impl<T: Write> SockReaderFdWriter<T> {
    const DEBUG_FNAME: &str = "SockReaderFdWriter";
    pub fn spawn(self) { 
        let mut buf = [0u8; KIB64];
        let mut buf_len: usize;
        let mut cursor: usize;

        'top: loop {
            if self.kill.load(SeqCst) { panic!() }
            let mut stream_res = self.stream.lock();
            debug_err_append(
                &stream_res,
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME);
            let mut stream = stream_res.unwrap();

            cursor = HEADER_LEN;
            loop {
                match stream.read(&mut buf[cursor..]) {
                    Ok(nb) => { 
                        if nb != 0 {
                            cursor += nb;
                            continue;
                        } else if cursor > HEADER_LEN {
                            break;
                        } else { continue; }
                    }
                    Err(ref e) if e.kind() ==
                        io::ErrorKind::WouldBlock => {
                        if cursor > 0 { break; }
                        drop(stream);
                        stream_res = self.stream.lock();
                        debug_err_append(
                            &stream_res,
                            Self::DEBUG_FNAME,
                            ERR_LOG_DIR_NAME);
                        stream = stream_res.unwrap();
                        continue;
                    }
                    Err(ref e) if e.kind() ==
                        io::ErrorKind::Interrupted => {
                        drop(stream);
                        stream_res = self.stream.lock();
                        debug_err_append(
                            &stream_res,
                            Self::DEBUG_FNAME,
                            ERR_LOG_DIR_NAME);
                        stream = stream_res.unwrap();
                        continue;
                    }
                    Err(ref e) if e.kind() == 
                        io::ErrorKind::ConnectionReset => {
                        self.reset_conn.store(true, SeqCst);
                        drop(stream);
                        stream_res = self.stream.lock();
                        debug_err_append(
                            &stream_res,
                            Self::DEBUG_FNAME,
                            ERR_LOG_DIR_NAME);
                        stream = stream_res.unwrap();
                    }
                    Err(e) => {
                        debug_err_append(
                            &Err::<T, io::Error>(e),
                            Self::DEBUG_FNAME,
                            ERR_LOG_DIR_NAME);
                        continue 'top;
                    }
                }
            }

            drop(stream);
            if cursor == HEADER_LEN { 
                continue 'top;
            }

            buf[..HEADER_LEN].copy_from_slice(
                &MsgHeader::new(cursor as u64).0);
            buf_len = cursor;

            let fd = self.fd.lock();
            debug_err_append(
                &fd,
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME);
            let mut fd = fd.unwrap();

            cursor = 0;
            while cursor < buf_len {
                match fd.write(&buf[cursor..buf_len]) {
                    Ok(n_bytes) => cursor += n_bytes,

                    Err(err) if err.kind() ==
                        io::ErrorKind::Interrupted => continue,

                    //Err(err) if err.kind() == 
                    //    io::ErrorKind::BrokenPipe => 

                    Err(err) => {
                        debug_err_append(
                            &Err::<T, io::Error>(err),
                            Self::DEBUG_FNAME,
                            ERR_LOG_DIR_NAME);
                        continue 'top;
                    }
                }
            }

            let flush_res = fd.flush();
            debug_err_append(
                &flush_res,
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME);
            drop(fd);
        }
    }
}

struct SockWriterFdReader<T: Read> {
    stream: Arc<Mutex<UnixStream>>,
    fd: Arc<Mutex<T>>, 
    kill: Arc<AtomicBool>,
    reset_conn: Arc<AtomicBool>,
}

struct OFIdx {
    start: usize, 
    end: usize,
}

impl<T: Read> SockWriterFdReader<T> {
    const DEBUG_FNAME: &str = "SockWriterFdReader";
    const MSG_KILL_TRIG: &str = "Error: kill flag triggered";

    pub fn spawn(self) {
        let mut buf = [0u8; KIB64];
        let mut header = [0u8; HEADER_LEN];
        let mut overflow: Option<OFIdx> = None;
        let mut cursor = 0usize; 
        let mut msg_len;
        let mut stream;
        let mut stream_res;

        'top: loop {
            if self.kill.load(SeqCst) { 
                panic!("{}", Self::MSG_KILL_TRIG) 
            }

            let fd_res = self.fd.lock();
            debug_err_append(
                &fd_res,
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME);
            let mut fd = fd_res.unwrap();

            if let Some(of_idx) = overflow {
                buf.copy_within(of_idx.start..of_idx.end, 0);
                cursor = of_idx.end - of_idx.start;
                overflow = None;
            }

            while cursor < HEADER_LEN {
                match fd.read(&mut buf[cursor..]) {
                    Ok(nb) => cursor += nb, 

                    Err(err) if err.kind() == 
                        io::ErrorKind::Interrupted => {

                    }

                    Err(err) => {
                        debug_err_append(
                            &Err::<(), io::Error>(err),
                            Self::DEBUG_FNAME,
                            ERR_LOG_DIR_NAME);
                        cursor = 0;
                        continue 'top;
                    }
                }
            }

            // msg_len includes the header in the new implementation
            // this is why HEADER_LEN is not added to msg_len to account
            // for the cursor header bytes which have been read. 
            header.clone_from_slice(&buf[..HEADER_LEN]);
            msg_len = MsgHeader::len(header); 
            while (cursor as u64) < msg_len {
                match fd.read(&mut buf[cursor..]) {
                    Ok(nb) => cursor += nb,

                    Err(err) if err.kind() ==
                        io::ErrorKind::Interrupted => continue,

                    //Err(err) if err.kind() == 
                    //    io::ErrorKind::BrokenPipe =>

                    Err(err) => {
                        debug_err_append(
                            &Err::<(), io::Error>(err),
                            Self::DEBUG_FNAME,
                            ERR_LOG_DIR_NAME);
                        cursor = 0;
                        continue 'top;
                    }
                }
            }

            drop(fd);
            if cursor as u64 > msg_len {
                overflow = Some(OFIdx {
                    start: msg_len as usize,
                    end: cursor });
            } else {
                overflow = None;
            }

            stream_res = self.stream.lock();
            debug_err_append(
                &stream_res,
                Self::DEBUG_FNAME,
                ERR_LOG_DIR_NAME);
            stream = stream_res.unwrap();

            cursor = HEADER_LEN;
            while cursor < (msg_len as usize) {
                match stream.write(&buf[cursor..(msg_len as usize)]) {
                    Ok(nb) => { 
                        cursor += nb;
                        continue;
                    }
                    Err(ref e) if e.kind() ==
                        io::ErrorKind::WouldBlock => {
                        drop(stream);
                        stream_res = self.stream.lock();
                        debug_err_append(
                            &stream_res,
                            Self::DEBUG_FNAME,
                            ERR_LOG_DIR_NAME);
                        stream = stream_res.unwrap();
                        continue;
                    }
                    Err(ref e) if e.kind() ==
                        io::ErrorKind::Interrupted => {
                        drop(stream);
                        stream_res = self.stream.lock();
                        debug_err_append(
                            &stream_res,
                            Self::DEBUG_FNAME,
                            ERR_LOG_DIR_NAME);
                        stream = stream_res.unwrap();
                        continue;
                    }
                    Err(e) => {
                        debug_err_append(
                            &Err::<T, io::Error>(e),
                            Self::DEBUG_FNAME,
                            ERR_LOG_DIR_NAME);
                        continue 'top;
                    }
                }
            } 

            let _ = stream.flush();
            drop(stream);
            cursor = 0;
        }
    }
}

/// Returns a UnixStream with rw timeouts set or an  
/// error, likely a WouldBlock if you have nonblocking set.
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
const _SLEEP_DURATION_CTRL: Duration = Duration::from_secs(3);
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
    pub fn new() -> DynError<Self> {
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
    pub fn new() -> DynError<Self> {
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
        const STREAM_ERR: &str = "Error: failed stream_and_touts, within SockStdInOutCon thread management"; 
        let (written, read) = (
            Arc::new(Mutex::new(written)),
            Arc::new(Mutex::new(read)));
        let stream = stream_and_touts(&self.0)?;
        self.0.set_nonblocking(true)?;
        let stream_ctrl = Arc::new(Mutex::new(stream));
        let thread_ctrl = SockStdInOutCon::spawn(
            stream_ctrl.clone(),
            written.clone(),
            read.clone());

        loop { 
            //thread::sleep(SLEEP_DURATION_CTRL);

            if thread_ctrl.reset_conn.load(SeqCst) {
                let stream = match stream_and_touts(&self.0) {
                    Ok(stream) => stream,
                    Err(err) if err.kind() == 
                        io::ErrorKind::WouldBlock => continue,
                    Err(err) => {
                        debug_err_append(
                            &Err::<T, io::Error>(err),
                            DBG_FNAME_MAIN,
                            ERR_LOG_DIR_NAME);
                        panic!("{}", STREAM_ERR);
                    }
                };

                let Ok(mut stream_lock) = stream_ctrl.lock() else {
                    let mutex_err = Err(anyhow!(MUTEX_ERR).into());
                    debug_err_append(
                        &mutex_err,
                        DBG_FNAME_MAIN,
                        ERR_LOG_DIR_NAME);
                    return mutex_err;
                }; 

                *stream_lock = stream;
                thread_ctrl.reset_conn.store(false, SeqCst); 
            }

            if finish_check(&thread_ctrl) {
                let thread_err = Err(anyhow!(THREAD_ERR).into()); 
                debug_err_append(
                    &thread_err,
                    DBG_FNAME_MAIN,
                    ERR_LOG_DIR_NAME);
                return thread_err;
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
