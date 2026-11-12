//! Pencarian Spotlight: skor fuzzy nama aplikasi dan kalkulator ekspresi sederhana. Murni.

/// Skor kecocokan `query` terhadap `name` (lebih besar lebih baik), `None` bila tidak cocok.
/// Urutan prioritas: sama persis > awalan nama > awalan kata > inisial kata > substring > subsekuens.
pub fn score(query: &str, name: &str) -> Option<u32> {
    let q: Vec<char> = query.trim().to_lowercase().chars().collect();
    if q.is_empty() {
        return None;
    }
    let n_low = name.to_lowercase();
    let n: Vec<char> = n_low.chars().collect();
    let qs: String = q.iter().collect();
    if n_low == qs {
        return Some(1000);
    }
    if n_low.starts_with(&qs) {
        return Some(900 - (n.len() as u32).min(100));
    }
    // awalan kata
    let words: Vec<&str> = n_low.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    if let Some(i) = words.iter().position(|w| w.starts_with(&qs)) {
        return Some(800 - (i as u32 * 20).min(100));
    }
    // inisial kata ("vsc" -> Visual Studio Code)
    let initials: String = words.iter().filter_map(|w| w.chars().next()).collect();
    if initials.starts_with(&qs) {
        return Some(700);
    }
    if let Some(pos) = n_low.find(&qs) {
        return Some(600 - (pos as u32).min(100));
    }
    // subsekuens
    let mut qi = 0;
    let mut gaps = 0u32;
    let mut last: Option<usize> = None;
    for (i, c) in n.iter().enumerate() {
        if qi < q.len() && *c == q[qi] {
            if let Some(l) = last {
                gaps += (i - l - 1) as u32;
            }
            last = Some(i);
            qi += 1;
        }
    }
    (qi == q.len()).then(|| 400u32.saturating_sub(gaps.min(300)))
}

/// Indeks `names` yang cocok, diurutkan skor menurun lalu nama menaik, maksimal `limit`.
pub fn rank(query: &str, names: &[String], limit: usize) -> Vec<usize> {
    let mut v: Vec<(usize, u32)> = names.iter().enumerate().filter_map(|(i, n)| score(query, n).map(|s| (i, s))).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| names[a.0].to_lowercase().cmp(&names[b.0].to_lowercase())));
    v.into_iter().take(limit).map(|(i, _)| i).collect()
}

/// Hitung ekspresi aritmetika (+ - * / ^, kurung, desimal titik atau koma, minus unary, "x" sebagai kali).
/// `None` bila bukan ekspresi.
pub fn eval(expr: &str) -> Option<f64> {
    let s: Vec<char> = expr
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            ',' => '.',
            '\u{00D7}' | 'x' | 'X' => '*',
            '\u{00F7}' => '/',
            c => c,
        })
        .collect();
    if s.is_empty() || !s.iter().any(|c| "+-*/^".contains(*c)) || !s.iter().any(|c| c.is_ascii_digit()) {
        return None;
    }
    if !s.iter().all(|c| c.is_ascii_digit() || "+-*/^().".contains(*c)) {
        return None;
    }
    let mut p = Parser { s: &s, i: 0 };
    let v = p.expr()?;
    (p.i == s.len() && v.is_finite()).then_some(v)
}

struct Parser<'a> {
    s: &'a [char],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }
    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        while let Some(c) = self.peek() {
            if c == '+' || c == '-' {
                self.i += 1;
                let r = self.term()?;
                v = if c == '+' { v + r } else { v - r };
            } else {
                break;
            }
        }
        Some(v)
    }
    fn term(&mut self) -> Option<f64> {
        let mut v = self.power()?;
        while let Some(c) = self.peek() {
            if c == '*' || c == '/' {
                self.i += 1;
                let r = self.power()?;
                v = if c == '*' { v * r } else { v / r };
            } else {
                break;
            }
        }
        Some(v)
    }
    fn power(&mut self) -> Option<f64> {
        let b = self.unary()?;
        if self.peek() == Some('^') {
            self.i += 1;
            let e = self.power()?;
            return Some(b.powf(e));
        }
        Some(b)
    }
    fn unary(&mut self) -> Option<f64> {
        match self.peek()? {
            '-' => {
                self.i += 1;
                Some(-self.unary()?)
            }
            '+' => {
                self.i += 1;
                self.unary()
            }
            _ => self.atom(),
        }
    }
    fn atom(&mut self) -> Option<f64> {
        if self.peek()? == '(' {
            self.i += 1;
            let v = self.expr()?;
            if self.peek()? != ')' {
                return None;
            }
            self.i += 1;
            return Some(v);
        }
        let start = self.i;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.') {
            self.i += 1;
        }
        if start == self.i {
            return None;
        }
        self.s[start..self.i].iter().collect::<String>().parse().ok()
    }
}

/// Teks hasil kalkulator: bilangan bulat tanpa desimal, selain itu maksimal 10 angka di belakang koma.
pub fn format_number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let s = format!("{:.10}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranking_priorities() {
        assert!(score("chrome", "Chrome").unwrap() > score("chr", "Chrome").unwrap());
        assert!(score("vis", "Visual Studio Code").unwrap() > score("studio", "Visual Studio Code").unwrap());
        assert_eq!(score("vsc", "Visual Studio Code"), Some(700));
        assert!(score("xyz", "Chrome").is_none());
        assert!(score("cme", "Chrome").is_some(), "subsekuens");
    }

    #[test]
    fn rank_orders_and_limits() {
        let names: Vec<String> = ["Notepad", "Note Taker", "Calculator", "Another Note"].iter().map(|s| s.to_string()).collect();
        let r = rank("note", &names, 3);
        assert_eq!(names[r[0]], "Notepad"); // awalan, nama lebih pendek menang
        assert_eq!(r.len(), 3);
    }

    #[test]
    fn calculator() {
        assert_eq!(eval("2+3*4"), Some(14.0));
        assert_eq!(eval("(2+3)*4"), Some(20.0));
        assert_eq!(eval("-5+2"), Some(-3.0));
        assert_eq!(eval("2^3^2"), Some(512.0));
        assert_eq!(eval("10/4"), Some(2.5));
        assert_eq!(eval("1,5*2"), Some(3.0));
        assert_eq!(eval("2x3"), Some(6.0));
        assert_eq!(eval("chrome"), None);
        assert_eq!(eval("12"), None, "angka saja bukan ekspresi");
        assert_eq!(eval("2+"), None);
        assert_eq!(eval("1/0"), None);
    }

    #[test]
    fn number_format() {
        assert_eq!(format_number(14.0), "14");
        assert_eq!(format_number(2.5), "2.5");
        assert_eq!(format_number(1.0 / 3.0), "0.3333333333");
    }
}
