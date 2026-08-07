use crate::error::{AppError, Result};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VdfValue {
    String(String),
    Object(BTreeMap<String, VdfValue>),
}

impl VdfValue {
    pub fn object(&self) -> Option<&BTreeMap<String, VdfValue>> {
        match self {
            Self::Object(value) => Some(value),
            Self::String(_) => None,
        }
    }

    pub fn string(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            Self::Object(_) => None,
        }
    }
}

pub fn parse(input: &str) -> Result<BTreeMap<String, VdfValue>> {
    let mut parser = Parser::new(input);
    let result = parser.parse_object(false)?;
    parser.skip_ignored();
    if parser.peek().is_some() {
        return Err(AppError::InvalidVdf("unexpected trailing data".into()));
    }
    Ok(result)
}

struct Parser<'a> {
    input: &'a [u8],
    offset: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            offset: 0,
        }
    }

    fn parse_object(&mut self, nested: bool) -> Result<BTreeMap<String, VdfValue>> {
        let mut values = BTreeMap::new();
        loop {
            self.skip_ignored();
            match self.peek() {
                None if nested => return Err(AppError::InvalidVdf("unterminated object".into())),
                None => return Ok(values),
                Some(b'}') if nested => {
                    self.offset += 1;
                    return Ok(values);
                }
                Some(b'}') => return Err(AppError::InvalidVdf("unexpected closing brace".into())),
                _ => {}
            }

            let key = self.parse_token()?;
            self.skip_ignored();
            let value = if self.peek() == Some(b'{') {
                self.offset += 1;
                VdfValue::Object(self.parse_object(true)?)
            } else {
                VdfValue::String(self.parse_token()?)
            };
            values.insert(key, value);
        }
    }

    fn parse_token(&mut self) -> Result<String> {
        self.skip_ignored();
        match self.peek() {
            Some(b'"') => self.parse_quoted(),
            Some(b'{') | Some(b'}') | None => Err(AppError::InvalidVdf("expected token".into())),
            Some(_) => self.parse_bare(),
        }
    }

    fn parse_quoted(&mut self) -> Result<String> {
        self.offset += 1;
        let mut output = Vec::new();
        while let Some(byte) = self.peek() {
            self.offset += 1;
            match byte {
                b'"' => {
                    return String::from_utf8(output)
                        .map_err(|_| AppError::InvalidVdf("quoted string is not UTF-8".into()));
                }
                b'\\' => {
                    let escaped = self
                        .peek()
                        .ok_or_else(|| AppError::InvalidVdf("unterminated escape".into()))?;
                    self.offset += 1;
                    match escaped {
                        b'n' => output.push(b'\n'),
                        b'r' => output.push(b'\r'),
                        b't' => output.push(b'\t'),
                        b'"' => output.push(b'"'),
                        b'\\' => output.push(b'\\'),
                        other => {
                            output.push(b'\\');
                            output.push(other);
                        }
                    }
                }
                other => output.push(other),
            }
        }
        Err(AppError::InvalidVdf("unterminated quoted string".into()))
    }

    fn parse_bare(&mut self) -> Result<String> {
        let start = self.offset;
        while let Some(byte) = self.peek() {
            if byte.is_ascii_whitespace() || matches!(byte, b'{' | b'}') {
                break;
            }
            self.offset += 1;
        }
        if self.offset == start {
            return Err(AppError::InvalidVdf("expected token".into()));
        }
        std::str::from_utf8(&self.input[start..self.offset])
            .map(str::to_string)
            .map_err(|_| AppError::InvalidVdf("token is not UTF-8".into()))
    }

    fn skip_ignored(&mut self) {
        loop {
            while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
                self.offset += 1;
            }
            if self.peek() == Some(b'/') && self.input.get(self.offset + 1) == Some(&b'/') {
                self.offset += 2;
                while self.peek().is_some_and(|byte| byte != b'\n') {
                    self.offset += 1;
                }
                continue;
            }
            break;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.offset).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_keyvalues() {
        let input = r#"
        "libraryfolders"
        {
            "0"
            {
                "path" "C:\\Program Files (x86)\\Steam"
                "apps"
                {
                    "570" "123"
                }
            }
        }
        "#;
        let parsed = parse(input).unwrap();
        let libraries = parsed["libraryfolders"].object().unwrap();
        let first = libraries["0"].object().unwrap();
        assert_eq!(first["path"].string(), Some("C:\\Program Files (x86)\\Steam"));
    }

    #[test]
    fn skips_line_comments() {
        let parsed = parse("// header\n\"root\" { \"value\" \"1\" }").unwrap();
        assert_eq!(
            parsed["root"].object().unwrap()["value"].string(),
            Some("1")
        );
    }
}
