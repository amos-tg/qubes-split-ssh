mod data;
mod msg_header;
pub mod debug;
pub mod types;

use types::DynError;
use debug::append;
use msg_header::{
    MsgHeader,
    HEADER_LEN,
    flags::*,
    FLAGS_INDEX,
};
use data::CRwLock;

use std::{
    fs,
    env,
    time::Duration,
    ops::{Deref, DerefMut},
    io::{
        self,
        Read,
        Write,
        ErrorKind::{
            Interrupted, 
            WouldBlock,
            TimedOut,
            BrokenPipe,
        },
    },
    net::Shutdown,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::*},
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
const NUM_THREADS: usize = 2;

type Thread = JoinHandle<()>;

#[derive(PartialEq, Clone)]
enum Model {
    Client,
    Server,
}

pub struct SockStdInOutCon {
    kill: Arc<AtomicBool>,
    sock_reader_fd_writer: Thread,
    sock_writer_fd_reader: Thread,
    new_stream: Arc<CRwLock<UnixStream, NUM_THREADS>>,
}

impl<'a> SockStdInOutCon {
    const DEBUG_FNAME: &'static str = "SockStdInOutCon";
    const SRFW_ERR: &'static str = "Error: SockReaderFdWriter failed to spawn";
    const SWFR_ERR: &'static str = "Error: SockWriterFdReader failed to spawn";

    fn spawn<T, U>(
        stream: UnixStream,
        written: T,
        read: U,
        model: Model,
    ) -> Self where
        T: Write + Send + 'static,
        U: Read + Send + 'static,
    {
        let kill = Arc::new(AtomicBool::new(false));

        let new_stream = { 
            let stream = stream.try_clone();
            if let Err(ref e) = stream { 
                kill_thread(&kill, Self::DEBUG_FNAME, &e.to_string());
            }
            Arc::new(CRwLock::<UnixStream, NUM_THREADS>::new(stream.unwrap()))
        };

        let srfw_stream = { 
            let stream = stream.try_clone();
            if let Err(ref e) = stream { 
                kill_thread(&kill, Self::DEBUG_FNAME, &e.to_string());
            }
            stream.unwrap()
        };

        let sock_reader_fd_writer = {
            let srfw = SockReaderFdWriter {
                new_stream: new_stream.clone(),
                stream: srfw_stream,
                fd: written,
                kill: kill.clone(),
                model: model.clone(),
            };

            thread::Builder::new()
                .name(SockReaderFdWriter::<T>::DEBUG_FNAME.to_string())
                .spawn(move || { srfw.spawn() })  
                .expect(Self::SRFW_ERR)
        };

        let sock_writer_fd_reader = {
            let swfr = SockWriterFdReader {
                new_stream: new_stream.clone(),
                stream: stream,
                fd: read,
                kill: kill.clone(),
                model,
            };

            thread::Builder::new()
                .name(SockWriterFdReader::<U>::DEBUG_FNAME.to_string())
                .spawn(move || { swfr.spawn() })
                .expect(Self::SWFR_ERR)
        };

        Self {
            kill,
            sock_reader_fd_writer, 
            sock_writer_fd_reader,
            new_stream,
        }
    }
}

impl Drop for SockStdInOutCon {
    fn drop(&mut self) {
        self.kill.store(true, SeqCst); 
    }
}

const MSG_KILL_TRIG: &str = "Error: kill flag triggered";

fn kill_thread(
    kill: &AtomicBool, 
    dbg_fname: &str,
    msg: &str,
) {
    kill.store(true, SeqCst);
    append(
        msg,
        dbg_fname,
        ERR_LOG_DIR_NAME);
    panic!("{}", msg);
}

/// returns true if the error is Interrupted, WouldBlock, TimedOut, else returns false.
#[inline]
fn is_io_err_minor(err: &io::Error) -> bool {
    match err.kind() {
        Interrupted => true,
        WouldBlock => true,
        TimedOut => true, 
        _ => false, 
    }
}

fn load_new_stream(
    kill: &AtomicBool, debug_fname: &str, 
    new_stream: &CRwLock<UnixStream, NUM_THREADS>, stream: &mut UnixStream,
) {
    if new_stream.count().load(SeqCst) != 0 {
        let new_stream_guard = new_stream.read();
        if let Err(ref e) = new_stream_guard {
            kill_thread(kill, debug_fname, &e.to_string());
        }

        let new_stream_res = (*(new_stream_guard.unwrap())).try_clone();

        if let Err(ref e) = new_stream_res {
            kill_thread(kill, debug_fname, &e.to_string());
        }

        *stream = new_stream_res.unwrap();

        return;    
    } 
} 

struct SockReaderFdWriter<T: Write + Send> {
    new_stream: Arc<CRwLock<UnixStream, NUM_THREADS>>,
    stream: UnixStream,
    fd: T,
    kill: Arc<AtomicBool>,   
    model: Model,
}

impl<T: Write + Send> SockReaderFdWriter<T> {
    const DEBUG_FNAME: &str = "SockReaderFdWriter";

    pub fn spawn(mut self) { 
        let mut buf = [0u8; KIB64];
        let mut msg_len: usize;
        let mut cursor: usize;
        let mut header = MsgHeader::new();

        'reconn: loop {
            load_new_stream(
                &self.kill, Self::DEBUG_FNAME, 
                &self.new_stream, &mut self.stream); 

            if self.model == Model::Client {
                self.send_disconn_msg();
            }
        loop {
            if self.kill.load(SeqCst) { panic!("{}", MSG_KILL_TRIG) }
            cursor = HEADER_LEN;

            match self.stream.read(&mut buf[cursor..]) {
                Ok(nb) => { 
                    if nb != 0 {
                        cursor += nb;
                    } else { 
                        continue 'reconn;
                    } 
                }

                Err(ref e) if is_io_err_minor(e) => {
                    continue; 
                }

                Err(e) => kill_thread(
                    &self.kill, Self::DEBUG_FNAME, &e.to_string()),
            }

            header.update(cursor as u64, NONE);
            buf[..HEADER_LEN].copy_from_slice(&*header);
            msg_len = cursor;
            cursor = 0;

            while cursor < msg_len {
                match self.fd.write(&buf[cursor..msg_len]) {
                    Ok(n_bytes) => cursor += n_bytes,

                    Err(ref e) if is_io_err_minor(e) => continue,

                    Err(e) => kill_thread(
                        &self.kill, Self::DEBUG_FNAME, &e.to_string()),
                }
            }

            let flush_res = self.fd.flush();
            if let Err(e) = flush_res {
                kill_thread(&self.kill, Self::DEBUG_FNAME, &e.to_string());
            }
        }}
    }

    fn send_disconn_msg(&mut self) {
        const EMPTY: u64 = 0;

        let mut cursor = 0; 
        let mut header = MsgHeader::new();  

        header.update(EMPTY, RECONN);
        

        while cursor < HEADER_LEN {
            match self.fd.write(&*header) {
                Ok(nb) => cursor += nb, 

                Err(ref e) if is_io_err_minor(e) => continue, 

                Err(e) => kill_thread(&self.kill, Self::DEBUG_FNAME, &e.to_string()),
            }
        }
    } 
}

struct SockWriterFdReader<T: Read> {
    new_stream: Arc<CRwLock<UnixStream, NUM_THREADS>>,
    stream: UnixStream,
    fd: T, 
    kill: Arc<AtomicBool>,
    model: Model,
}

impl<T: Read + Send> SockWriterFdReader<T> {
    const DEBUG_FNAME: &str = "SockWriterFdReader";

    pub fn spawn(mut self) {
        let mut buf = [0u8; KIB64];
        let mut header = MsgHeader::new();
        let mut cursor: usize;
        let mut msg_len;

        'reconn: loop {
            match self.model {
                Model::Client => {
                    // This can kill the thread on failure
                    load_new_stream(
                        &self.kill, Self::DEBUG_FNAME,
                        &self.new_stream, &mut self.stream);
                }

                Model::Server => {
                    if let Err(e) = self.disconn_ssh_agent() {
                        kill_thread(&self.kill, Self::DEBUG_FNAME, &e.to_string());
                    }   

                    // supposedly ssh-agent connections don't timeout by default 
                    // so it's fine in this case to wait an indeterminate amount
                    // of time for the next connection.
                    if let Err(e) = self.reconn_ssh_agent() {
                        kill_thread(&self.kill, Self::DEBUG_FNAME, &e.to_string());
                    }   
                }
            } 
        loop {
            cursor = 0; 

            if self.kill.load(SeqCst) { 
                panic!("{}", MSG_KILL_TRIG) 
            }

            while cursor < HEADER_LEN {
                match self.fd.read(&mut buf[cursor..]) {
                    Ok(nb) => {
                        if nb != 0 {
                            cursor += nb; 
                        } else {
                            continue;
                        }
                    }

                    Err(ref e) if is_io_err_minor(e) => continue,

                    Err(e) => kill_thread(&self.kill, Self::DEBUG_FNAME, &e.to_string()),
                }
            }

            if header[FLAGS_INDEX] == RECONN {
                continue 'reconn;
            }

            header.clone_from_slice(&buf[..HEADER_LEN]);
            msg_len = header.len() as usize; 

            while cursor < msg_len {
                match self.fd.read(&mut buf[cursor..]) {
                    Ok(nb) => { 
                        if nb != 0 {
                            cursor += nb;
                        } 
                    } 

                    Err(ref e) if is_io_err_minor(e) => continue,

                    Err(e) => kill_thread(&self.kill, Self::DEBUG_FNAME, &e.to_string()),
                }
            }

            cursor = HEADER_LEN;

            while cursor < msg_len {
                match self.stream.write(&buf[cursor..msg_len]) {
                    Ok(nb) => cursor += nb,
                    
                    Err(ref e) if is_io_err_minor(e) => continue,

                    Err(e) if e.kind() == BrokenPipe => continue 'reconn, 

                    Err(e) => kill_thread(&self.kill, Self::DEBUG_FNAME, &e.to_string()),
                }
            } 

            if let Err(e) = self.stream.flush() {
                kill_thread(&self.kill, Self::DEBUG_FNAME, &e.to_string());
            }
        }}
    }

    /// shuts down the read and right halves of the UnixStream
    /// in self.stream.
    #[inline]
    fn disconn_ssh_agent(&mut self) -> io::Result<()> {
        self.stream.shutdown(Shutdown::Both)?; 
        return Ok(());
    }

    /// Gets a new connection to the ssh-agent; blocks until the new 
    /// connection is acquired. Timeouts are applied to the ssh-agent 
    /// with the touts function. The new connection to the ssh-agent
    /// calls replace on self.new_stream to ensure that both of the 
    /// IO manager threads get proper access. 
    fn reconn_ssh_agent(&mut self) -> DynError<()> {
        let sock = conn_ssh_agent()?; 
        touts(&sock)?;
        self.new_stream.replace(sock)?;
        return Ok(());
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
    stream.set_read_timeout(Some(TOUT_SECS))?; 
    stream.set_write_timeout(Some(TOUT_SECS))?;
    return Ok(());
}

const SOCK_VAR: &str = "SSH_AUTH_SOCK";
const THREAD_ERR: &str = "Error: at least one of the threads failed";

fn finish_check(conn: &SockStdInOutCon) -> bool {
    if conn.sock_reader_fd_writer.is_finished() || 
        conn.sock_writer_fd_reader.is_finished() 
    { 
        return true; 
    } else {
        return false;
    }
}

fn conn_ssh_agent() -> DynError<UnixStream> {
    let path = env::var(SOCK_VAR)?;
    let sock = if fs::exists(&path)? {
        UnixStream::connect(&path)?
    } else {
        return Err(anyhow!(
            "Error: ssh-agent socket doesn't exist to connect to"
        ).into());
    };
    return Ok(sock);
}

pub struct SockStream(UnixStream);

impl SockStream {
    // SockStream is used on the vault side
    pub fn new() -> DynError<Self> {
        let sock = conn_ssh_agent()?;
        touts(&sock)?;
        return Ok(Self(sock));
    }
    
    pub fn handle_connections<T, U>(
        self,
        written: T,
        read: U,
    ) -> Result<(), anyhow::Error> where
        T: Write + Send + 'static,
        U: Read + Send + 'static, 
    {
        let handle = SockStdInOutCon::spawn(self.0, written, read, Model::Server);

        loop {
            if finish_check(&handle) { 
                Err(anyhow!(THREAD_ERR))?;
            }
        } 
    }  
}

pub struct SockListener(UnixListener);

impl SockListener {
    const DEBUG_FNAME: &str = "Controller";

    // used by the client side
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
        T: Write + Send + 'static,
        U: Read + Send + 'static, 
    {
        let stream = stream_and_touts(&self.0)?;
        let thread_ctrl = SockStdInOutCon::spawn(stream, written, read, Model::Client);
        self.0.set_nonblocking(true)?;

        let mut conn_queue = Vec::with_capacity(5);
        loop { 
            if finish_check(&thread_ctrl) {
                Err(anyhow!(THREAD_ERR))? 
            }

            match self.0.accept() {
                Ok((conn, _)) => conn_queue.push(conn),

                Err(ref e) if e.kind() == WouldBlock => (),

                Err(e) => kill_thread(
                    &thread_ctrl.kill, Self::DEBUG_FNAME, &e.to_string()),
            }

            if !conn_queue.is_empty() 
                && thread_ctrl.new_stream.count().load(SeqCst) == 0 
            {
                thread_ctrl.new_stream.replace(conn_queue.pop().unwrap())?;
            }
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
