#[cfg(test)]
mod msg_header_tests;

use std::ops::{Deref, DerefMut};

pub mod flags {
    pub const NONE: u8 = 0;
    pub const RECONN: u8 = 1;
  //pub const example: u8 = 1 << 1; 
  //pub const example: u8 = 1 << 2; 
}

// in bytes
pub const HEADER_LEN: usize = 9;
pub const FLAGS_INDEX: usize = 8;
pub const LENGTH_LEN: usize = 8;

pub struct MsgHeader(pub [u8; HEADER_LEN]);

impl MsgHeader {
    pub fn new() -> Self {
        return Self([0u8; HEADER_LEN]);
    }

    pub fn update(&mut self, len: u64, flags: u8) {
        #[cfg(target_endian = "little")]
        self.0[..LENGTH_LEN].copy_from_slice(&len.to_le_bytes());

        #[cfg(target_endian = "big")]
        self.0[..LENGTH_LEN].copy_from_slice(&len.to_be_bytes());

        self.0[FLAGS_INDEX] = flags;
    } 

    pub fn len(&self) -> u64 {
        unsafe {
            let length_bytes = self[..LENGTH_LEN].as_ptr() as *const [u8; LENGTH_LEN];

            #[cfg(target_endian = "little")]
            return u64::from_le_bytes(*length_bytes);

            #[cfg(target_endian = "big")]
            return u64::from_be_bytes(*length_bytes);
        }
    }
}

impl Deref for MsgHeader {
    type Target = [u8; HEADER_LEN];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for MsgHeader {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
