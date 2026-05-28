// Copyright (C) 2026 themadorg
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tokenizer compatible with Madmail [`framework/config/lexer`](../../context/madmail/framework/config/lexer).

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub line: u32,
    pub text: String,
}

pub fn lex_all(input: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(input);
    let mut tokens = Vec::new();
    while lexer.next() {
        tokens.push(lexer.token.clone());
    }
    tokens
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: u32,
    token: Token,
}

impl Lexer {
    fn new(input: &str) -> Self {
        let mut chars: Vec<char> = input.chars().collect();
        if chars.first() == Some(&'\u{FEFF}') {
            chars.remove(0);
        }
        Self {
            chars,
            pos: 0,
            line: 1,
            token: Token {
                line: 1,
                text: String::new(),
            },
        }
    }

    fn next(&mut self) -> bool {
        let mut val = String::new();
        let mut comment = false;
        let mut quoted = false;
        let mut escaped = false;
        let mut token_line = self.line;

        loop {
            let Some(ch) = self.read_rune() else {
                if !val.is_empty() {
                    self.token = Token {
                        line: token_line,
                        text: val,
                    };
                    return true;
                }
                return false;
            };

            if quoted {
                if !escaped {
                    if ch == '\\' {
                        escaped = true;
                        continue;
                    }
                    if ch == '"' {
                        self.token = Token {
                            line: token_line,
                            text: val,
                        };
                        return true;
                    }
                }
                if ch == '\n' {
                    self.line += 1;
                }
                if escaped {
                    if ch != '"' {
                        val.push('\\');
                    }
                }
                val.push(ch);
                escaped = false;
                continue;
            }

            if ch.is_whitespace() {
                if ch == '\r' {
                    continue;
                }
                if ch == '\n' {
                    self.line += 1;
                    comment = false;
                }
                if !val.is_empty() {
                    self.token = Token {
                        line: token_line,
                        text: val,
                    };
                    return true;
                }
                continue;
            }

            if ch == '#' {
                comment = true;
            }
            if comment {
                continue;
            }

            if val.is_empty() {
                token_line = self.line;
                if ch == '"' {
                    quoted = true;
                    continue;
                }
            }

            val.push(ch);
        }
    }

    fn read_rune(&mut self) -> Option<char> {
        if self.pos >= self.chars.len() {
            return None;
        }
        let ch = self.chars[self.pos];
        self.pos += 1;
        Some(ch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize(input: &str) -> Vec<Token> {
        lex_all(input)
    }

    #[test]
    fn host_port_and_braces() {
        let tokens = tokenize("host:123 {\n\tdirective\n}");
        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[0].text, "host:123");
        assert_eq!(tokens[1].text, "{");
        assert_eq!(tokens[2].text, "directive");
        assert_eq!(tokens[3].text, "}");
    }

    #[test]
    fn quoted_value_with_spaces() {
        let tokens = tokenize(r#"a "quoted value" b"#);
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[1].text, "quoted value");
    }

    #[test]
    fn escaped_quotes_inside_string() {
        let tokens = tokenize(r#"A "quoted \"value\" inside" B"#);
        assert_eq!(tokens[1].text, r#"quoted "value" inside"#);
    }

    #[test]
    fn strips_bom() {
        let tokens = tokenize("\u{FEFF}:8080");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text, ":8080");
    }
}
