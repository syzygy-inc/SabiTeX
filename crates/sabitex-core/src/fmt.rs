//! Format files: dumping and undumping (tex.web Part 50, §1299-§1329).
//!
//! tex.web's format is unabashedly system-dependent (§1299 even encourages
//! incompatibility), so this port defines its own: a fixed-width
//! little-endian section stream, identical between 32-bit (wasm) and
//! 64-bit hosts. The *contents* — mem, eqtb, hash, fonts, trie — mirror
//! exactly what §1302-§1327 dump.

/// The codec result; the message names the section that failed.
pub type FmtResult<T> = Result<T, &'static str>;

/// `dump_int` and friends: a growable little-endian byte sink.
#[derive(Default)]
pub struct FmtWriter {
    pub buf: Vec<u8>,
}

impl FmtWriter {
    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn len_of(&mut self, n: usize) {
        self.u64(n as u64);
    }

    pub fn bool(&mut self, v: bool) {
        self.u8(u8::from(v));
    }

    pub fn str(&mut self, s: &str) {
        self.len_of(s.len());
        self.buf.extend_from_slice(s.as_bytes());
    }

    pub fn u16s(&mut self, v: &[u16]) {
        self.len_of(v.len());
        for &x in v {
            self.u16(x);
        }
    }

    pub fn u8s(&mut self, v: &[u8]) {
        self.len_of(v.len());
        self.buf.extend_from_slice(v);
    }

    pub fn i32s(&mut self, v: &[i32]) {
        self.len_of(v.len());
        for &x in v {
            self.i32(x);
        }
    }

    pub fn words(&mut self, v: &[crate::memword::MemoryWord]) {
        self.len_of(v.len());
        for &x in v {
            self.u64(x.bits());
        }
    }
}

/// `undump_int` and friends: the matching reader.
pub struct FmtReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> FmtReader<'a> {
    pub fn new(data: &'a [u8]) -> FmtReader<'a> {
        FmtReader { data, pos: 0 }
    }

    /// Bytes not yet consumed.
    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn take(&mut self, n: usize) -> FmtResult<&'a [u8]> {
        // `n` comes from the file: no arithmetic on it before the bound check.
        if n > self.remaining() {
            return Err("unexpected end of format file");
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn u8(&mut self) -> FmtResult<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn u16(&mut self) -> FmtResult<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    pub fn i32(&mut self) -> FmtResult<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub fn u64(&mut self) -> FmtResult<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    /// A count stored in the file that is checked by the caller against a
    /// fixed expectation (an array size) before anything is allocated. Only
    /// the conversion to `usize` is validated here (no truncation on 32-bit).
    pub fn count(&mut self) -> FmtResult<usize> {
        usize::try_from(self.u64()?).map_err(|_| "count in format file does not fit in memory")
    }

    /// The length of a sequence whose elements take `elem` bytes each. The
    /// length is validated against the bytes that remain in the file
    /// *before* any allocation, so a corrupt or hostile length is a normal
    /// error and never a panic or a huge allocation (T1). `elem` must be
    /// at least 1.
    pub fn seq_len(&mut self, elem: usize) -> FmtResult<usize> {
        debug_assert!(elem >= 1);
        let n = self.count()?;
        let bytes = n
            .checked_mul(elem)
            .ok_or("sequence length in format file overflows")?;
        if bytes > self.remaining() {
            return Err("sequence in format file is longer than the rest of the file");
        }
        Ok(n)
    }

    pub fn bool(&mut self) -> FmtResult<bool> {
        Ok(self.u8()? != 0)
    }

    pub fn str(&mut self) -> FmtResult<String> {
        let n = self.seq_len(1)?;
        let b = self.take(n)?;
        String::from_utf8(b.to_vec()).map_err(|_| "bad string in format file")
    }

    pub fn u16s(&mut self) -> FmtResult<Vec<u16>> {
        let n = self.seq_len(2)?;
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(self.u16()?);
        }
        Ok(v)
    }

    pub fn u8s(&mut self) -> FmtResult<Vec<u8>> {
        let n = self.seq_len(1)?;
        Ok(self.take(n)?.to_vec())
    }

    pub fn i32s(&mut self) -> FmtResult<Vec<i32>> {
        let n = self.seq_len(4)?;
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(self.i32()?);
        }
        Ok(v)
    }

    pub fn words(&mut self) -> FmtResult<Vec<crate::memword::MemoryWord>> {
        let n = self.seq_len(8)?;
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(crate::memword::MemoryWord::from_bits(self.u64()?));
        }
        Ok(v)
    }

    pub fn done(&self) -> FmtResult<()> {
        if self.pos == self.data.len() {
            Ok(())
        } else {
            Err("trailing garbage in format file")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_len(len: u64, payload: &[u8]) -> Vec<u8> {
        let mut v = len.to_le_bytes().to_vec();
        v.extend_from_slice(payload);
        v
    }

    /// T1: a length that does not fit is a normal error, never a panic or an
    /// allocation of that size.
    #[test]
    fn huge_lengths_are_errors_not_panics() {
        for len in [u64::MAX, u64::MAX / 2, 1 << 40, 1 << 32] {
            let d = with_len(len, &[0; 16]);
            assert!(FmtReader::new(&d).u16s().is_err(), "{len}");
            assert!(FmtReader::new(&d).i32s().is_err(), "{len}");
            assert!(FmtReader::new(&d).words().is_err(), "{len}");
            assert!(FmtReader::new(&d).u8s().is_err(), "{len}");
            assert!(FmtReader::new(&d).str().is_err(), "{len}");
        }
    }

    /// The length is checked against the bytes that remain, per element size.
    #[test]
    fn lengths_beyond_the_rest_of_the_file_are_errors() {
        // 3 u16 need 6 bytes; only 4 follow.
        let d = with_len(3, &[0; 4]);
        assert!(FmtReader::new(&d).u16s().is_err());
        // exactly enough
        let d = with_len(2, &[1, 0, 2, 0]);
        assert_eq!(FmtReader::new(&d).u16s().unwrap(), vec![1, 2]);
        // 1 word needs 8 bytes; only 7 follow
        let d = with_len(1, &[0; 7]);
        assert!(FmtReader::new(&d).words().is_err());
        // a string longer than the file
        let d = with_len(5, b"abc");
        assert!(FmtReader::new(&d).str().is_err());
    }

    /// Reads past the end (including at the very end) are errors, and the
    /// reader stays usable afterwards.
    #[test]
    fn short_reads_are_errors() {
        let mut r = FmtReader::new(&[1, 2, 3]);
        assert!(r.i32().is_err());
        assert_eq!(r.u8().unwrap(), 1);
        assert_eq!(r.u16().unwrap(), 0x0302);
        assert!(r.u8().is_err());
        assert!(r.done().is_ok());
    }

    /// Round trip of every writer/reader pair.
    #[test]
    fn writer_and_reader_agree() {
        let mut w = FmtWriter::default();
        w.u8(7);
        w.u16(65535);
        w.i32(-1);
        w.u64(u64::MAX);
        w.bool(true);
        w.str("fmt");
        w.u16s(&[1, 2, 3]);
        w.u8s(&[9]);
        w.i32s(&[-5, 5]);
        w.words(&[crate::memword::MemoryWord::from_bits(0x0102_0304_0506_0708)]);
        let mut r = FmtReader::new(&w.buf);
        assert_eq!(r.u8().unwrap(), 7);
        assert_eq!(r.u16().unwrap(), 65535);
        assert_eq!(r.i32().unwrap(), -1);
        assert_eq!(r.u64().unwrap(), u64::MAX);
        assert!(r.bool().unwrap());
        assert_eq!(r.str().unwrap(), "fmt");
        assert_eq!(r.u16s().unwrap(), vec![1, 2, 3]);
        assert_eq!(r.u8s().unwrap(), vec![9]);
        assert_eq!(r.i32s().unwrap(), vec![-5, 5]);
        assert_eq!(r.words().unwrap()[0].bits(), 0x0102_0304_0506_0708);
        assert!(r.done().is_ok());
    }
}
