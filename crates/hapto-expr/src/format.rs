//! Formatting numbers with Python format specs (`".2f"`, `"+.3g"`, `".1%"`).
//!
//! Readouts, interpolated text and `format(x, spec)` all print through here,
//! so a number reads the same in the figure, beside it, and in the prose. The
//! dialect is Python's format-spec mini-language, which kinemo's `Format`
//! uses too: `[[fill]align][sign][#][0][width][,][.precision][type]` with
//! types `f`, `e`, `g`, `%`, `d` or none.

/// No value prints as an em dash everywhere (§4.8).
pub const NO_VALUE: &str = "—";

#[derive(Debug, Clone, Default, PartialEq)]
struct Spec {
    fill: char,
    align: Option<char>,
    sign: char,
    alternate: bool,
    zero: bool,
    width: usize,
    grouping: bool,
    precision: Option<usize>,
    kind: Option<char>,
}

fn parse(spec: &str) -> Option<Spec> {
    let chars: Vec<char> = spec.chars().collect();
    let mut s = Spec {
        fill: ' ',
        sign: '-',
        ..Default::default()
    };
    let mut i = 0;
    let is_align = |c: char| matches!(c, '<' | '>' | '^' | '=');
    if chars.len() >= 2 && is_align(chars[1]) {
        s.fill = chars[0];
        s.align = Some(chars[1]);
        i = 2;
    } else if chars.first().copied().is_some_and(is_align) {
        s.align = Some(chars[0]);
        i = 1;
    }
    if let Some(&c) = chars.get(i) {
        if matches!(c, '+' | '-' | ' ') {
            s.sign = c;
            i += 1;
        }
    }
    if chars.get(i) == Some(&'#') {
        s.alternate = true;
        i += 1;
    }
    if chars.get(i) == Some(&'0') {
        s.zero = true;
        i += 1;
    }
    let start = i;
    while chars.get(i).is_some_and(|c| c.is_ascii_digit()) {
        i += 1;
    }
    if i > start {
        s.width = chars[start..i].iter().collect::<String>().parse().ok()?;
    }
    if chars.get(i) == Some(&',') {
        s.grouping = true;
        i += 1;
    }
    if chars.get(i) == Some(&'.') {
        i += 1;
        let start = i;
        while chars.get(i).is_some_and(|c| c.is_ascii_digit()) {
            i += 1;
        }
        if i == start {
            return None;
        }
        s.precision = Some(chars[start..i].iter().collect::<String>().parse().ok()?);
    }
    if let Some(&c) = chars.get(i) {
        if !matches!(c, 'f' | 'F' | 'e' | 'E' | 'g' | 'G' | '%' | 'd' | 'n') {
            return None;
        }
        s.kind = Some(c);
        i += 1;
    }
    (i == chars.len()).then_some(s)
}

/// Whether `spec` is a format spec this formatter understands.
pub fn is_valid(spec: &str) -> bool {
    parse(spec).is_some()
}

/// Scientific notation the way Python writes it: `1.500000e+03`.
fn scientific(x: f64, precision: usize, upper: bool) -> String {
    let text = format!("{x:.precision$e}");
    let (mantissa, exp) = text.split_once('e').unwrap_or((&text, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let e = if upper { 'E' } else { 'e' };
    format!(
        "{mantissa}{e}{}{:02}",
        if exp < 0 { '-' } else { '+' },
        exp.abs()
    )
}

fn strip_zeros(s: &str) -> String {
    if let Some((num, exp)) = s.split_once(['e', 'E']) {
        let sep = if s.contains('E') { 'E' } else { 'e' };
        return format!("{}{sep}{exp}", strip_zeros(num));
    }
    if !s.contains('.') {
        return s.to_string();
    }
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Python's `g`: fixed or scientific by magnitude, trailing zeros removed.
fn general(x: f64, precision: usize, alternate: bool, upper: bool) -> String {
    let p = precision.max(1);
    if x == 0.0 {
        return if alternate {
            format!("{:.*}", p - 1, 0.0)
        } else {
            "0".into()
        };
    }
    // The exponent after rounding to p significant digits.
    let rounded = scientific(x, p - 1, false);
    let exp: i32 = rounded
        .split_once('e')
        .and_then(|(_, e)| e.parse().ok())
        .unwrap_or(0);
    let text = if exp < -4 || exp >= p as i32 {
        scientific(x, p - 1, upper)
    } else {
        format!("{:.*}", (p as i32 - 1 - exp).max(0) as usize, x)
    };
    if alternate {
        text
    } else {
        strip_zeros(&text)
    }
}

fn group(int_part: &str) -> String {
    let digits: Vec<char> = int_part.chars().collect();
    let mut out = String::new();
    for (i, c) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*c);
    }
    out
}

/// Format `x` with a Python format spec; `None` when the spec is invalid.
pub fn format(x: f64, spec: &str) -> Option<String> {
    let s = parse(spec)?;
    if !x.is_finite() {
        return Some(NO_VALUE.into());
    }
    let negative = x.is_sign_negative() && x != 0.0;
    let a = x.abs();
    let mut body = match s.kind {
        Some('f' | 'F') => format!("{:.*}", s.precision.unwrap_or(6), a),
        Some(k @ ('e' | 'E')) => scientific(a, s.precision.unwrap_or(6), k == 'E'),
        Some(k @ ('g' | 'G')) => general(a, s.precision.unwrap_or(6), s.alternate, k == 'G'),
        Some('n') => general(a, s.precision.unwrap_or(6), s.alternate, false),
        Some('%') => format!("{:.*}%", s.precision.unwrap_or(6), a * 100.0),
        Some('d') => format!("{}", a.round()),
        None => match s.precision {
            // Python's default for floats with a precision behaves like `g`.
            Some(p) => general(a, p, s.alternate, false),
            None => format!("{a}"),
        },
        Some(_) => return None,
    };
    if s.grouping {
        let (int_part, rest) = match body.find(|c: char| !c.is_ascii_digit()) {
            Some(i) => body.split_at(i),
            None => (body.as_str(), ""),
        };
        body = format!("{}{rest}", group(int_part));
    }
    let sign = match (negative, s.sign) {
        (true, _) => "-",
        (false, '+') => "+",
        (false, ' ') => " ",
        _ => "",
    };
    let len = sign.chars().count() + body.chars().count();
    if len >= s.width {
        return Some(format!("{sign}{body}"));
    }
    let pad = s.width - len;
    let (fill, align) = if s.zero && s.align.is_none() {
        ('0', '=')
    } else {
        (s.fill, s.align.unwrap_or('>'))
    };
    let fill_str = |n: usize| fill.to_string().repeat(n);
    Some(match align {
        '<' => format!("{sign}{body}{}", fill_str(pad)),
        '^' => format!(
            "{}{sign}{body}{}",
            fill_str(pad / 2),
            fill_str(pad - pad / 2)
        ),
        '=' => format!("{sign}{}{body}", fill_str(pad)),
        _ => format!("{}{sign}{body}", fill_str(pad)),
    })
}

/// The default for a number with no spec: four significant digits, with
/// trailing zeros removed (§10.2).
pub fn default(x: f64) -> String {
    if !x.is_finite() {
        return NO_VALUE.into();
    }
    format(x, ".4g").unwrap_or_else(|| x.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(x: f64, spec: &str) -> String {
        format(x, spec).unwrap_or_else(|| panic!("invalid spec {spec:?}"))
    }

    #[test]
    fn fixed_and_percent() {
        assert_eq!(f(1.23456, ".2f"), "1.23");
        assert_eq!(f(2.0, "+.3f"), "+2.000");
        assert_eq!(f(-2.0, "+.1f"), "-2.0");
        assert_eq!(f(0.125, ".1%"), "12.5%");
        assert_eq!(f(1.0, "f"), "1.000000");
        assert_eq!(f(std::f64::consts::PI, ".3f"), "3.142");
    }

    #[test]
    fn scientific_like_python() {
        assert_eq!(f(1500.0, "e"), "1.500000e+03");
        assert_eq!(f(0.00012, ".2e"), "1.20e-04");
        assert_eq!(f(1500.0, ".1E"), "1.5E+03");
    }

    #[test]
    fn general_switches_by_magnitude() {
        assert_eq!(f(123.456, ".3g"), "123");
        assert_eq!(f(1234.5, ".3g"), "1.23e+03");
        assert_eq!(f(0.0001234, ".3g"), "0.000123");
        assert_eq!(f(0.00001234, ".3g"), "1.23e-05");
        assert_eq!(f(2.5, "+.3g"), "+2.5");
        assert_eq!(f(0.0, ".3g"), "0");
    }

    #[test]
    fn width_alignment_grouping() {
        assert_eq!(f(42.0, "6.1f"), "  42.0");
        assert_eq!(f(42.0, "<6.1f"), "42.0  ");
        assert_eq!(f(42.0, "^8.1f"), "  42.0  ");
        assert_eq!(f(-42.0, "07.1f"), "-0042.0");
        assert_eq!(f(1234567.0, ",.0f"), "1,234,567");
        assert_eq!(f(1234567.891, ",.2f"), "1,234,567.89");
        assert_eq!(f(7.0, "*>5d"), "****7");
    }

    #[test]
    fn integers_and_defaults() {
        assert_eq!(f(7.0, "d"), "7");
        assert_eq!(f(0.1, ""), "0.1");
        assert_eq!(default(1.234567), "1.235");
        assert_eq!(default(2.0), "2");
        assert_eq!(default(1234567.0), "1.235e+06");
        assert_eq!(default(f64::NAN), NO_VALUE);
    }

    #[test]
    fn bad_specs_are_refused() {
        assert!(format(1.0, ".2q").is_none());
        assert!(format(1.0, ".f").is_none());
        assert!(format(1.0, "2f3").is_none());
        assert!(is_valid("+.3g"));
    }
}
