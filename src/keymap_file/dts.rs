//! Devicetree source, after preprocessing: nodes, labels and properties. Only what keymaps
//! use; `/delete-node/` and friends are skipped.

use super::expr;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Node {
    pub name: String,
    pub labels: Vec<String>,
    pub props: Vec<(String, Vec<Value>)>,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Cells(Vec<Cell>),
    Str(String),
    Bytes(Vec<u8>),
    /// `&label` outside cells (a path reference).
    Ref(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Num(i64),
    /// `&label`: a phandle, e.g. the behavior in `<&kp A>`.
    Ref(String),
}

impl Node {
    pub fn prop(&self, name: &str) -> Option<&[Value]> {
        self.props.iter().rev().find(|(n, _)| n == name).map(|(_, v)| v.as_slice())
    }

    pub fn string(&self, name: &str) -> Option<&str> {
        match self.prop(name)?.first()? {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    /// All cells of a property, across `<...>, <...>` groups.
    pub fn cells(&self, name: &str) -> Vec<Cell> {
        self.prop(name)
            .unwrap_or_default()
            .iter()
            .flat_map(|v| match v {
                Value::Cells(c) => c.clone(),
                _ => Vec::new(),
            })
            .collect()
    }

    /// Depth-first walk over this node and its descendants.
    pub fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a Node)) {
        f(self);
        for c in &self.children {
            c.walk(f);
        }
    }
}

/// Parses a whole file into one root holding `/ { }` contents and `&label { }` overlays
/// (the latter as children named `&label`).
pub fn parse(src: &str) -> Result<Node, String> {
    let mut p = Parser { s: src.as_bytes(), pos: 0 };
    let mut root = Node { name: "/".into(), ..Node::default() };
    loop {
        p.skip_ws();
        if p.pos >= p.s.len() {
            return Ok(root);
        }
        if p.eat_word("/dts-v1/") || p.eat_word("/plugin/") {
            p.expect(b';')?;
        } else if p.eat_word("/delete-node/") || p.eat_word("/memreserve/") || p.eat_word("/omit-if-no-ref/") {
            p.skip_statement()?;
        } else if p.eat_word("/include/") {
            p.string()?;
        } else if p.peek() == Some(b'/') {
            p.pos += 1;
            p.skip_ws();
            let body = p.block()?;
            root.props.extend(body.props);
            root.children.extend(body.children);
        } else if p.peek() == Some(b'&') {
            p.pos += 1;
            let name = if p.peek() == Some(b'{') {
                p.pos += 1;
                let path = p.until(b'}')?;
                format!("&{{{path}}}")
            } else {
                format!("&{}", p.name())
            };
            p.skip_ws();
            let mut node = p.block()?;
            node.name = name;
            root.children.push(node);
        } else {
            // A labelled top-level node or other construct we don't need.
            let mut labels = Vec::new();
            let name = p.labels_and_name(&mut labels)?;
            p.skip_ws();
            if p.peek() == Some(b'{') {
                let mut node = p.block()?;
                node.name = name;
                node.labels = labels;
                root.children.push(node);
            } else {
                p.skip_statement()?;
            }
        }
    }
}

struct Parser<'a> {
    s: &'a [u8],
    pos: usize,
}

fn is_name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || b",._+-#?@".contains(&c)
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }

    fn context(&self) -> String {
        let end = (self.pos + 30).min(self.s.len());
        String::from_utf8_lossy(&self.s[self.pos..end]).into_owned()
    }

    fn err<T>(&self, msg: &str) -> Result<T, String> {
        Err(format!("{msg} near {:?}", self.context()))
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, c: u8) -> Result<(), String> {
        self.skip_ws();
        if self.peek() == Some(c) {
            self.pos += 1;
            Ok(())
        } else {
            self.err(&format!("expected '{}'", c as char))
        }
    }

    fn eat_word(&mut self, w: &str) -> bool {
        let hit = self.s[self.pos..].starts_with(w.as_bytes());
        if hit {
            self.pos += w.len();
        }
        hit
    }

    fn name(&mut self) -> String {
        self.skip_ws();
        let start = self.pos;
        while self.peek().is_some_and(is_name_char) {
            self.pos += 1;
        }
        String::from_utf8_lossy(&self.s[start..self.pos]).into_owned()
    }

    fn until(&mut self, end: u8) -> Result<String, String> {
        let start = self.pos;
        while self.peek().is_some_and(|c| c != end) {
            self.pos += 1;
        }
        if self.peek().is_none() {
            return self.err(&format!("missing '{}'", end as char));
        }
        let s = String::from_utf8_lossy(&self.s[start..self.pos]).into_owned();
        self.pos += 1;
        Ok(s)
    }

    /// Skips to the end of the current statement (`;` at this nesting level).
    fn skip_statement(&mut self) -> Result<(), String> {
        let mut depth = 0usize;
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                b'"' => {
                    self.pos -= 1;
                    self.string()?;
                }
                b'{' => depth += 1,
                b'}' => depth = depth.saturating_sub(1),
                b';' if depth == 0 => return Ok(()),
                _ => {}
            }
        }
        self.err("missing ';'")
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = Vec::new();
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                b'"' => return Ok(String::from_utf8_lossy(&out).into_owned()),
                b'\\' => {
                    let e = self.peek().unwrap_or(b'\\');
                    self.pos += 1;
                    out.push(match e {
                        b'n' => b'\n',
                        b't' => b'\t',
                        e => e,
                    });
                }
                c => out.push(c),
            }
        }
        self.err("unterminated string")
    }

    /// `label: label2: name` — returns the name, collecting labels.
    fn labels_and_name(&mut self, labels: &mut Vec<String>) -> Result<String, String> {
        loop {
            let name = self.name();
            if name.is_empty() {
                return self.err("expected a name");
            }
            self.skip_ws();
            if self.peek() == Some(b':') {
                self.pos += 1;
                labels.push(name);
            } else {
                return Ok(name);
            }
        }
    }

    /// `{ ... };`
    fn block(&mut self) -> Result<Node, String> {
        self.expect(b'{')?;
        let mut node = Node::default();
        loop {
            self.skip_ws();
            match self.peek() {
                None => return self.err("missing '}'"),
                Some(b'}') => {
                    self.pos += 1;
                    self.expect(b';')?;
                    return Ok(node);
                }
                Some(b';') => self.pos += 1,
                Some(b'/') => {
                    // /delete-property/, /delete-node/
                    self.skip_statement()?;
                }
                _ => {
                    let mut labels = Vec::new();
                    let name = self.labels_and_name(&mut labels)?;
                    self.skip_ws();
                    match self.peek() {
                        Some(b'{') => {
                            let mut child = self.block()?;
                            child.name = name;
                            child.labels = labels;
                            node.children.push(child);
                        }
                        Some(b'=') => {
                            self.pos += 1;
                            let values = self.values()?;
                            node.props.push((name, values));
                        }
                        Some(b';') => {
                            self.pos += 1;
                            node.props.push((name, Vec::new()));
                        }
                        _ => return self.err("expected '{', '=' or ';'"),
                    }
                }
            }
        }
    }

    /// Property values up to and including the `;`.
    fn values(&mut self) -> Result<Vec<Value>, String> {
        let mut values = Vec::new();
        loop {
            self.skip_ws();
            // Labels inside values (`foo = label: <1>;`) are allowed but unused.
            match self.peek() {
                Some(b'"') => values.push(Value::Str(self.string()?)),
                Some(b'<') => {
                    self.pos += 1;
                    values.push(Value::Cells(self.cells()?));
                }
                Some(b'[') => {
                    self.pos += 1;
                    let hex: String = self.until(b']')?.split_whitespace().collect();
                    let bytes = (0..hex.len() / 2).filter_map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok());
                    values.push(Value::Bytes(bytes.collect()));
                }
                Some(b'&') => {
                    self.pos += 1;
                    values.push(Value::Ref(self.name()));
                }
                _ => return self.err("expected a property value"),
            }
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b';') => {
                    self.pos += 1;
                    return Ok(values);
                }
                _ => return self.err("expected ',' or ';'"),
            }
        }
    }

    /// Cells after `<`, up to and including `>`.
    fn cells(&mut self) -> Result<Vec<Cell>, String> {
        let mut cells = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                None => return self.err("missing '>'"),
                Some(b'>') => {
                    self.pos += 1;
                    return Ok(cells);
                }
                Some(b'&') => {
                    self.pos += 1;
                    if self.peek() == Some(b'{') {
                        self.pos += 1;
                        cells.push(Cell::Ref(self.until(b'}')?));
                    } else {
                        cells.push(Cell::Ref(self.name()));
                    }
                }
                Some(b'(') => {
                    let text = self.parenthesized()?;
                    let toks = super::pp::lex(&text);
                    let words: Vec<&str> = toks.iter().map(|t| t.text.as_str()).collect();
                    cells.push(Cell::Num(expr::eval(&words).map_err(|e| format!("{e} in {text}"))?));
                }
                Some(b'\'') => {
                    self.pos += 1;
                    let c = self.until(b'\'')?;
                    cells.push(Cell::Num(c.chars().next().map_or(0, |c| c as i64)));
                }
                _ => {
                    let start = self.pos;
                    while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-') {
                        self.pos += 1;
                    }
                    let word = String::from_utf8_lossy(&self.s[start..self.pos]).into_owned();
                    let n = word.strip_prefix('-').map_or_else(|| expr::number(&word), |w| expr::number(w).map(|v| -v));
                    match n {
                        Some(n) => cells.push(Cell::Num(n)),
                        None if word.is_empty() => return self.err("unexpected character in cells"),
                        // An undefined name (e.g. a key name we don't know): keep the slot.
                        None => return Err(format!("unknown name {word:?} in bindings")),
                    }
                }
            }
        }
    }

    /// A balanced `( ... )`, returned with the parentheses.
    fn parenthesized(&mut self) -> Result<String, String> {
        let start = self.pos;
        let mut depth = 0usize;
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(String::from_utf8_lossy(&self.s[start..self.pos]).into_owned());
                    }
                }
                _ => {}
            }
        }
        self.err("missing ')'")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nodes_labels_and_values() {
        let src = r#"/dts-v1/;
            / {
                behaviors { hm: homerow_mods { compatible = "zmk,behavior-hold-tap"; #binding-cells = <2>;
                    flavor = "balanced"; bindings = <&kp>, <&kp>; }; };
                keymap { compatible = "zmk,keymap";
                    base { display-name = "Base"; bindings = <&kp 0x70004 &mo 1 &hm (2 | 0) -1>; };
                    extra { status = "reserved"; };
                };
            };
            &lt { tapping-term-ms = <200>; };
            /delete-node/ &foo;"#;
        let root = parse(src).unwrap();
        let mut keymap = None;
        root.walk(&mut |n| {
            if n.string("compatible") == Some("zmk,keymap") {
                keymap = Some(n);
            }
        });
        let keymap = keymap.unwrap();
        assert_eq!(keymap.children.len(), 2);
        let base = &keymap.children[0];
        assert_eq!(base.string("display-name"), Some("Base"));
        use Cell::*;
        assert_eq!(
            base.cells("bindings"),
            vec![Ref("kp".into()), Num(0x70004), Ref("mo".into()), Num(1), Ref("hm".into()), Num(2), Num(-1)]
        );
        let hm = &root.children[0].children[0];
        assert_eq!(hm.labels, vec!["hm"]);
        assert_eq!(hm.cells("#binding-cells"), vec![Num(2)]);
        assert_eq!(hm.cells("bindings"), vec![Ref("kp".into()), Ref("kp".into())]);
        assert_eq!(root.children[2].name, "&lt");
    }

    #[test]
    fn errors_point_at_the_problem() {
        let e = parse("/ { keymap { bindings = <&kp FOO>; }; };").unwrap_err();
        assert!(e.contains("FOO"), "{e}");
        assert!(parse("/ { a { b = <1>; };").is_err());
    }
}
