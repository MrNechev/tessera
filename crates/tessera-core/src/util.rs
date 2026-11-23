//! Small helpers that reproduce Go standard-library behaviour the Letopis API depends on:
//! `time.ParseDuration`/`Duration.String`, RFC 3339 with trimmed nanoseconds, `path.Clean`,
//! `path.Match`, `url.PathEscape`/`PathUnescape` and `exec.LookPath`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Offset, TimeZone, Utc};

// ---------- durations ----------

/// Parses a Go duration string ("30m", "1h30m", "1.5s", "500ms", "-2h"). Returns `None` on
/// malformed input, like Go's `time.ParseDuration` error. Negative durations clamp to zero.
pub fn parse_go_duration(s: &str) -> Option<Duration> {
    let mut s = s;
    let mut neg = false;
    if let Some(r) = s.strip_prefix('-') {
        neg = true;
        s = r;
    } else if let Some(r) = s.strip_prefix('+') {
        s = r;
    }
    if s == "0" {
        return Some(Duration::ZERO);
    }
    if s.is_empty() {
        return None;
    }
    let mut total: f64 = 0.0;
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
            i += 1;
        }
        if start == i {
            return None;
        }
        let num: f64 = s[start..i].parse().ok()?;
        let ustart = i;
        while i < b.len() && !(b[i].is_ascii_digit() || b[i] == b'.') {
            i += 1;
        }
        let unit = &s[ustart..i];
        let mult = match unit {
            "ns" => 1.0,
            "us" | "µs" | "μs" => 1e3,
            "ms" => 1e6,
            "s" => 1e9,
            "m" => 60e9,
            "h" => 3600e9,
            _ => return None,
        };
        total += num * mult;
    }
    if neg {
        return Some(Duration::ZERO);
    }
    Some(Duration::from_nanos(total.round() as u64))
}

/// Formats a duration like Go's `Duration.String` ("1h2m3s", "30m0s", "1.5s", "0s").
pub fn format_go_duration(d: Duration) -> String {
    let ns = d.as_nanos() as u64;
    if ns == 0 {
        return "0s".into();
    }
    if ns < 1_000_000_000 {
        // sub-second: ns, µs or ms with fractional digits
        let (unit, div) = if ns < 1_000 {
            ("ns", 1u64)
        } else if ns < 1_000_000 {
            ("µs", 1_000)
        } else {
            ("ms", 1_000_000)
        };
        return format!("{}{}", frac(ns, div), unit);
    }
    let mut out = String::new();
    let h = ns / 3_600_000_000_000;
    let rem = ns % 3_600_000_000_000;
    let m = rem / 60_000_000_000;
    let s_ns = rem % 60_000_000_000;
    if h > 0 {
        out.push_str(&format!("{h}h"));
    }
    if h > 0 || m > 0 {
        out.push_str(&format!("{m}m"));
    }
    out.push_str(&frac(s_ns, 1_000_000_000));
    out.push('s');
    out
}

fn frac(v: u64, div: u64) -> String {
    let whole = v / div;
    let mut f = v % div;
    if f == 0 {
        return whole.to_string();
    }
    let mut digits = div.to_string().len() - 1;
    while f.is_multiple_of(10) {
        f /= 10;
        digits -= 1;
    }
    format!("{whole}.{f:0digits$}")
}

// ---------- time ----------

/// Formats like Go's `t.UTC().Format(time.RFC3339Nano)`: trailing zeros of the fraction are
/// trimmed and the zone is `Z`.
pub fn rfc3339_nano_utc(t: DateTime<Utc>) -> String {
    rfc3339_nano(&t)
}

/// Go's RFC3339Nano for any zone (offset `+01:00` or `Z` for UTC).
pub fn rfc3339_nano<Tz: TimeZone>(t: &DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let base = t.format("%Y-%m-%dT%H:%M:%S").to_string();
    let nanos = t.timestamp_subsec_nanos() % 1_000_000_000;
    let mut s = base;
    if nanos > 0 {
        let mut f = format!("{nanos:09}");
        while f.ends_with('0') {
            f.pop();
        }
        s.push('.');
        s.push_str(&f);
    }
    let off = t.offset().fix().local_minus_utc();
    if off == 0 {
        s.push('Z');
    } else {
        let sign = if off < 0 { '-' } else { '+' };
        let a = off.abs();
        s.push_str(&format!("{sign}{:02}:{:02}", a / 3600, (a % 3600) / 60));
    }
    s
}

/// The zero `time.Time` as Go's JSON encoder writes it.
pub const GO_ZERO_TIME: &str = "0001-01-01T00:00:00Z";

/// Unix nanoseconds of a file's modification time (Go `fi.ModTime().UnixNano()`).
pub fn mtime_ns(md: &std::fs::Metadata) -> i64 {
    use std::os::unix::fs::MetadataExt;
    md.mtime()
        .saturating_mul(1_000_000_000)
        .saturating_add(md.mtime_nsec())
}

/// HTTP date (RFC 7231 IMF-fixdate), like Go's `http.TimeFormat`.
pub fn http_date(t: DateTime<Utc>) -> String {
    t.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

/// Parses the three HTTP date formats Go's `http.ParseTime` accepts.
pub fn parse_http_date(s: &str) -> Option<DateTime<Utc>> {
    for f in [
        "%a, %d %b %Y %H:%M:%S GMT",
        "%A, %d-%b-%y %H:%M:%S GMT",
        "%a %b %e %H:%M:%S %Y",
    ] {
        if let Ok(n) = chrono::NaiveDateTime::parse_from_str(s.trim(), f) {
            return Some(Utc.from_utc_datetime(&n));
        }
    }
    None
}

// ---------- paths ----------

/// Go's `path.Clean` for slash-separated paths.
pub fn path_clean(p: &str) -> String {
    if p.is_empty() {
        return ".".into();
    }
    let rooted = p.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if let Some(last) = out.last() {
                    if *last != ".." {
                        out.pop();
                        continue;
                    }
                }
                if !rooted {
                    out.push("..");
                }
            }
            s => out.push(s),
        }
    }
    let joined = out.join("/");
    if rooted {
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".into()
    } else {
        joined
    }
}

/// Go's `path.Dir` ("" for top-level files becomes ".").
pub fn path_dir(p: &str) -> String {
    match p.rfind('/') {
        Some(i) => path_clean(&p[..i + 1]),
        None => ".".into(),
    }
}

/// Go's `path.Base`.
pub fn path_base(p: &str) -> String {
    if p.is_empty() {
        return ".".into();
    }
    let p = p.trim_end_matches('/');
    if p.is_empty() {
        return "/".into();
    }
    match p.rfind('/') {
        Some(i) => p[i + 1..].to_string(),
        None => p.to_string(),
    }
}

/// Go's `path.Ext` / `filepath.Ext`: the suffix from the last dot of the last element.
pub fn path_ext(p: &str) -> &str {
    let b = p.as_bytes();
    let mut i = b.len();
    while i > 0 {
        i -= 1;
        if b[i] == b'/' {
            break;
        }
        if b[i] == b'.' {
            return &p[i..];
        }
    }
    ""
}

/// Go's `path.Match`: shell pattern matching where `*` and `?` never match `/`.
/// A malformed pattern never matches (Go returns `ErrBadPattern`).
pub fn path_match(pattern: &str, name: &str) -> bool {
    match_inner(pattern.as_bytes(), name.as_bytes()).unwrap_or(false)
}

fn match_inner(mut pattern: &[u8], mut name: &[u8]) -> Option<bool> {
    'pattern: while !pattern.is_empty() {
        let (star, chunk, rest) = scan_chunk(pattern);
        pattern = rest;
        if star && chunk.is_empty() {
            // trailing * matches the rest unless it contains a slash
            return Some(!name.contains(&b'/'));
        }
        let (t, ok) = match_chunk(chunk, name)?;
        if ok && (t.is_empty() || !pattern.is_empty()) {
            name = t;
            continue;
        }
        if star {
            let mut i = 0;
            while i < name.len() && name[i] != b'/' {
                let (t, ok) = match_chunk(chunk, &name[i + 1..])?;
                if ok {
                    if pattern.is_empty() && !t.is_empty() {
                        i += 1;
                        continue;
                    }
                    name = t;
                    continue 'pattern;
                }
                i += 1;
            }
        }
        // validate the rest of the pattern like Go does before failing
        while !pattern.is_empty() {
            let (_, chunk, rest) = scan_chunk(pattern);
            pattern = rest;
            match_chunk(chunk, b"")?;
        }
        return Some(false);
    }
    Some(name.is_empty())
}

fn scan_chunk(mut pattern: &[u8]) -> (bool, &[u8], &[u8]) {
    let mut star = false;
    while !pattern.is_empty() && pattern[0] == b'*' {
        pattern = &pattern[1..];
        star = true;
    }
    let mut in_range = false;
    let mut i = 0;
    while i < pattern.len() {
        match pattern[i] {
            b'\\' => {
                if i + 1 < pattern.len() {
                    i += 1;
                }
            }
            b'[' => in_range = true,
            b']' => in_range = false,
            b'*' if !in_range => break,
            _ => {}
        }
        i += 1;
    }
    (star, &pattern[..i], &pattern[i..])
}

/// Returns `None` for a malformed pattern.
fn match_chunk<'a>(mut chunk: &[u8], mut s: &'a [u8]) -> Option<(&'a [u8], bool)> {
    let mut failed = false;
    while !chunk.is_empty() {
        if !failed && s.is_empty() {
            failed = true;
        }
        match chunk[0] {
            b'[' => {
                let mut r: char = '\0';
                if !failed {
                    let (c, n) = decode_rune(s);
                    r = c;
                    s = &s[n..];
                }
                chunk = &chunk[1..];
                let mut negated = false;
                if !chunk.is_empty() && chunk[0] == b'^' {
                    negated = true;
                    chunk = &chunk[1..];
                }
                let mut matched = false;
                let mut nrange = 0;
                loop {
                    if !chunk.is_empty() && chunk[0] == b']' && nrange > 0 {
                        chunk = &chunk[1..];
                        break;
                    }
                    let (lo, rest) = get_esc(chunk)?;
                    chunk = rest;
                    let mut hi = lo;
                    if chunk.first() == Some(&b'-') {
                        let (h, rest) = get_esc(&chunk[1..])?;
                        hi = h;
                        chunk = rest;
                    }
                    if lo <= r && r <= hi {
                        matched = true;
                    }
                    nrange += 1;
                }
                if matched == negated {
                    failed = true;
                }
            }
            b'?' => {
                if !failed {
                    if s[0] == b'/' {
                        failed = true;
                    }
                    let (_, n) = decode_rune(s);
                    s = &s[n..];
                }
                chunk = &chunk[1..];
            }
            b'\\' => {
                chunk = &chunk[1..];
                if chunk.is_empty() {
                    return None;
                }
                if !failed {
                    if chunk[0] != s[0] {
                        failed = true;
                    }
                    s = &s[1..];
                }
                chunk = &chunk[1..];
            }
            c => {
                if !failed {
                    if c != s[0] {
                        failed = true;
                    }
                    s = &s[1..];
                }
                chunk = &chunk[1..];
            }
        }
    }
    if failed {
        return Some((b"", false));
    }
    Some((s, true))
}

fn get_esc(chunk: &[u8]) -> Option<(char, &[u8])> {
    if chunk.is_empty() || chunk[0] == b'-' || chunk[0] == b']' {
        return None;
    }
    let mut chunk = chunk;
    if chunk[0] == b'\\' {
        chunk = &chunk[1..];
        if chunk.is_empty() {
            return None;
        }
    }
    let (r, n) = decode_rune(chunk);
    if r == char::REPLACEMENT_CHARACTER && n == 1 {
        return None;
    }
    chunk = &chunk[n..];
    if chunk.is_empty() {
        return None;
    }
    Some((r, chunk))
}

fn decode_rune(s: &[u8]) -> (char, usize) {
    for n in 1..=4.min(s.len()) {
        if let Ok(st) = std::str::from_utf8(&s[..n]) {
            if let Some(c) = st.chars().next() {
                return (c, n);
            }
        }
    }
    (char::REPLACEMENT_CHARACTER, 1)
}

// ---------- URLs ----------

/// Go's `url.PathEscape`.
pub fn path_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &c in s.as_bytes() {
        let keep = c.is_ascii_alphanumeric()
            || matches!(
                c,
                b'-' | b'_' | b'.' | b'~' | b'$' | b'&' | b'+' | b':' | b'=' | b'@'
            );
        if keep {
            out.push(c as char);
        } else {
            out.push_str(&format!("%{c:02X}"));
        }
    }
    out
}

/// Go's `url.PathUnescape` (no `+` → space; malformed escapes are an error).
pub fn path_unescape(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let h = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(h, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

// ---------- processes ----------

/// Go's `exec.LookPath`: finds an executable in `$PATH`.
pub fn look_path(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    if name.contains('/') {
        let p = Path::new(name);
        return p.is_file().then(|| p.to_path_buf());
    }
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let p = dir.join(name);
        if let Ok(md) = std::fs::metadata(&p) {
            if md.is_file() && md.permissions().mode() & 0o111 != 0 {
                return Some(p);
            }
        }
    }
    None
}

/// First line of a command's output, at most 240 bytes (error messages).
pub fn first_line(s: &str) -> String {
    let s = s.trim();
    let s = s.split('\n').next().unwrap_or("");
    truncate_bytes(s, 240).to_string()
}

/// Truncates to at most `n` bytes on a char boundary.
pub fn truncate_bytes(s: &str, n: usize) -> &str {
    if s.len() <= n {
        return s;
    }
    let mut i = n;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    &s[..i]
}

/// Number of logical CPUs (Go's `runtime.NumCPU`).
pub fn num_cpu() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// Current Unix time in milliseconds.
pub fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(parse_go_duration("30m"), Some(Duration::from_secs(1800)));
        assert_eq!(parse_go_duration("1h30m"), Some(Duration::from_secs(5400)));
        assert_eq!(parse_go_duration("1.5s"), Some(Duration::from_millis(1500)));
        assert_eq!(parse_go_duration("500ms"), Some(Duration::from_millis(500)));
        assert_eq!(parse_go_duration("0"), Some(Duration::ZERO));
        assert_eq!(parse_go_duration("10"), None);
        assert_eq!(parse_go_duration("x"), None);
        assert_eq!(format_go_duration(Duration::from_secs(1800)), "30m0s");
        assert_eq!(format_go_duration(Duration::from_secs(3723)), "1h2m3s");
        assert_eq!(format_go_duration(Duration::from_secs(5)), "5s");
        assert_eq!(format_go_duration(Duration::from_millis(1500)), "1.5s");
        assert_eq!(format_go_duration(Duration::from_millis(250)), "250ms");
        assert_eq!(format_go_duration(Duration::ZERO), "0s");
        assert_eq!(
            format_go_duration(Duration::from_secs(365 * 24 * 3600)),
            "8760h0m0s"
        );
    }

    #[test]
    fn rfc3339() {
        let t = Utc.with_ymd_and_hms(2024, 7, 14, 16, 22, 5).unwrap();
        assert_eq!(rfc3339_nano_utc(t), "2024-07-14T16:22:05Z");
        let t = t + chrono::Duration::nanoseconds(120_000_000);
        assert_eq!(rfc3339_nano_utc(t), "2024-07-14T16:22:05.12Z");
    }

    #[test]
    fn clean_and_match() {
        assert_eq!(path_clean("a//b/./c/.."), "a/b");
        assert_eq!(path_clean("./a.jpg"), "a.jpg");
        assert_eq!(path_clean("../x"), "../x");
        assert_eq!(path_clean("a/../../x"), "../x");
        assert_eq!(path_clean("/../x"), "/x");
        assert_eq!(path_clean(""), ".");
        assert_eq!(path_dir("a/b.jpg"), "a");
        assert_eq!(path_dir("b.jpg"), ".");
        assert_eq!(path_base("a/b.jpg"), "b.jpg");
        assert_eq!(path_ext("a/b.tar.gz"), ".gz");
        assert_eq!(path_ext("a.d/b"), "");

        assert!(path_match("*_files", "page_files"));
        assert!(!path_match("*_files", "a/page_files"));
        assert!(path_match("Private/*", "Private/x"));
        assert!(!path_match("Private/*", "Private/x/y"));
        assert!(path_match("*.tmp", "a.tmp"));
        assert!(path_match("a?c", "abc"));
        assert!(!path_match("a?c", "a/c"));
        assert!(path_match("[a-c]x", "bx"));
        assert!(!path_match("[^a-c]x", "bx"));
        assert!(path_match("\\*", "*"));
        assert!(!path_match("[", "a"));
        assert!(path_match("*", "Смешни"));
        assert!(path_match("a*b*c", "axxbyyc"));
    }

    #[test]
    fn escape() {
        assert_eq!(path_escape("a b/c?d"), "a%20b%2Fc%3Fd");
        assert_eq!(path_escape("x+y:z=@$&"), "x+y:z=@$&");
        assert_eq!(path_escape("ж"), "%D0%B6");
        assert_eq!(path_unescape("a%20b").as_deref(), Some("a b"));
        assert_eq!(path_unescape("a%2").as_deref(), None);
        assert_eq!(path_unescape("a+b").as_deref(), Some("a+b"));
    }
}
