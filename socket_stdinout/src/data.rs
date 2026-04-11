use anyhow::anyhow;
use std::{
    sync::{
        RwLock,
        atomic::{AtomicUsize, Ordering::*},
        LockResult,
        RwLockReadGuard,
        TryLockError,
    },
};

pub struct CRwLock<T> {
    data: RwLock<T>,
    counter: AtomicUsize,
    reset: usize,
}

impl<T> CRwLock<T> {
    pub fn new(data: T, num_recievers: usize) -> Self {
        let data = RwLock::new(data);
        let counter = AtomicUsize::new(num_recievers);
        return Self { data, counter, reset: num_recievers };
    }

    pub fn read(&self) -> Option<LockResult<RwLockReadGuard<'_, T>>> {
        let current: usize = self.counter.load(SeqCst); 
        if current == 0 { return None; }
        
        // this sucks... 
        match self.counter
            .compare_exchange(current, current - 1, SeqCst, Relaxed)
        {
            Ok(_) => (),
            Err(_) => (),
        }

        return Some(self.data.read()); 
    }

    /// Replaces the data inside the container if the counter == 0.
    /// Otherwise, does nothing and returns early. Function is blocking
    /// and handles multiple replacements at the same time, pre-empted 
    /// replacers return early having done nothing.
    pub fn replace(&self, new: T) -> Result<(), anyhow::Error> {
        loop { 
            if self.counter.load(SeqCst) != 0 {
                return Ok(());
            }  

            match self.data.try_write() {
                Ok(mut mref_data) => *mref_data = new,
                Err(TryLockError::WouldBlock) => continue,
                Err(TryLockError::Poisoned(_)) => 
                    return Err(anyhow!("Error: Poisoned Mutex")),
            };

            break;
        }

        match self.counter.compare_exchange(0, self.reset, SeqCst, Relaxed) {
            Ok(_) => (),
            Err(_) => return Ok(()),
        }

        return Ok(());
    }
}
