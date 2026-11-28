#[derive(Clone, Debug)]
struct Token {
    kind: TokenKind,
    text: String,
    line: usize,
    column: usize,
    source_length: usize,
    /// The module this token was read from. `0` is the entry file being compiled;
    /// library modules loaded through `from "..." use` get their own IDs.
    file: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenKind {
    Identifier,
    Number,
    String,
    Symbol,
    Eof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceTokenKind {
    Identifier,
    Number,
    String,
    Symbol,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceToken {
    pub kind: SourceTokenKind,
    pub text: String,
    pub line: u32,
    pub start_character: u32,
    pub length: u32,
}

impl Token {
    fn new(kind: TokenKind, text: impl Into<String>, line: usize, column: usize) -> Self {
        let text = text.into();
        Self {
            kind,
            source_length: text.encode_utf16().count(),
            text,
            line,
            column,
            file: 0,
        }
    }

    fn describe(&self) -> String {
        if self.kind == TokenKind::Eof {
            "end of file".to_string()
        } else {
            format!("{:?}", self.text)
        }
    }
}

/// Separates the module ID that prefixes diagnostics raised inside library modules.
const MODULE_DIAGNOSTIC_MARK: char = '\u{1}';

fn diagnostic(token: &Token, message: impl AsRef<str>) -> String {
    let located = format!("{}:{}: {}", token.line, token.column, message.as_ref());
    if token.file == 0 {
        located
    } else {
        module_diagnostic(token.file, located)
    }
}

/// Tag a `line:column: message` diagnostic with the library module it belongs to.
fn module_diagnostic(file: u32, message: String) -> String {
    if file == 0 || message.starts_with(MODULE_DIAGNOSTIC_MARK) {
        return message;
    }
    format!("{MODULE_DIAGNOSTIC_MARK}{file}{MODULE_DIAGNOSTIC_MARK}{message}")
}

/// Split a diagnostic into its library module ID (`0` for the entry file) and the plain message.
fn split_module_diagnostic(message: &str) -> (u32, &str) {
    if let Some(rest) = message.strip_prefix(MODULE_DIAGNOSTIC_MARK) {
        if let Some((file, message)) = rest.split_once(MODULE_DIAGNOSTIC_MARK) {
            if let Ok(file) = file.parse() {
                return (file, message);
            }
        }
    }
    (0, message)
}

/// Lex a `.rk` source or `.rks` scene. `[` and `]` index text in sources and
/// delimit lists in scenes.
fn lex(source: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let (mut index, mut line, mut column) = (0, 1, 1);
    while index < chars.len() {
        let current = chars[index];
        if current == '\n' {
            line += 1;
            column = 1;
            index += 1;
            continue;
        }
        if current.is_whitespace() {
            index += 1;
            column += 1;
            continue;
        }
        if current == '/' && chars.get(index + 1) == Some(&'/') {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
                column += 1;
            }
            continue;
        }
        let start_line = line;
        let start_column = column;
        if current.is_ascii_alphabetic() || current == '_' {
            let start = index;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || chars[index] == '_')
            {
                index += 1;
                column += 1;
            }
            let text: String = chars[start..index].iter().collect();
            tokens.push(Token::new(
                TokenKind::Identifier,
                text,
                start_line,
                start_column,
            ));
            continue;
        }
        if current.is_ascii_digit() {
            let start = index;
            while index < chars.len() && chars[index].is_ascii_digit() {
                index += 1;
                column += 1;
            }
            if index + 1 < chars.len() && chars[index] == '.' && chars[index + 1].is_ascii_digit() {
                index += 1;
                column += 1;
                while index < chars.len() && chars[index].is_ascii_digit() {
                    index += 1;
                    column += 1;
                }
            }
            let text: String = chars[start..index].iter().collect();
            tokens.push(Token::new(
                TokenKind::Number,
                text,
                start_line,
                start_column,
            ));
            continue;
        }
        if current == '"' {
            let source_start = index;
            index += 1;
            column += 1;
            let mut value = String::new();
            let mut terminated = false;
            while index < chars.len() {
                match chars[index] {
                    '"' => {
                        index += 1;
                        column += 1;
                        terminated = true;
                        break;
                    }
                    '\\' => {
                        index += 1;
                        column += 1;
                        let Some(escaped) = chars.get(index).copied() else {
                            break;
                        };
                        value.push(match escaped {
                            'n' => '\n',
                            'r' => '\r',
                            't' => '\t',
                            '"' => '"',
                            '\\' => '\\',
                            other => {
                                return Err(diagnostic(
                                    &Token::new(TokenKind::String, value, start_line, start_column),
                                    format!("unsupported string escape \\{other}"),
                                ));
                            }
                        });
                        index += 1;
                        column += 1;
                    }
                    '\n' => {
                        return Err(diagnostic(
                            &Token::new(TokenKind::String, value, start_line, start_column),
                            "string literal cannot contain a newline",
                        ));
                    }
                    character => {
                        value.push(character);
                        index += 1;
                        column += 1;
                    }
                }
            }
            if !terminated {
                return Err(diagnostic(
                    &Token::new(TokenKind::String, value, start_line, start_column),
                    "unterminated string literal",
                ));
            }
            let mut token = Token::new(TokenKind::String, value, start_line, start_column);
            token.source_length = chars[source_start..index]
                .iter()
                .map(|character| character.len_utf16())
                .sum();
            tokens.push(token);
            continue;
        }
        let pair = chars
            .get(index..index + 2)
            .map(|slice| slice.iter().collect::<String>());
        if let Some(pair) = pair.filter(|pair| {
            matches!(
                pair.as_str(),
                "==" | "!="
                    | "<="
                    | ">="
                    | "&&"
                    | "||"
                    | "+="
                    | "-="
                    | "*="
                    | "/="
                    | "%="
                    | "++"
                    | "--"
                    | "->"
                    | "=>"
            )
        }) {
            tokens.push(Token::new(
                TokenKind::Symbol,
                pair,
                start_line,
                start_column,
            ));
            index += 2;
            column += 2;
            continue;
        }
        if "{}()[]:;,.+-*/%<>=!".contains(current) {
            tokens.push(Token::new(
                TokenKind::Symbol,
                current.to_string(),
                start_line,
                start_column,
            ));
            index += 1;
            column += 1;
            continue;
        }
        return Err(diagnostic(
            &Token::new(TokenKind::Symbol, current.to_string(), line, column),
            format!("unexpected character {current:?}"),
        ));
    }
    tokens.push(Token::new(TokenKind::Eof, "", line, column));
    Ok(tokens)
}

pub fn source_tokens(source: &str) -> Result<Vec<SourceToken>, String> {
    editor_tokens(source, lex(source)?)
}

/// Editor tokens for a `.rks` scene file, which also contains `[` and `]`.
pub fn scene_source_tokens(source: &str) -> Result<Vec<SourceToken>, String> {
    editor_tokens(source, lex(source)?)
}

fn editor_tokens(source: &str, tokens: Vec<Token>) -> Result<Vec<SourceToken>, String> {
    let mut result = Vec::new();
    for token in tokens {
        let kind = match token.kind {
            TokenKind::Identifier => SourceTokenKind::Identifier,
            TokenKind::Number => SourceTokenKind::Number,
            TokenKind::String => SourceTokenKind::String,
            TokenKind::Symbol => SourceTokenKind::Symbol,
            TokenKind::Eof => continue,
        };
        let line_text = source.split('\n').nth(token.line - 1).unwrap_or("");
        let start_character = line_text
            .trim_end_matches('\r')
            .chars()
            .take(token.column - 1)
            .map(char::len_utf16)
            .sum::<usize>() as u32;
        result.push(SourceToken {
            kind,
            text: token.text.clone(),
            line: (token.line - 1) as u32,
            start_character,
            length: token.source_length as u32,
        });
    }
    Ok(result)
}
