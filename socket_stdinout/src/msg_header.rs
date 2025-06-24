use std::ops::{Deref, DerefMut};

pub const HEADER_LEN: usize = 8;
pub struct MsgHeader(pub [u8; HEADER_LEN]);

impl MsgHeader {
    pub fn new(len: u64) -> Self {
        let mut header = [0u8; HEADER_LEN];
        let mut shift_count;

        #[cfg(target_endian = "big")] {
            shift_count = 56u8;
            for idx in 0..HEADER_LEN {
                header[idx] = (len >> shift_count) as u8;            
                shift_count -= 8;
            }
        }

        #[cfg(target_endian = "little")] {
            shift_count = 0u8;
            for idx in 0..HEADER_LEN {
                header[idx] = (len >> shift_count) as u8;
                shift_count += 8;
            }
        }

        dbg!(
            "MsgHeader::new constructed header",
            header,
            "calculated length (Self::len)",
            Self::len(&header),
        );

        return Self(header);
    } 

    pub fn len(header: impl AsRef<[u8]>) -> u64 {
        let mut total: u64 = 0;
        let mut shift_count = 56;
        let header = header.as_ref();

        for byte in 0..(HEADER_LEN - 1) {
            total |= header[byte] as u64;
            total >>= shift_count;
            shift_count -= 8;
        } 

        dbg!("MsgHeader::len", total, "\n\n");
        return total;
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
