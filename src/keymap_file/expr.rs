//! C integer expressions, as found in `#if` and in devicetree cells: `(0x04 << 24 | 0x7002a)`.

pub fn eval(words: &[&str]) -> Result<i64, String> {
    let mut p = Parser { words, pos: 0 };
    let v = p.ternary()?;
    match p.words.get(p.pos) {
        None => Ok(v),
        Some(w) => Err(format!("unexpected {w:?} in expression")),
    }
}

/// Parses a C integer literal: decimal, hex, octal or binary, with optional U/L suffixes.
pub fn number(s: &str) -> Option<i64> {
    let s = s.trim_end_matches(['u', 'U', 'l', 'L']);
    let (digits, radix) = if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        (h, 16)
    } else if let Some(b) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        (b, 2)
    } else if s.len() > 1 && s.starts_with('0') {
        (&s[1..], 8)
    } else {
        (s, 10)
    };
    u64::from_str_radix(digits, radix).ok().map(|v| v as i64)
}

struct Parser<'a> {
    words: &'a [&'a str],
    pos: usize,
}

/// Binary operators by precedence level, loosest first.
const LEVELS: [&[&str]; 10] = [
    &["||"],
    &["&&"],
    &["|"],
    &["^"],
    &["&"],
    &["==", "!="],
    &["<", ">", "<=", ">="],
    &["<<", ">>"],
    &["+", "-"],
    &["*", "/", "%"],
];

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&'a str> {
        self.words.get(self.pos).copied()
    }

    fn eat(&mut self, w: &str) -> bool {
        let hit = self.peek() == Some(w);
        if hit {
            self.pos += 1;
        }
        hit
    }

    fn ternary(&mut self) -> Result<i64, String> {
        let c = self.binary(0)?;
        if !self.eat("?") {
            return Ok(c);
        }
        let a = self.ternary()?;
        if !self.eat(":") {
            return Err("expected ':'".into());
        }
        let b = self.ternary()?;
        Ok(if c != 0 { a } else { b })
    }

    fn binary(&mut self, level: usize) -> Result<i64, String> {
        if level == LEVELS.len() {
            return self.unary();
        }
        let mut l = self.binary(level + 1)?;
        while let Some(op) = self.peek().filter(|w| LEVELS[level].contains(w)) {
            self.pos += 1;
            let r = self.binary(level + 1)?;
            l = match op {
                "||" => i64::from(l != 0 || r != 0),
                "&&" => i64::from(l != 0 && r != 0),
                "|" => l | r,
                "^" => l ^ r,
                "&" => l & r,
                "==" => i64::from(l == r),
                "!=" => i64::from(l != r),
                "<" => i64::from(l < r),
                ">" => i64::from(l > r),
                "<=" => i64::from(l <= r),
                ">=" => i64::from(l >= r),
                "<<" => l.wrapping_shl(r as u32),
                ">>" => l.wrapping_shr(r as u32),
                "+" => l.wrapping_add(r),
                "-" => l.wrapping_sub(r),
                "*" => l.wrapping_mul(r),
                "/" | "%" if r == 0 => return Err("division by zero".into()),
                "/" => l.wrapping_div(r),
                _ => l.wrapping_rem(r),
            };
        }
        Ok(l)
    }

    fn unary(&mut self) -> Result<i64, String> {
        match self.peek() {
            Some("-") => {
                self.pos += 1;
                Ok(self.unary()?.wrapping_neg())
            }
            Some("+") => {
                self.pos += 1;
                self.unary()
            }
            Some("~") => {
                self.pos += 1;
                Ok(!self.unary()?)
            }
            Some("!") => {
                self.pos += 1;
                Ok(i64::from(self.unary()? == 0))
            }
            Some("(") => {
                self.pos += 1;
                let v = self.ternary()?;
                if !self.eat(")") {
                    return Err("expected ')'".into());
                }
                Ok(v)
            }
            Some(w) => {
                self.pos += 1;
                if let Some(c) = w.strip_prefix('\'').and_then(|w| w.strip_suffix('\'')) {
                    return c.chars().next().map(|c| c as i64).ok_or("empty character literal".into());
                }
                number(w).ok_or_else(|| format!("not a number: {w:?}"))
            }
            None => Err("expression ends early".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(s: &str) -> Result<i64, String> {
        let toks = super::super::pp::lex(s);
        let words: Vec<&str> = toks.iter().map(|t| t.text.as_str()).collect();
        eval(&words)
    }

    #[test]
    fn precedence_and_literals() {
        assert_eq!(e("1 + 2 * 3"), Ok(7));
        assert_eq!(e("(0x04 << 24 | ((0x07 << 16) | 0x2a))"), Ok(0x0407_002A));
        assert_eq!(e("1 << 2 + 1"), Ok(8));
        assert_eq!(e("010 + 0b11 + 1UL"), Ok(12));
        assert_eq!(e("-600 & 0xFFFF"), Ok(0xFDA8));
        assert_eq!(e("!0 && 2 > 1 ? 5 : 6"), Ok(5));
        assert_eq!(e("~0 & 0xF"), Ok(15));
        assert!(e("1 +").is_err() && e("(1").is_err() && e("1 / 0").is_err() && e("A").is_err());
    }
}
