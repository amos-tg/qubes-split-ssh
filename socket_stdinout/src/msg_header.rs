#[cfg(test)]
mod msg_header_tests;

use std::ops::{Deref, DerefMut};

pub const HEADER_LEN: usize = 8;
pub struct MsgHeader(pub [u8; HEADER_LEN]);

impl MsgHeader {
    pub fn new(len: u64) -> Self {
        #[cfg(target_endian = "little")]
        return MsgHeader(len.to_le_bytes());

        #[cfg(target_endian = "big")]
        return MsgHeader(len.to_be_bytes());
    } 

    pub fn len(header: [u8; 8]) -> u64 {
        #[cfg(target_endian = "little")]
        return u64::from_le_bytes(header);

        #[cfg(target_endian = "big")]
        return u64::from_be_bytes(header);
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
