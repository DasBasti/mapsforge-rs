//! Minimal byte-source abstraction so the parser works without `std::io`
//! (e.g. on a microcontroller reading through C file callbacks).
use crate::Result;

/// Sequential big-endian reads.
pub trait ByteRead {
    fn read_bytes(&mut self, buf: &mut [u8]) -> Result<()>;

    fn read_u8(&mut self) -> Result<u8> {
        let mut b = [0u8; 1];
        self.read_bytes(&mut b)?;
        Ok(b[0])
    }

    fn read_u16_be(&mut self) -> Result<u16> {
        let mut b = [0u8; 2];
        self.read_bytes(&mut b)?;
        Ok(u16::from_be_bytes(b))
    }

    fn read_i32_be(&mut self) -> Result<i32> {
        let mut b = [0u8; 4];
        self.read_bytes(&mut b)?;
        Ok(i32::from_be_bytes(b))
    }

    fn read_u32_be(&mut self) -> Result<u32> {
        let mut b = [0u8; 4];
        self.read_bytes(&mut b)?;
        Ok(u32::from_be_bytes(b))
    }

    fn read_u64_be(&mut self) -> Result<u64> {
        let mut b = [0u8; 8];
        self.read_bytes(&mut b)?;
        Ok(u64::from_be_bytes(b))
    }
}

/// A [`ByteRead`] with random access, as needed to jump to tiles.
pub trait ByteSource: ByteRead {
    fn seek_to(&mut self, pos: u64) -> Result<()>;
    fn position(&mut self) -> Result<u64>;
}

#[cfg(feature = "std")]
impl<T: std::io::Read + ?Sized> ByteRead for T {
    fn read_bytes(&mut self, buf: &mut [u8]) -> Result<()> {
        Ok(std::io::Read::read_exact(self, buf)?)
    }
}

#[cfg(feature = "std")]
impl<T: std::io::Read + std::io::Seek + ?Sized> ByteSource for T {
    fn seek_to(&mut self, pos: u64) -> Result<()> {
        std::io::Seek::seek(self, std::io::SeekFrom::Start(pos))?;
        Ok(())
    }

    fn position(&mut self) -> Result<u64> {
        Ok(std::io::Seek::stream_position(self)?)
    }
}
