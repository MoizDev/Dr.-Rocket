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

/// Every datum in `text` (a whole program).
pub fn read_all(chars: &[char]) -> Vec<Node> {
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        skip_space(chars, &mut i);
        if i >= chars.len() {
            return out;
        }
        match datum(chars, &mut i) {
            Some(n) => out.push(n),
            // A stray close bracket.
            None => i += 1,
        }
    }
}

/// The nodes from a top-level form down to the one covering exactly `start..end`.
pub fn path_to(forms: &[Node], start: usize, end: usize) -> Option<Vec<&Node>> {
    let mut path = Vec::new();
    let mut level = forms;
    loop {
        let n = level.iter().find(|n| n.start <= start && end <= n.end)?;
        path.push(n);
        if (n.start, n.end) == (start, end) {
            return Some(path);
        }
        level = n.items();
    }
}

fn skip_space(c: &[char], i: &mut usize) {
    while *i < c.len() {
        if c[*i].is_whitespace() {
            *i += 1;
        } else if c[*i] == ';' {
            while *i < c.len() && c[*i] != '\n' {
                *i += 1;
            }
        } else if c[*i] == '#' && c.get(*i + 1) == Some(&'|') {
            // Block comments nest.
            let mut depth = 0;
            while *i < c.len() {
                if c[*i] == '#' && c.get(*i + 1) == Some(&'|') {
                    depth += 1;
                    *i += 2;
                } else if c[*i] == '|' && c.get(*i + 1) == Some(&'#') {
                    depth -= 1;
                    *i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    *i += 1;
                }
            }
        } else if c[*i] == '#' && c.get(*i + 1) == Some(&';') {
            // A datum comment: skip the next datum.
            *i += 2;
            datum(c, i);
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
    fn whole_programs_with_comments_and_paths_into_them() {
        let text: Vec<char> = "#| a (block) |# (f 1) #;(skip me) (g (h 2))".chars().collect();
        let forms = read_all(&text);
        assert_eq!(forms.iter().map(|n| n.head()).collect::<Vec<_>>(), vec![Some("f"), Some("g")]);
        let h = text.iter().collect::<String>().find("(h 2)").unwrap();
        let path = path_to(&forms, h, h + 5).unwrap();
        assert_eq!(path.iter().map(|n| n.head()).collect::<Vec<_>>(), vec![Some("g"), Some("h")]);
        assert!(path_to(&forms, h, h + 4).is_none());
    }

    #[test]
    fn quotes_strings_and_characters_are_atoms() {
        let n = read(r#"[f '() "a ) b" #\( 'x]"#).unwrap();
        let atoms: Vec<&str> = n.items().iter().filter_map(Node::atom).collect();
        assert_eq!(atoms, vec!["f", "'()", "\"a ) b\"", "#\\(", "'x"]);
    }
}
