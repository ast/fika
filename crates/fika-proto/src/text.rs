//! Text payload coding (SPEC §9.4): a 32-bit binary arithmetic coder
//! (Witten–Neal–Cleary style, bit output) driving an order-1 adaptive
//! frequency model over a ~120 symbol alphabet, with ESC/REP for code
//! points outside it and EOT as terminator.

use crate::error::ProtoError;
use crate::prior::PRIOR;

pub const SYM_EOT: usize = 0;
pub const SYM_ESC: usize = 1;
pub const SYM_REP: usize = 2;
pub const SYM_NL: usize = 3;
const ASCII_FIRST: usize = 4; // index of ' '
const ASCII_COUNT: usize = 95; // 0x20..=0x7E
const EXTRA: &[char] = &[
    'å', 'ä', 'ö', 'Å', 'Ä', 'Ö', 'é', 'É', 'ü', 'Ü', 'ø', 'Ø', 'æ', 'Æ', '€', '–', '“', '”', '’',
    '…',
];
pub const ALPHABET: usize = ASCII_FIRST + ASCII_COUNT + EXTRA.len(); // 119
const MODEL_SIZE: usize = 128;
const INCREMENT: u32 = 24;
const MAX_TOTAL: u32 = 4096;
const CODEPOINT_BITS: usize = 21;

pub fn symbol_of(c: char) -> Option<usize> {
    if c == '\n' {
        return Some(SYM_NL);
    }
    let u = c as u32;
    if (0x20..=0x7E).contains(&u) {
        return Some(ASCII_FIRST + (u - 0x20) as usize);
    }
    EXTRA
        .iter()
        .position(|&e| e == c)
        .map(|i| ASCII_FIRST + ASCII_COUNT + i)
}

pub fn char_of(sym: usize) -> Option<char> {
    if sym == SYM_NL {
        return Some('\n');
    }
    if (ASCII_FIRST..ASCII_FIRST + ASCII_COUNT).contains(&sym) {
        return char::from_u32(0x20 + (sym - ASCII_FIRST) as u32);
    }
    EXTRA
        .get(sym.checked_sub(ASCII_FIRST + ASCII_COUNT)?)
        .copied()
}

/// Order-1 adaptive model: one frequency table per previous symbol.
struct Model {
    freq: Vec<[u32; MODEL_SIZE]>,
    total: Vec<u32>,
}

impl Model {
    fn new() -> Self {
        let mut freq = vec![[0u32; MODEL_SIZE]; MODEL_SIZE];
        let mut total = vec![0u32; MODEL_SIZE];
        for ctx in 0..MODEL_SIZE {
            let src = &PRIOR[ctx.min(ALPHABET - 1)];
            for s in 0..ALPHABET {
                freq[ctx][s] = src[s] as u32;
                total[ctx] += src[s] as u32;
            }
        }
        Self { freq, total }
    }

    fn range(&self, ctx: usize, sym: usize) -> (u32, u32, u32) {
        let row = &self.freq[ctx];
        let low: u32 = row[..sym].iter().sum();
        (low, low + row[sym], self.total[ctx])
    }

    fn find(&self, ctx: usize, target: u32) -> (usize, u32, u32) {
        let row = &self.freq[ctx];
        let mut low = 0;
        for (sym, &f) in row.iter().enumerate() {
            if target < low + f {
                return (sym, low, low + f);
            }
            low += f;
        }
        unreachable!("target within total")
    }

    fn update(&mut self, ctx: usize, sym: usize) {
        self.freq[ctx][sym] += INCREMENT;
        self.total[ctx] += INCREMENT;
        if self.total[ctx] > MAX_TOTAL {
            let mut t = 0;
            for f in self.freq[ctx].iter_mut() {
                if *f > 0 {
                    *f = (*f).div_ceil(2);
                }
                t += *f;
            }
            self.total[ctx] = t;
        }
    }
}

const PRECISION: u32 = 32;
const TOP: u64 = 1 << PRECISION;
const HALF: u64 = TOP / 2;
const QUARTER: u64 = TOP / 4;
const MASK: u64 = TOP - 1;

struct Encoder {
    low: u64,
    high: u64,
    pending: u32,
    out: Vec<u8>,
}

impl Encoder {
    fn new() -> Self {
        Self {
            low: 0,
            high: MASK,
            pending: 0,
            out: Vec::new(),
        }
    }

    fn emit(&mut self, bit: u8) {
        self.out.push(bit);
        for _ in 0..self.pending {
            self.out.push(1 - bit);
        }
        self.pending = 0;
    }

    fn encode(&mut self, low: u32, high: u32, total: u32) {
        let range = self.high - self.low + 1;
        self.high = self.low + range * high as u64 / total as u64 - 1;
        self.low += range * low as u64 / total as u64;
        loop {
            if self.high < HALF {
                self.emit(0);
            } else if self.low >= HALF {
                self.emit(1);
                self.low -= HALF;
                self.high -= HALF;
            } else if self.low >= QUARTER && self.high < 3 * QUARTER {
                self.pending += 1;
                self.low -= QUARTER;
                self.high -= QUARTER;
            } else {
                break;
            }
            self.low <<= 1;
            self.high = (self.high << 1) | 1;
        }
    }

    fn encode_raw(&mut self, value: u32, nbits: usize) {
        for i in (0..nbits).rev() {
            let b = (value >> i) & 1;
            self.encode(b, b + 1, 2);
        }
    }

    fn finish(mut self) -> Vec<u8> {
        self.pending += 1;
        if self.low < QUARTER {
            self.emit(0);
        } else {
            self.emit(1);
        }
        self.out
    }
}

struct Decoder<'a> {
    bits: &'a [u8],
    pos: usize,
    low: u64,
    high: u64,
    value: u64,
}

impl<'a> Decoder<'a> {
    fn new(bits: &'a [u8]) -> Self {
        let mut d = Self {
            bits,
            pos: 0,
            low: 0,
            high: MASK,
            value: 0,
        };
        for _ in 0..PRECISION {
            d.value = (d.value << 1) | d.next_bit();
        }
        d
    }

    fn next_bit(&mut self) -> u64 {
        let b = self.bits.get(self.pos).copied().unwrap_or(0) as u64;
        self.pos += 1;
        b
    }

    fn target(&self, total: u32) -> u32 {
        let range = self.high - self.low + 1;
        (((self.value - self.low + 1) * total as u64 - 1) / range) as u32
    }

    fn consume(&mut self, low: u32, high: u32, total: u32) {
        let range = self.high - self.low + 1;
        self.high = self.low + range * high as u64 / total as u64 - 1;
        self.low += range * low as u64 / total as u64;
        loop {
            if self.high < HALF {
            } else if self.low >= HALF {
                self.low -= HALF;
                self.high -= HALF;
                self.value -= HALF;
            } else if self.low >= QUARTER && self.high < 3 * QUARTER {
                self.low -= QUARTER;
                self.high -= QUARTER;
                self.value -= QUARTER;
            } else {
                break;
            }
            self.low <<= 1;
            self.high = (self.high << 1) | 1;
            self.value = (self.value << 1) | self.next_bit();
        }
    }

    fn decode_raw(&mut self, nbits: usize) -> u32 {
        let mut v = 0u32;
        for _ in 0..nbits {
            let b = self.target(2);
            self.consume(b, b + 1, 2);
            v = (v << 1) | b;
        }
        v
    }

    /// Bits consumed so far beyond the input length indicates runaway.
    fn overrun(&self) -> bool {
        self.pos > self.bits.len() + PRECISION as usize + 8
    }
}

/// Encode text to a bit vector including EOT and flush.
pub fn encode(text: &str) -> Vec<u8> {
    let mut model = Model::new();
    let mut enc = Encoder::new();
    let mut ctx = SYM_EOT;
    let mut last_escaped: Option<char> = None;
    let emit = |model: &mut Model, enc: &mut Encoder, ctx: &mut usize, sym: usize| {
        let (lo, hi, tot) = model.range(*ctx, sym);
        enc.encode(lo, hi, tot);
        model.update(*ctx, sym);
        *ctx = sym;
    };
    for c in text.chars() {
        match symbol_of(c) {
            Some(sym) => emit(&mut model, &mut enc, &mut ctx, sym),
            None if last_escaped == Some(c) => emit(&mut model, &mut enc, &mut ctx, SYM_REP),
            None => {
                emit(&mut model, &mut enc, &mut ctx, SYM_ESC);
                enc.encode_raw(c as u32, CODEPOINT_BITS);
                last_escaped = Some(c);
            }
        }
    }
    emit(&mut model, &mut enc, &mut ctx, SYM_EOT);
    enc.finish()
}

/// Decode a bit vector (zeros assumed past its end) up to EOT.
pub fn decode(bits: &[u8]) -> Result<String, ProtoError> {
    let mut model = Model::new();
    let mut dec = Decoder::new(bits);
    let mut ctx = SYM_EOT;
    let mut out = String::new();
    let mut last_escaped: Option<char> = None;
    loop {
        if dec.overrun() || out.len() > 4096 {
            return Err(ProtoError::Text("no EOT"));
        }
        let (_, _, tot) = model.range(ctx, 0);
        let target = dec.target(tot);
        let (sym, lo, hi) = model.find(ctx, target);
        dec.consume(lo, hi, tot);
        model.update(ctx, sym);
        ctx = sym;
        match sym {
            SYM_EOT => return Ok(out),
            SYM_ESC => {
                let cp = dec.decode_raw(CODEPOINT_BITS);
                let c = char::from_u32(cp).ok_or(ProtoError::Text("bad code point"))?;
                out.push(c);
                last_escaped = Some(c);
            }
            SYM_REP => out.push(last_escaped.ok_or(ProtoError::Text("REP without ESC"))?),
            s => out.push(char_of(s).ok_or(ProtoError::Text("symbol outside alphabet"))?),
        }
    }
}

/// Raw fallback (SPEC §9.4): UTF-8 bytes then a zero byte.
pub fn encode_raw(text: &str) -> Vec<u8> {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    fika_modem::symbols::bytes_to_bits(&bytes)
}

pub fn decode_raw(bits: &[u8]) -> Result<String, ProtoError> {
    let bytes = fika_modem::symbols::bits_to_bytes(bits);
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8(bytes[..end].to_vec()).map_err(|_| ProtoError::Text("invalid UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alphabet_size() {
        assert_eq!(ALPHABET, 119);
        for s in 3..ALPHABET {
            assert_eq!(symbol_of(char_of(s).unwrap()), Some(s));
        }
    }

    #[test]
    fn roundtrip_various() {
        let texts = [
            "",
            "hej",
            "Hej Albin, hur är det med antennen? 73 de SM6WJM",
            "Kaffe på fika-frekvensen kl 19 ☕☕ ok?",
            "The quick brown fox jumps over the lazy dog. 1234567890 !?\n",
            "emoji 🙂🙂🙂 and 日本語",
        ];
        for t in texts {
            let bits = encode(t);
            let back = decode(&bits).unwrap_or_else(|e| panic!("{t:?}: {e}"));
            assert_eq!(back, t);
            // Zero padding after the flush must not change the result.
            let mut padded = bits.clone();
            padded.extend([0u8; 300]);
            assert_eq!(decode(&padded).unwrap(), t);
            eprintln!(
                "{:>5} bits for {:>3} chars ({:.2} b/c): {t:?}",
                bits.len(),
                t.chars().count(),
                bits.len() as f64 / t.chars().count().max(1) as f64
            );
        }
    }

    #[test]
    fn prior_gives_under_four_bits_per_char_on_chat_text() {
        let t = "Hej, hör dig bra här. Antennen är uppe igen, kör 10 W på 40 m. 73 de SM6WJM";
        let bits = encode(t);
        let bpc = bits.len() as f64 / t.chars().count() as f64;
        assert!(bpc < 4.0, "{bpc:.2} bits per char");
    }

    #[test]
    fn adaptive_model_beats_uniform_on_repetitive_text() {
        let t = "the same words again and again and again and again and again and again";
        let bits = encode(t);
        let uniform = (t.chars().count() as f64 * (ALPHABET as f64).log2()) as usize;
        assert!(bits.len() < uniform, "{} vs uniform {uniform}", bits.len());
    }

    #[test]
    fn raw_roundtrip() {
        let t = "åäö raw";
        assert_eq!(decode_raw(&encode_raw(t)).unwrap(), t);
    }
}
