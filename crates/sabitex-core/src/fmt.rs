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

    fn take(&mut self, n: usize) -> FmtResult<&'a [u8]> {
        if self.pos + n > self.data.len() {
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

    pub fn seq_len(&mut self) -> FmtResult<usize> {
        Ok(self.u64()? as usize)
    }

    pub fn bool(&mut self) -> FmtResult<bool> {
        Ok(self.u8()? != 0)
    }

    pub fn str(&mut self) -> FmtResult<String> {
        let n = self.seq_len()?;
        let b = self.take(n)?;
        String::from_utf8(b.to_vec()).map_err(|_| "bad string in format file")
    }

    pub fn u16s(&mut self) -> FmtResult<Vec<u16>> {
        let n = self.seq_len()?;
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(self.u16()?);
        }
        Ok(v)
    }

    pub fn u8s(&mut self) -> FmtResult<Vec<u8>> {
        let n = self.seq_len()?;
        Ok(self.take(n)?.to_vec())
    }

    pub fn i32s(&mut self) -> FmtResult<Vec<i32>> {
        let n = self.seq_len()?;
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(self.i32()?);
        }
        Ok(v)
    }

    pub fn words(&mut self) -> FmtResult<Vec<crate::memword::MemoryWord>> {
        let n = self.seq_len()?;
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
