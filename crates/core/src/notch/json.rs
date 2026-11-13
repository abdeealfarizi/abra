//! Pembaca JSON kecil (RFC 8259) untuk jawaban LRCLIB, tanpa dependensi. Tidak untuk menulis JSON.

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn as_arr(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(a) => Some(a),
            _ => None,
        }
    }
}

struct P<'a> {
    s: &'a [u8],
    i: usize,
    depth: u32,
}

impl P<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }
    fn eat(&mut self, lit: &[u8]) -> bool {
        if self.s[self.i..].starts_with(lit) {
            self.i += lit.len();
            true
        } else {
            false
        }
    }
    fn value(&mut self) -> Option<Json> {
        self.ws();
        let c = *self.s.get(self.i)?;
        match c {
            b'n' => self.eat(b"null").then_some(Json::Null),
            b't' => self.eat(b"true").then_some(Json::Bool(true)),
            b'f' => self.eat(b"false").then_some(Json::Bool(false)),
            b'"' => self.string().map(Json::Str),
            b'[' => self.array(),
            b'{' => self.object(),
            b'-' | b'0'..=b'9' => self.number(),
            _ => None,
        }
    }
    fn number(&mut self) -> Option<Json> {
        let start = self.i;
        while self.i < self.s.len() && matches!(self.s[self.i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
            self.i += 1;
        }
        std::str::from_utf8(&self.s[start..self.i]).ok()?.parse().ok().map(Json::Num)
    }
    fn hex4(&mut self) -> Option<u32> {
        let h = std::str::from_utf8(self.s.get(self.i..self.i + 4)?).ok()?;
        self.i += 4;
        u32::from_str_radix(h, 16).ok()
    }
    fn string(&mut self) -> Option<String> {
        self.i += 1; // "
        let mut out: Vec<u8> = Vec::new();
        loop {
            let c = *self.s.get(self.i)?;
            self.i += 1;
            match c {
                b'"' => return String::from_utf8(out).ok(),
                b'\\' => {
                    let e = *self.s.get(self.i)?;
                    self.i += 1;
                    let ch = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let hi = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&hi) {
                                // pasangan surrogate
                                if !self.eat(b"\\u") {
                                    return None;
                                }
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return None;
                                }
                                0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                            } else {
                                hi
                            };
                            char::from_u32(code)?
                        }
                        _ => return None,
                    };
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                }
                _ => out.push(c),
            }
        }
    }
    fn array(&mut self) -> Option<Json> {
        self.i += 1;
        self.depth += 1;
        if self.depth > 64 {
            return None;
        }
        let mut v = Vec::new();
        self.ws();
        if self.eat(b"]") {
            self.depth -= 1;
            return Some(Json::Arr(v));
        }
        loop {
            v.push(self.value()?);
            self.ws();
            if self.eat(b",") {
                continue;
            }
            if self.eat(b"]") {
                self.depth -= 1;
                return Some(Json::Arr(v));
            }
            return None;
        }
    }
    fn object(&mut self) -> Option<Json> {
        self.i += 1;
        self.depth += 1;
        if self.depth > 64 {
            return None;
        }
        let mut m = Vec::new();
        self.ws();
        if self.eat(b"}") {
            self.depth -= 1;
            return Some(Json::Obj(m));
        }
        loop {
            self.ws();
            if self.s.get(self.i) != Some(&b'"') {
                return None;
            }
            let k = self.string()?;
            self.ws();
            if !self.eat(b":") {
                return None;
            }
            let v = self.value()?;
            m.push((k, v));
            self.ws();
            if self.eat(b",") {
                continue;
            }
            if self.eat(b"}") {
                self.depth -= 1;
                return Some(Json::Obj(m));
            }
            return None;
        }
    }
}

/// Urai satu dokumen JSON; `None` bila tidak sah.
pub fn parse(src: &str) -> Option<Json> {
    let mut p = P { s: src.as_bytes(), i: 0, depth: 0 };
    let v = p.value()?;
    p.ws();
    (p.i == p.s.len()).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_objects_arrays_escapes_and_numbers() {
        let j = parse(r#" { "a": [1, -2.5e1, true, null], "s": "x\"y\\z\né🎵", "o": {} } "#).unwrap();
        assert_eq!(j.get("a").unwrap().as_arr().unwrap()[1].as_f64(), Some(-25.0));
        assert_eq!(j.get("a").unwrap().as_arr().unwrap()[2].as_bool(), Some(true));
        assert_eq!(j.get("s").unwrap().as_str(), Some("x\"y\\z\né🎵"));
        assert_eq!(j.get("o"), Some(&Json::Obj(vec![])));
        assert_eq!(j.get("missing"), None);
    }

    #[test]
    fn rejects_broken_input() {
        for bad in ["", "{", "[1,]", r#"{"a" 1}"#, r#""\ud83c""#, "tru", "{} x"] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }
}
