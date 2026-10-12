//! The most recent output of a terminal, capped in size.

/// Keeps roughly the last `cap` bytes. The cut is moved to a line boundary so a restored
/// terminal does not start in the middle of an escape sequence.
#[derive(Debug)]
pub struct Scrollback {
    buf: Vec<u8>,
    cap: usize,
}

impl Scrollback {
    pub fn new(cap: usize) -> Self {
        Self {
            buf: Vec::new(),
            cap,
        }
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
        // Trim in batches, not on every push, to keep appends cheap.
        if self.buf.len() > self.cap + self.cap / 2 {
            let mut cut = self.buf.len() - self.cap;
            if let Some(newline) = self.buf[cut..].iter().position(|&b| b == b'\n') {
                cut += newline + 1;
            }
            self.buf.drain(..cut);
        }
    }

    /// The kept output, at most `cap` bytes.
    pub fn snapshot(&self) -> Vec<u8> {
        if self.buf.len() <= self.cap {
            return self.buf.clone();
        }
        let mut cut = self.buf.len() - self.cap;
        if let Some(newline) = self.buf[cut..].iter().position(|&b| b == b'\n') {
            cut += newline + 1;
        }
        self.buf[cut..].to_vec()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_everything_under_the_cap() {
        let mut s = Scrollback::new(100);
        s.push(b"hello\n");
        s.push(b"world\n");
        assert_eq!(s.snapshot(), b"hello\nworld\n");
        assert!(!s.is_empty());
    }

    #[test]
    fn never_returns_more_than_the_cap() {
        let mut s = Scrollback::new(1000);
        for i in 0..2000 {
            s.push(format!("line {i}\n").as_bytes());
        }
        let snap = s.snapshot();
        assert!(snap.len() <= 1000, "{}", snap.len());
        assert!(snap.len() > 800, "keeps most of the budget: {}", snap.len());
        assert!(String::from_utf8(snap).unwrap().ends_with("line 1999\n"));
    }

    #[test]
    fn cuts_at_a_line_boundary() {
        let mut s = Scrollback::new(50);
        for i in 0..100 {
            s.push(format!("row-{i:03}\n").as_bytes());
        }
        let text = String::from_utf8(s.snapshot()).unwrap();
        assert!(text.starts_with("row-"), "starts mid line: {text:?}");
    }

    #[test]
    fn a_single_long_line_is_still_capped() {
        let mut s = Scrollback::new(100);
        s.push(&vec![b'x'; 10_000]);
        assert!(s.snapshot().len() <= 100);
    }

    #[test]
    fn empty_by_default() {
        let s = Scrollback::new(10);
        assert!(s.is_empty());
        assert!(s.snapshot().is_empty());
    }
}
