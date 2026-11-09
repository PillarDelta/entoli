//! Minimal deterministic byte writer/reader. Fixed-width little-endian integers,
//! u32-length-prefixed UTF-8 strings, one-byte tags.

pub struct Writer {
    pub buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Writer { buf: Vec::new() }
    }
    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i64(&mut self, v: i64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn str(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.buf.extend_from_slice(s.as_bytes());
    }
    pub fn raw(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }
}

pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadError(pub String);

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }
    pub fn done(&self) -> bool {
        self.pos == self.buf.len()
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], ReadError> {
        if self.pos + n > self.buf.len() {
            return Err(ReadError(format!(
                "truncated: need {n} bytes at {}, have {}",
                self.pos,
                self.buf.len() - self.pos
            )));
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    pub fn u8(&mut self) -> Result<u8, ReadError> {
        Ok(self.take(1)?[0])
    }
    pub fn u32(&mut self) -> Result<u32, ReadError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn i64(&mut self) -> Result<i64, ReadError> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub fn str(&mut self) -> Result<String, ReadError> {
        let n = self.u32()? as usize;
        let b = self.take(n)?;
        String::from_utf8(b.to_vec()).map_err(|e| ReadError(format!("bad utf8: {e}")))
    }
    pub fn raw(&mut self, n: usize) -> Result<&'a [u8], ReadError> {
        self.take(n)
    }
}
