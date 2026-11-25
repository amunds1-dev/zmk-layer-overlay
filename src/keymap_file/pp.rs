//! A small C preprocessor, enough for ZMK keymaps: object- and function-like macros (with `#`
//! and `##`), conditionals, and `#include` of files next to the keymap. ZMK's own headers are
//! replaced by a generated table of their values (`zmk-defines.h`).

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::expr;

const ZMK_DEFINES: &str = include_str!("zmk-defines.h");
const MAX_INCLUDE_DEPTH: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Ident,
    Number,
    Str,
    Punct,
}

#[derive(Debug, Clone)]
pub struct Tok {
    pub kind: Kind,
    pub text: String,
    /// Whitespace before this token (kept so the output text stays readable as DTS).
    pub space: bool,
    /// Macros that may not expand this token again (C's "hide set").
    hide: Rc<Vec<String>>,
}

impl Tok {
    fn new(kind: Kind, text: impl Into<String>, space: bool) -> Self {
        Tok { kind, text: text.into(), space, hide: Rc::default() }
    }

    fn is(&self, p: &str) -> bool {
        self.kind == Kind::Punct && self.text == p
    }
}

#[derive(Debug, Clone)]
struct Macro {
    /// `None` for object-like macros.
    params: Option<Vec<String>>,
    variadic: bool,
    body: Vec<Tok>,
}

/// Removes comments and joins continued lines, keeping line breaks so directives stay on
/// their own lines.
fn clean(src: &str) -> String {
    let src = src.replace("\\\r\n", "").replace("\\\n", "");
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' | '\'' => {
                out.push(c);
                while let Some(d) = chars.next() {
                    out.push(d);
                    if d == '\\' {
                        if let Some(e) = chars.next() {
                            out.push(e);
                        }
                    } else if d == c || d == '\n' {
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'/') => {
                while chars.peek().is_some_and(|&d| d != '\n') {
                    chars.next();
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                out.push(' ');
                let mut prev = ' ';
                for d in chars.by_ref() {
                    if d == '\n' {
                        out.push('\n');
                    }
                    if prev == '*' && d == '/' {
                        break;
                    }
                    prev = d;
                }
            }
            _ => out.push(c),
        }
    }
    out
}

pub fn lex(line: &str) -> Vec<Tok> {
    const PUNCT2: [&str; 11] = ["##", "<<", ">>", "&&", "||", "==", "!=", "<=", ">=", "->", "..."];
    let chars: Vec<char> = line.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    let mut space = false;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            space = true;
            i += 1;
            continue;
        }
        let start = i;
        let kind = if c.is_ascii_alphabetic() || c == '_' {
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            Kind::Ident
        } else if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit)) {
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.') {
                i += 1;
            }
            Kind::Number
        } else if c == '"' || c == '\'' {
            i += 1;
            while i < chars.len() && chars[i] != c {
                i += if chars[i] == '\\' { 2 } else { 1 };
            }
            i = (i + 1).min(chars.len());
            Kind::Str
        } else {
            let two: String = chars[i..chars.len().min(i + 3)].iter().collect();
            i += PUNCT2.iter().find(|p| two.starts_with(**p)).map_or(1, |p| p.len());
            Kind::Punct
        };
        toks.push(Tok::new(kind, chars[start..i].iter().collect::<String>(), space));
        space = false;
    }
    toks
}

/// Joins tokens back into text, with a space wherever the source had whitespace.
pub fn join(toks: &[Tok]) -> String {
    let mut s = String::new();
    for t in toks {
        if t.space && !s.is_empty() {
            s.push(' ');
        }
        s.push_str(&t.text);
    }
    s
}

#[derive(Default)]
pub struct Preprocessor {
    macros: HashMap<String, Macro>,
    /// Problems worth reporting (unresolved includes, bad directives); parsing goes on.
    pub warnings: Vec<String>,
}

struct Cond {
    /// This branch is being emitted.
    active: bool,
    /// Some branch of this #if chain was taken (so #else/#elif are off).
    taken: bool,
    /// The enclosing region is active.
    parent: bool,
}

impl Preprocessor {
    /// A preprocessor that knows ZMK's header values.
    pub fn with_zmk() -> Self {
        let mut pp = Preprocessor::default();
        let mut sink = Vec::new();
        pp.run(ZMK_DEFINES, None, 0, &mut sink);
        pp
    }

    /// Preprocesses a file, returning the expanded tokens.
    pub fn file(&mut self, path: &Path) -> std::io::Result<Vec<Tok>> {
        let src = fs::read_to_string(path)?;
        let mut out = Vec::new();
        self.run(&src, Some(path), 0, &mut out);
        Ok(out)
    }

    #[cfg(test)]
    pub fn text(&mut self, src: &str) -> Vec<Tok> {
        let mut out = Vec::new();
        self.run(src, None, 0, &mut out);
        out
    }

    fn run(&mut self, src: &str, path: Option<&Path>, depth: usize, out: &mut Vec<Tok>) {
        let src = clean(src);
        let mut conds: Vec<Cond> = Vec::new();
        let mut pending: Vec<Tok> = Vec::new();
        for line in src.lines() {
            let active = conds.last().is_none_or(|c| c.active);
            let toks = lex(line);
            let directive = match toks.as_slice() {
                [hash, rest @ ..] if hash.is("#") => match rest.first() {
                    None => Some(("", rest)),
                    Some(t) if t.kind == Kind::Ident && is_directive(&t.text) => Some((t.text.as_str(), &rest[1..])),
                    _ => None, // e.g. DTS `#binding-cells = <1>;`
                },
                _ => None,
            };
            let Some((name, args)) = directive else {
                if active {
                    let mut toks = toks;
                    if let Some(first) = toks.first_mut() {
                        first.space = true;
                    }
                    pending.extend(toks);
                }
                continue;
            };
            // Text before a directive is expanded with the macros defined so far.
            if !pending.is_empty() {
                out.extend(self.expand(std::mem::take(&mut pending)));
            }
            match name {
                "if" | "ifdef" | "ifndef" => {
                    let value = active
                        && match name {
                            "ifdef" => args.first().is_some_and(|t| self.macros.contains_key(&t.text)),
                            "ifndef" => args.first().is_none_or(|t| !self.macros.contains_key(&t.text)),
                            _ => self.condition(args),
                        };
                    conds.push(Cond { active: value, taken: value, parent: active });
                }
                "elif" => {
                    if let Some(c) = conds.last_mut() {
                        let (parent, taken) = (c.parent, c.taken);
                        let value = parent && !taken && self.condition(args);
                        let c = conds.last_mut().unwrap();
                        c.active = value;
                        c.taken |= value;
                    }
                }
                "else" => {
                    if let Some(c) = conds.last_mut() {
                        c.active = c.parent && !c.taken;
                        c.taken = true;
                    }
                }
                "endif" => {
                    conds.pop();
                }
                _ if !active => {}
                "define" => self.directive_define(args),
                "undef" => {
                    if let Some(t) = args.first() {
                        self.macros.remove(&t.text);
                    }
                }
                "include" => self.include(args, path, depth, out),
                "error" => self.warnings.push(format!("#error {}", join(args))),
                _ => {} // pragma, warning, line, null directive
            }
        }
        if !pending.is_empty() {
            out.extend(self.expand(pending));
        }
    }

    fn directive_define(&mut self, args: &[Tok]) {
        let Some(name) = args.first().filter(|t| t.kind == Kind::Ident) else {
            self.warnings.push(format!("bad #define {}", join(args)));
            return;
        };
        let rest = &args[1..];
        // Function-like only when "(" follows the name directly.
        if let Some(open) = rest.first()
            && open.is("(")
            && !open.space
        {
            let close = rest.iter().position(|t| t.is(")")).unwrap_or(rest.len() - 1);
            let mut params = Vec::new();
            let mut variadic = false;
            for t in &rest[1..close] {
                match t.kind {
                    Kind::Ident => params.push(t.text.clone()),
                    Kind::Punct if t.text == "..." => {
                        variadic = true;
                        params.push("__VA_ARGS__".into());
                    }
                    _ => {}
                }
            }
            let body = rest.get(close + 1..).unwrap_or_default().to_vec();
            self.macros.insert(name.text.clone(), Macro { params: Some(params), variadic, body });
        } else {
            self.macros.insert(name.text.clone(), Macro { params: None, variadic: false, body: rest.to_vec() });
        }
    }

    fn include(&mut self, args: &[Tok], path: Option<&Path>, depth: usize, out: &mut Vec<Tok>) {
        let target = match args {
            [t] if t.kind == Kind::Str => t.text.trim_matches('"').to_string(),
            [open, inner @ .., close] if open.is("<") && close.is(">") => {
                inner.iter().map(|t| t.text.as_str()).collect::<String>()
            }
            _ => {
                self.warnings.push(format!("bad #include {}", join(args)));
                return;
            }
        };
        // ZMK's own headers and behaviors are built in.
        if target.starts_with("dt-bindings/") || target == "behaviors.dtsi" {
            return;
        }
        let dir = path.and_then(Path::parent).unwrap_or(Path::new("."));
        let candidates: Vec<PathBuf> = vec![dir.join(&target), dir.join("..").join(&target)];
        match candidates.iter().find(|p| p.is_file()) {
            Some(p) if depth < MAX_INCLUDE_DEPTH => match fs::read_to_string(p) {
                Ok(src) => self.run(&src, Some(p), depth + 1, out),
                Err(e) => self.warnings.push(format!("cannot read {}: {e}", p.display())),
            },
            Some(p) => self.warnings.push(format!("includes nested too deep at {}", p.display())),
            None => self.warnings.push(format!("include not found: {target}")),
        }
    }

    /// Evaluates an #if/#elif condition.
    fn condition(&self, args: &[Tok]) -> bool {
        // Resolve `defined X` / `defined(X)` before expansion.
        let mut toks = Vec::new();
        let mut i = 0;
        while i < args.len() {
            if args[i].kind == Kind::Ident && args[i].text == "defined" {
                let (name, skip) = match args.get(i + 1) {
                    Some(t) if t.is("(") => (args.get(i + 2), 4),
                    t => (t, 2),
                };
                let set = name.is_some_and(|t| self.macros.contains_key(&t.text));
                toks.push(Tok::new(Kind::Number, if set { "1" } else { "0" }, true));
                i += skip;
            } else {
                toks.push(args[i].clone());
                i += 1;
            }
        }
        let expanded = self.expand(toks);
        let words: Vec<&str> =
            expanded.iter().map(|t| if t.kind == Kind::Ident { "0" } else { t.text.as_str() }).collect();
        expr::eval(&words).is_ok_and(|v| v != 0)
    }

    /// Expands macros in a token list (rescanning results, honouring hide sets).
    pub fn expand(&self, toks: Vec<Tok>) -> Vec<Tok> {
        let mut input: VecDeque<Tok> = toks.into();
        let mut out = Vec::new();
        while let Some(t) = input.pop_front() {
            let m = match t.kind {
                Kind::Ident if !t.hide.contains(&t.text) => self.macros.get(&t.text),
                _ => None,
            };
            let Some(m) = m else {
                out.push(t);
                continue;
            };
            let mut hide = (*t.hide).clone();
            hide.push(t.text.clone());
            let hide = Rc::new(hide);
            let result = match &m.params {
                None => m.body.clone(),
                Some(params) => {
                    if !input.front().is_some_and(|n| n.is("(")) {
                        out.push(t);
                        continue;
                    }
                    let Some(args) = take_args(&mut input) else {
                        out.push(t);
                        continue;
                    };
                    self.substitute(m, params, args)
                }
            };
            for (i, mut r) in result.into_iter().enumerate().rev() {
                let mut h = (*r.hide).clone();
                h.extend(hide.iter().cloned());
                r.hide = Rc::new(h);
                if i == 0 {
                    r.space = t.space;
                }
                input.push_front(r);
            }
        }
        out
    }

    fn substitute(&self, m: &Macro, params: &[String], mut args: Vec<Vec<Tok>>) -> Vec<Tok> {
        if m.variadic && args.len() > params.len() {
            // Fold the extra arguments back into __VA_ARGS__.
            let mut rest = args.split_off(params.len() - 1);
            let mut va = rest.remove(0);
            for a in rest {
                va.push(Tok::new(Kind::Punct, ",", false));
                va.extend(a);
            }
            args.push(va);
        }
        let arg = |name: &str| params.iter().position(|p| p == name).map(|i| args.get(i).cloned().unwrap_or_default());
        let body = &m.body;
        let mut out: Vec<Tok> = Vec::new();
        let mut i = 0;
        while i < body.len() {
            let t = &body[i];
            // #param: stringify.
            if t.is("#")
                && let Some(next) = body.get(i + 1)
                && let Some(a) = arg(&next.text)
            {
                let s = join(&a).replace('\\', "\\\\").replace('"', "\\\"");
                out.push(Tok::new(Kind::Str, format!("\"{s}\""), t.space));
                i += 2;
                continue;
            }
            // a ## b: paste the last token so far with the next one (arguments unexpanded).
            if t.is("##") {
                if let Some(next) = body.get(i + 1) {
                    let mut right = arg(&next.text).unwrap_or_else(|| vec![next.clone()]);
                    match (out.pop(), right.first()) {
                        (Some(left), Some(first)) => {
                            let pasted = lex(&format!("{}{}", left.text, first.text));
                            let mut pasted = pasted.into_iter();
                            if let Some(mut p) = pasted.next() {
                                p.space = left.space;
                                out.push(p);
                            }
                            out.extend(pasted);
                            right.remove(0);
                        }
                        (Some(left), None) => out.push(left),
                        (None, _) => {}
                    }
                    out.extend(right);
                }
                i += 2;
                continue;
            }
            match arg(&t.text).filter(|_| t.kind == Kind::Ident) {
                Some(a) => {
                    // Arguments next to ## are pasted raw; others are fully expanded first.
                    let raw = body.get(i + 1).is_some_and(|n| n.is("##"));
                    let mut a = if raw { a } else { self.expand(a) };
                    if let Some(first) = a.first_mut() {
                        first.space = t.space;
                    }
                    out.extend(a);
                }
                None => out.push(t.clone()),
            }
            i += 1;
        }
        out
    }
}

fn is_directive(name: &str) -> bool {
    matches!(
        name,
        "define"
            | "undef"
            | "include"
            | "if"
            | "ifdef"
            | "ifndef"
            | "elif"
            | "else"
            | "endif"
            | "pragma"
            | "error"
            | "warning"
            | "line"
    )
}

/// Takes `( a, b, ... )` off the front of the input; `None` if the parenthesis isn't closed.
fn take_args(input: &mut VecDeque<Tok>) -> Option<Vec<Vec<Tok>>> {
    let mut depth = 0usize;
    let mut len = 0;
    for t in input.iter() {
        len += 1;
        if t.is("(") {
            depth += 1;
        } else if t.is(")") {
            depth -= 1;
            if depth == 0 {
                break;
            }
        }
    }
    if depth != 0 {
        return None;
    }
    let toks: Vec<Tok> = input.drain(..len).collect();
    let inner = &toks[1..toks.len() - 1];
    let mut args = vec![Vec::new()];
    let mut depth = 0usize;
    for t in inner {
        if t.is("(") {
            depth += 1;
        } else if t.is(")") {
            depth = depth.saturating_sub(1);
        } else if t.is(",") && depth == 0 {
            args.push(Vec::new());
            continue;
        }
        args.last_mut().unwrap().push(t.clone());
    }
    Some(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> String {
        let mut pp = Preprocessor::with_zmk();
        let out = pp.text(src);
        join(&out)
    }

    #[test]
    fn comments_and_continuations() {
        assert_eq!(run("a /* x\n y */ b // c\nd \\\ne"), "a b d e");
        assert_eq!(run("\"// not a comment\""), "\"// not a comment\"");
    }

    #[test]
    fn zmk_names_and_modifier_functions() {
        assert_eq!(run("Q"), "0x70014");
        assert_eq!(run("BT_SEL 0 BT_CLR"), "3 0 0 0");
        // LA(BACKSPACE) = (0x04 << 24 | 0x7002a)
        let s = run("LA(BACKSPACE)");
        let words: Vec<String> = lex(&s).into_iter().map(|t| t.text).collect();
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        assert_eq!(expr::eval(&words).unwrap(), 0x0407_002A);
    }

    #[test]
    fn user_macros() {
        let src = "#define NAV 1\n#define HRM(k, m) &hm m k\n#define CAT(a, b) a##b\n#define STR(x) #x\n\
                   NAV HRM(A, LSHFT) CAT(fo, o) STR(a b)";
        assert_eq!(run(src), "1 &hm 0x700e1 0x70004 foo \"a b\"");
    }

    #[test]
    fn recursion_stops() {
        assert_eq!(run("#define X X + 1\nX"), "X + 1");
        assert_eq!(run("#define F(a) G(a)\n#define G(a) F(a)\nF(1)"), "F(1)");
    }

    #[test]
    fn function_like_name_without_parens_stays() {
        assert_eq!(run("#define F(a) a\nF + 1"), "F + 1");
        // Object-like macro expanding to a function-like name takes args from the text after it.
        assert_eq!(run("#define SHIFTED LS\nSHIFTED(N1)"), run("LS(N1)"));
    }

    #[test]
    fn conditionals() {
        let src = "#define A_ON\n#ifdef A_ON\nyes\n#else\nno\n#endif\n#if defined(NOPE) || 0\nbad\n#elif 2 > 1\nelif\n#endif\n#ifndef A_ON\nbad\n#endif";
        assert_eq!(run(src), "yes elif");
    }

    #[test]
    fn dts_hash_properties_pass_through() {
        assert_eq!(run("#binding-cells = <2>;"), "#binding-cells = <2>;");
    }

    #[test]
    fn variadic_macros() {
        assert_eq!(run("#define L(name, ...) name: __VA_ARGS__\nL(x, 1, 2, 3)"), "x: 1, 2, 3");
    }
}
