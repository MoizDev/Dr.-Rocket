//! A small reader for program text: just enough structure to describe code in words. Positions
//! are character offsets into the text it read, so parts can be matched with recorded values.

#[derive(Clone, Debug, PartialEq)]
pub enum Sx {
    /// An identifier or literal (`k`, `12`, `"hi"`, `'()`, `#true`).
    Atom(String),
    List(Vec<Node>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub sx: Sx,
    pub start: usize,
    pub end: usize,
}

impl Node {
    pub fn atom(&self) -> Option<&str> {
        match &self.sx {
            Sx::Atom(a) => Some(a),
            Sx::List(_) => None,
        }
    }
    pub fn items(&self) -> &[Node] {
        match &self.sx {
            Sx::List(v) => v,
            Sx::Atom(_) => &[],
        }
    }
    /// The head identifier of a list: `(node-key t)` -> `node-key`.
    pub fn head(&self) -> Option<&str> {
        self.items().first().and_then(Node::atom)
    }
    /// The arguments of a list form (everything after the head).
    pub fn args(&self) -> &[Node] {
        self.items().get(1..).unwrap_or(&[])
    }
}

/// The first datum in `text`.
pub fn read(text: &str) -> Option<Node> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    datum(&chars, &mut i)
}

fn skip_space(c: &[char], i: &mut usize) {
    while *i < c.len() {
        if c[*i].is_whitespace() {
            *i += 1;
        } else if c[*i] == ';' {
            while *i < c.len() && c[*i] != '\n' {
                *i += 1;
            }
        } else {
            break;
        }
    }
}

fn is_open(ch: char) -> bool {
    matches!(ch, '(' | '[' | '{')
}

fn is_close(ch: char) -> bool {
    matches!(ch, ')' | ']' | '}')
}

fn datum(c: &[char], i: &mut usize) -> Option<Node> {
    skip_space(c, i);
    let start = *i;
    let ch = *c.get(*i)?;
    if is_close(ch) {
        return None;
    }
    if is_open(ch) {
        *i += 1;
        let mut items = Vec::new();
        loop {
            skip_space(c, i);
            match c.get(*i) {
                None => break,
                Some(ch) if is_close(*ch) => {
                    *i += 1;
                    break;
                }
                _ => match datum(c, i) {
                    Some(n) => items.push(n),
                    None => break,
                },
            }
        }
        return Some(Node { sx: Sx::List(items), start, end: *i });
    }
    // Quoted data reads as one atom with its whole text: 'x, '(1 2), `(a ,b).
    if matches!(ch, '\'' | '`' | ',') {
        *i += 1;
        if c.get(*i) == Some(&'@') {
            *i += 1;
        }
        let inner = datum(c, i);
        let end = inner.map_or(*i, |n| n.end);
        return Some(Node { sx: Sx::Atom(c[start..end].iter().collect()), start, end });
    }
    if ch == '"' {
        *i += 1;
        while *i < c.len() && c[*i] != '"' {
            if c[*i] == '\\' {
                *i += 1;
            }
            *i += 1;
        }
        *i = (*i + 1).min(c.len());
        return Some(Node { sx: Sx::Atom(c[start..*i].iter().collect()), start, end: *i });
    }
    // `#\(` is a character, not an open bracket.
    if ch == '#' && c.get(*i + 1) == Some(&'\\') {
        *i += 3;
    }
    while *i < c.len() && !c[*i].is_whitespace() && !is_open(c[*i]) && !is_close(c[*i]) && c[*i] != '"' && c[*i] != ';' {
        *i += 1;
    }
    let end = (*i).min(c.len());
    Some(Node { sx: Sx::Atom(c[start..end].iter().collect()), start, end })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nested_forms_with_positions() {
        let n = read("(< k (node-key t)) ; why").unwrap();
        assert_eq!(n.head(), Some("<"));
        assert_eq!((n.start, n.end), (0, 18));
        let inner = &n.args()[1];
        assert_eq!(inner.head(), Some("node-key"));
        assert_eq!((inner.start, inner.end), (5, 17));
        assert_eq!(n.args()[0].atom(), Some("k"));
    }

    #[test]
    fn quotes_strings_and_characters_are_atoms() {
        let n = read(r#"[f '() "a ) b" #\( 'x]"#).unwrap();
        let atoms: Vec<&str> = n.items().iter().filter_map(Node::atom).collect();
        assert_eq!(atoms, vec!["f", "'()", "\"a ) b\"", "#\\(", "'x"]);
    }
}
