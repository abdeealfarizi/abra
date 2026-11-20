pub fn token(random: &[u8; 10]) -> String {
    random.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn url(ip: [u8; 4], port: u16, token: &str) -> String {
    format!("http://{}.{}.{}.{}:{port}/{token}/", ip[0], ip[1], ip[2], ip[3])
}

pub fn is_private(ip: [u8; 4]) -> bool {
    ip[0] == 10 || (ip[0] == 172 && (16..=31).contains(&ip[1])) || (ip[0] == 192 && ip[1] == 168)
}

pub fn escape_html(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            _ => o.push(c),
        }
    }
    o
}

pub fn pct(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn ascii_name(s: &str) -> String {
    let n: String = s.chars().map(|c| if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' { c } else { '_' }).collect();
    if n.trim().is_empty() {
        "file".into()
    } else {
        n
    }
}

pub fn human_size(b: u64, en: bool) -> String {
    let f = b as f64;
    let (v, unit) = if f < 1000.0 * 1000.0 { (f / 1000.0, "KB") } else if f < 1e9 { (f / 1e6, "MB") } else { (f / 1e9, "GB") };
    let s = if v < 10.0 { format!("{v:.1}") } else { format!("{}", v.round() as u64) };
    let s = if en { s } else { s.replace('.', ",") };
    format!("{s} {unit}")
}

pub fn page(token: &str, files: &[(String, u64)], lang: crate::i18n::Lang) -> String {
    let (title, hint, dl, html_lang) = match lang {
        crate::i18n::Lang::Id => ("Dikirim dari ABRA", "Ketuk berkas untuk menyimpannya ke HP ini.", "Unduh", "id"),
        crate::i18n::Lang::En => ("Sent from ABRA", "Tap a file to save it to this phone.", "Download", "en"),
        crate::i18n::Lang::Es => ("Enviado desde ABRA", "Toca un archivo para guardarlo en este teléfono.", "Descargar", "es"),
    };
    let mut rows = String::new();
    for (i, (name, size)) in files.iter().enumerate() {
        rows.push_str(&format!(
            "<a class=\"f\" href=\"/{token}/f/{i}\" download><span class=\"n\">{}</span><span class=\"s\">{}</span><span class=\"b\">{dl}</span></a>\n",
            escape_html(name),
            human_size(*size, lang == crate::i18n::Lang::En)
        ));
    }
    format!(
        "<!doctype html><html lang=\"{lang}\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<title>ABRA</title><style>\
body{{margin:0;font:16px -apple-system,system-ui,Segoe UI,Roboto,sans-serif;background:#0d0d10;color:#f2f2f5}}\
main{{max-width:560px;margin:0 auto;padding:28px 18px}}h1{{font-size:22px;margin:0 0 4px}}p{{color:#9a9aa3;margin:0 0 22px}}\
.f{{display:flex;align-items:center;gap:12px;padding:14px 16px;margin:0 0 10px;border-radius:18px;background:#1c1c22;color:inherit;text-decoration:none}}\
.n{{flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-weight:600}}.s{{color:#9a9aa3;font-size:14px}}\
.b{{background:#0a84ff;color:#fff;border-radius:999px;padding:7px 14px;font-weight:600;font-size:14px}}\
</style></head><body><main><h1>{title}</h1><p>{hint}</p>\n{rows}</main></body></html>",
        lang = html_lang
    )
}

pub fn fraction(sent: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (sent.min(total) as f64 / total as f64) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_url_and_private_ranges() {
        let t = token(&[0, 1, 0xab, 0xff, 9, 16, 32, 64, 128, 7]);
        assert_eq!(t, "0001abff091020408007");
        assert_eq!(url([192, 168, 1, 23], 54321, &t), "http://192.168.1.23:54321/0001abff091020408007/");
        assert!(is_private([192, 168, 0, 5]) && is_private([10, 2, 3, 4]) && is_private([172, 20, 0, 1]));
        assert!(!is_private([172, 32, 0, 1]) && !is_private([8, 8, 8, 8]) && !is_private([169, 254, 1, 1]));
    }

    #[test]
    fn names_are_escaped_and_encoded() {
        assert_eq!(escape_html("a<b>&\"c'"), "a&lt;b&gt;&amp;&quot;c&#39;");
        assert_eq!(pct("foto ü.jpg"), "foto%20%C3%BC.jpg");
        assert_eq!(ascii_name("foto ü\".jpg"), "foto __.jpg");
        assert_eq!(ascii_name("日本"), "__");
        let html = page("tok", &[("<x>.pdf".into(), 1_234_567)], crate::i18n::Lang::Id);
        assert!(html.contains("&lt;x&gt;.pdf") && html.contains("/tok/f/0") && html.contains("1,2 MB"));
        assert!(!html.contains("<script"), "tanpa skrip");
        assert!(page("tok", &[], crate::i18n::Lang::Es).contains("lang=\"es\""));
        assert!(page("tok", &[], crate::i18n::Lang::Es).contains("Enviado desde ABRA"));
    }

    #[test]
    fn sizes_and_fraction() {
        assert_eq!(human_size(820_000, true), "820 KB");
        assert_eq!(human_size(12_400_000, true), "12 MB");
        assert_eq!(human_size(3_450_000, false), "3,5 MB");
        assert_eq!(fraction(50, 200), 0.25);
        assert_eq!(fraction(500, 200), 1.0);
        assert_eq!(fraction(0, 0), 0.0);
    }
}
