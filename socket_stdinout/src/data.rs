use anyhow::anyhow;
use std::{
    sync::{
        RwLock,
        atomic::{AtomicUsize, Ordering::*},
        LockResult,
        RwLockReadGuard,
    },
};

/// the atomic counter from ref_count() needs to be 
/// zero in order to get a mutable reference to the 
/// data. The counter is reset to NUM_RX (number of 
/// receivers) whenever a mutable reference to the
/// data is taken. 
pub struct CRwLock<T, const NUM_RX: usize> {
    data: RwLock<T>,
    counter: AtomicUsize,
}

impl<T, const NUM_RX: usize> CRwLock<T, NUM_RX> {
    /// Counter starts at zero
    pub fn new(data: T) -> Self {
        let data = RwLock::new(data);
        let counter = AtomicUsize::new(0usize);
        return Self { data, counter };
    }

    #[inline]
    pub fn read(&self) -> LockResult<RwLockReadGuard<'_, T>> {
        return self.data.read(); 
    }

    /// replaces the data inside the RwLock<T> with the new: T argument
    /// Returns an anyhow::Error "Error: Poisened Mutex" if the RwLock
    /// is poisened.
    /// 
    /// This function blocks until it writes the new: T value into
    /// the inner RwLock<T>.
    #[inline]
    pub fn replace(&self, new: T) -> Result<(), anyhow::Error> {
        if self.counter.load(SeqCst) != 0 {
            return Ok(());
        }  

        let _ = self.counter.swap(NUM_RX, SeqCst);

        match self.data.write() {
            Ok(mut mref_data) => *mref_data = new,
            Err(_) => return Err(anyhow!("Error: Poisoned Mutex")),
        };

        return Ok(());
    }

    #[inline]
    pub fn count(&self) -> &AtomicUsize {
        return &self.counter;
    }
}
