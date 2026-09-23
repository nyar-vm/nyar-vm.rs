use crate::encoding::TextEncoding;

/// 码点迭代器，在字节切片上按指定编码逐个读取字符。
/// 对无效序列使用 U+FFFD 替换字符，不会 panic。
pub struct CodePointIter<'a> {
    bytes: &'a [u8],
    encoding: TextEncoding,
    pos: usize,
    pending_surrogate: Option<u16>,
}

impl<'a> CodePointIter<'a> {
    /// 使用给定的字节切片和编码创建迭代器。
    pub fn new(bytes: &'a [u8], encoding: TextEncoding) -> Self {
        Self { bytes, encoding, pos: 0, pending_surrogate: None }
    }

    /// 将迭代器定位到指定的字节偏移量。
    pub fn seek(&mut self, byte_offset: usize) {
        self.pos = byte_offset;
        self.pending_surrogate = None;
    }

    /// 尝试读取下一个字符。
    ///
    /// 返回 `(byte_len, ch)`；到达末尾时返回 `None`。
    pub fn try_next(&mut self) -> Option<(usize, u32)> {
        if let Some(surrogate) = self.pending_surrogate.take() {
            return Some((0, u32::from(surrogate)));
        }

        if self.pos >= self.bytes.len() {
            return None;
        }

        let (byte_len, ch) = match self.encoding {
            TextEncoding::Ascii => self.decode_ascii(),
            TextEncoding::Utf8 => self.decode_utf8(),
            TextEncoding::Utf16Le => self.decode_utf16_le(),
            TextEncoding::Utf16Be => self.decode_utf16_be(),
        };

        self.pos += byte_len;
        Some((byte_len, ch))
    }

    fn decode_ascii(&self) -> (usize, u32) {
        let b = self.bytes[self.pos];
        if b <= 0x7F { (1, u32::from(b)) } else { (1, 0xFFFD) }
    }

    fn decode_utf8(&mut self) -> (usize, u32) {
        let remaining = &self.bytes[self.pos..];
        match std::str::from_utf8(remaining) {
            Ok(text) => {
                let ch = text.chars().next().unwrap_or('\u{FFFD}');
                let byte_len = ch.len_utf8();
                if (ch as u32) > 0xFFFF {
                    let (hi, lo, has_pair) = {
                        let mut buf = [0u16; 2];
                        let rest_len = ch.encode_utf16(&mut buf).len();
                        let written_len = buf.len() - rest_len;
                        (buf[0], buf[1], written_len >= 2)
                    };
                    if has_pair {
                        self.pending_surrogate = Some(lo);
                    }
                    return (byte_len, u32::from(hi));
                }
                (byte_len, ch as u32)
            }
            Err(err) => (err.valid_up_to().max(1), 0xFFFD),
        }
    }

    fn decode_utf16_le(&mut self) -> (usize, u32) {
        let remaining = self.bytes.len() - self.pos;
        if remaining < 2 {
            return (remaining, 0xFFFD);
        }
        let u16 = u16::from_le_bytes([self.bytes[self.pos], self.bytes[self.pos + 1]]);
        self.decode_utf16_code_unit(u16, remaining)
    }

    fn decode_utf16_be(&mut self) -> (usize, u32) {
        let remaining = self.bytes.len() - self.pos;
        if remaining < 2 {
            return (remaining, 0xFFFD);
        }
        let u16 = u16::from_be_bytes([self.bytes[self.pos], self.bytes[self.pos + 1]]);
        self.decode_utf16_code_unit(u16, remaining)
    }

    fn decode_utf16_code_unit(&self, u16: u16, remaining: usize) -> (usize, u32) {
        if (0xD800..=0xDBFF).contains(&u16) {
            if remaining < 4 {
                return (remaining, 0xFFFD);
            }
            let low = match self.encoding {
                TextEncoding::Utf16Le => u16::from_le_bytes([self.bytes[self.pos + 2], self.bytes[self.pos + 3]]),
                TextEncoding::Utf16Be => u16::from_be_bytes([self.bytes[self.pos + 2], self.bytes[self.pos + 3]]),
                _ => unreachable!(),
            };
            if (0xDC00..=0xDFFF).contains(&low) { (4, u32::from(u16)) } else { (2, 0xFFFD) }
        }
        else if (0xDC00..=0xDFFF).contains(&u16) {
            (2, 0xFFFD)
        }
        else {
            (2, u32::from(u16))
        }
    }
}
