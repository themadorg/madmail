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

//! Parser compatible with Madmail [`framework/cfgparser`](../../context/madmail/framework/cfgparser).

use std::collections::HashMap;

use crate::madmail_lexer::{lex_all, Token};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub name: String,
    pub args: Vec<String>,
    /// `None` = not a block; `Some([])` = empty block.
    pub children: Option<Vec<Node>>,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub line: u32,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parsed configuration tree plus top-level `$(name) = …` macro values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigAst {
    pub nodes: Vec<Node>,
    pub macros: HashMap<String, Vec<String>>,
}

pub fn read(content: &str) -> Result<ConfigAst, ParseError> {
    let tokens = lex_all(content);
    let mut ctx = ParseContext {
        tokens,
        cursor: -1,
        nesting: -1,
        macros: HashMap::new(),
    };
    let children = ctx.read_nodes()?;
    if ctx.nesting > 0 {
        return Err(ctx.err("unexpected EOF when looking for }"));
    }
    Ok(ConfigAst {
        nodes: expand_environment(children),
        macros: ctx.macros,
    })
}

struct ParseContext {
    tokens: Vec<Token>,
    /// Dispenser cursor; starts at -1 (before the first token), like Madmail's lexer.
    cursor: i32,
    nesting: i32,
    macros: HashMap<String, Vec<String>>,
}

impl ParseContext {
    fn err(&self, msg: impl Into<String>) -> ParseError {
        ParseError {
            message: msg.into(),
            line: self.line(),
        }
    }

    fn line(&self) -> u32 {
        if self.cursor < 0 || self.cursor as usize >= self.tokens.len() {
            self.tokens.last().map(|t| t.line).unwrap_or(1)
        } else {
            self.tokens[self.cursor as usize].line
        }
    }

    fn val(&self) -> &str {
        if self.cursor < 0 {
            return "";
        }
        self.tokens
            .get(self.cursor as usize)
            .map(|t| t.text.as_str())
            .unwrap_or("")
    }

    fn next(&mut self) -> bool {
        if self.cursor < self.tokens.len() as i32 - 1 {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn num_line_breaks(&self, idx: i32) -> u32 {
        if idx < 0 {
            return 0;
        }
        self.tokens
            .get(idx as usize)
            .map(|t| t.text.matches('\n').count() as u32)
            .unwrap_or(0)
    }

    fn next_arg(&mut self) -> bool {
        if self.cursor < 0 {
            self.cursor = 0;
            return !self.tokens.is_empty();
        }
        if self.cursor as usize >= self.tokens.len() {
            return false;
        }
        if self.cursor < self.tokens.len() as i32 - 1 {
            let cur = &self.tokens[self.cursor as usize];
            let next = &self.tokens[self.cursor as usize + 1];
            if cur.line + self.num_line_breaks(self.cursor) == next.line {
                self.cursor += 1;
                return true;
            }
        }
        false
    }

    fn next_line(&mut self) -> bool {
        if self.cursor < 0 {
            self.cursor = 0;
            return !self.tokens.is_empty();
        }
        if self.cursor as usize >= self.tokens.len() {
            return false;
        }
        if self.cursor < self.tokens.len() as i32 - 1 {
            let cur = &self.tokens[self.cursor as usize];
            let next = &self.tokens[self.cursor as usize + 1];
            if cur.line + self.num_line_breaks(self.cursor) < next.line {
                self.cursor += 1;
                return true;
            }
        }
        false
    }

    fn read_node(&mut self) -> Result<Node, ParseError> {
        let line = self.line();
        if self.val() == "{" {
            return Err(self.err("block header"));
        }

        let mut node = Node {
            name: self.val().to_string(),
            args: Vec::new(),
            children: None,
            line,
        };

        if let Some(name) = is_snippet(&node.name) {
            node.name = name;
        }

        let mut continue_on_lf = false;
        loop {
            loop {
                if !(self.next_arg() || (continue_on_lf && self.next_line())) {
                    break;
                }
                continue_on_lf = false;
                if self.val() == "{" {
                    node.children = Some(self.read_nodes()?);
                    break;
                }
                node.args.push(self.val().to_string());
            }

            if let Some(last) = node.args.last_mut() {
                if last.ends_with('\\') {
                    let stripped = last.trim_end_matches('\\').to_string();
                    if stripped.is_empty() {
                        node.args.pop();
                    } else {
                        *last = stripped;
                    }
                    continue_on_lf = true;
                    continue;
                }
            }
            break;
        }

        if is_macro_decl(&node) {
            if self.nesting != 0 {
                return Err(self.err("macro declarations are only allowed at top-level"));
            }
            let macro_name = node.name[2..node.name.len() - 1].to_string();
            node.name = macro_name.clone();
            node.args = node.args[1..].to_vec();
            self.expand_macros(&mut node)?;
            self.macros.insert(macro_name, node.args.clone());
            node.name.clear();
            return Ok(node);
        }

        if !is_snippet_name(&node.name) {
            validate_node_name(&node.name)?;
        }

        self.expand_macros(&mut node)?;
        Ok(node)
    }

    fn read_nodes(&mut self) -> Result<Vec<Node>, ParseError> {
        if self.nesting > 255 {
            return Err(self.err("nesting limit reached"));
        }
        self.nesting += 1;
        let mut res = Vec::new();
        let mut require_newline = false;

        loop {
            if require_newline {
                if !self.next_line() {
                    if !self.next() {
                        self.nesting -= 1;
                        return Ok(res);
                    }
                    return Err(self.err("newline is required after closing brace"));
                }
            } else if !self.next() {
                break;
            }

            if self.val() == "}" {
                self.nesting -= 1;
                if self.nesting < 0 {
                    return Err(self.err("unexpected }"));
                }
                break;
            }

            let mut node = self.read_node()?;
            require_newline = true;

            let mut should_stop = false;
            if let Some(last) = node.args.last() {
                if last == "}" {
                    self.nesting -= 1;
                    if self.nesting < 0 {
                        return Err(self.err("unexpected }"));
                    }
                    node.args.pop();
                    should_stop = true;
                }
            }

            if is_snippet_name(&node.name) {
                if self.nesting != 1 {
                    return Err(self.err("snippet declarations are only allowed at top-level"));
                }
                if !node.args.is_empty() {
                    return Err(self.err("snippet declarations can't have arguments"));
                }
                continue;
            }

            if node.name.is_empty() {
                continue;
            }

            res.push(node);
            if should_stop {
                break;
            }
        }

        Ok(res)
    }

    fn expand_macros(&mut self, node: &mut Node) -> Result<(), ParseError> {
        if node.name.starts_with("$(") && node.name.ends_with(')') {
            return Err(self.err("can't use macro argument as directive name"));
        }

        let mut new_args = Vec::new();
        for arg in std::mem::take(&mut node.args) {
            if arg.starts_with("$(") && arg.ends_with(')') {
                let macro_name = &arg[2..arg.len() - 1];
                if let Some(replacement) = self.macros.get(macro_name) {
                    new_args.extend(replacement.iter().cloned());
                }
                continue;
            }
            let expanded = if arg.contains("$(") && arg.contains(')') {
                expand_single_value_macro(&arg, &self.macros)?
            } else {
                arg
            };
            new_args.push(expanded);
        }
        node.args = new_args;

        if let Some(children) = node.children.as_mut() {
            for child in children.iter_mut() {
                self.expand_macros(child)?;
            }
        }
        Ok(())
    }
}

fn is_snippet(name: &str) -> Option<String> {
    if name.starts_with('(') && name.ends_with(')') {
        Some(name[1..name.len() - 1].to_string())
    } else {
        None
    }
}

fn is_snippet_name(name: &str) -> bool {
    name.starts_with('(') && name.ends_with(')')
}

fn is_macro_decl(node: &Node) -> bool {
    node.name.starts_with("$(")
        && node.name.ends_with(')')
        && node.args.first().map(String::as_str) == Some("=")
        && node.args.len() >= 2
}

fn validate_node_name(name: &str) -> Result<(), ParseError> {
    if name.is_empty() {
        return Err(ParseError {
            message: "empty directive name".into(),
            line: 0,
        });
    }
    let mut chars = name.chars();
    if chars.next().is_some_and(|c| c.is_ascii_digit()) {
        return Err(ParseError {
            message: "directive name cannot start with a digit".into(),
            line: 0,
        });
    }
    for ch in chars {
        if !ch.is_ascii_alphanumeric() && ch != '.' && ch != '-' && ch != '_' {
            return Err(ParseError {
                message: format!("character not allowed in directive name: {ch}"),
                line: 0,
            });
        }
    }
    Ok(())
}

fn expand_single_value_macro(
    arg: &str,
    macros: &HashMap<String, Vec<String>>,
) -> Result<String, ParseError> {
    let mut out = arg.to_string();
    let mut start = 0;
    while let Some(rel) = out[start..].find("$(") {
        let abs = start + rel;
        let rest = &out[abs + 2..];
        let Some(end_rel) = rest.find(')') else {
            break;
        };
        let macro_name = &rest[..end_rel];
        let replacement = macros.get(macro_name);
        if let Some(vals) = replacement {
            if vals.len() > 1 {
                return Err(ParseError {
                    message: "can't expand macro with multiple arguments inside a string".into(),
                    line: 0,
                });
            }
            let value = vals.first().map(String::as_str).unwrap_or("");
            let placeholder = format!("$({macro_name})");
            out = out.replacen(&placeholder, value, 1);
        } else {
            start = abs + 2;
        }
    }
    Ok(out)
}

fn expand_environment(nodes: Vec<Node>) -> Vec<Node> {
    nodes
        .into_iter()
        .map(|mut node| {
            node.name = expand_env_in_string(&node.name);
            node.args = node
                .args
                .into_iter()
                .map(|a| expand_env_in_string(&a))
                .collect();
            if let Some(children) = node.children {
                node.children = Some(expand_environment(children));
            }
            node
        })
        .collect()
}

fn expand_env_in_string(s: &str) -> String {
    let mut out = s.to_string();
    while let Some(start) = out.find("{env:") {
        let Some(end_rel) = out[start..].find('}') else {
            break;
        };
        let end = start + end_rel;
        let var_name = &out[start + 5..end];
        let replacement = std::env::var(var_name).unwrap_or_default();
        out.replace_range(start..end + 1, &replacement);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_directive_with_args() {
        let ast = read("a a1 a2").unwrap();
        assert_eq!(ast.nodes.len(), 1);
        assert_eq!(ast.nodes[0].name, "a");
        assert_eq!(ast.nodes[0].args, ["a1", "a2"]);
    }

    #[test]
    fn block_with_children() {
        let cfg = r#"a a1 a2 {
            a_child1 c1arg1 c1arg2
            a_child2 c2arg1 c2arg2
        }"#;
        let ast = read(cfg).unwrap();
        assert_eq!(ast.nodes[0].name, "a");
        let children = ast.nodes[0].children.as_ref().unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].name, "a_child1");
    }

    #[test]
    fn line_continuation() {
        let ast = read("a a1 a2 \\\n   a3 a4").unwrap();
        assert_eq!(ast.nodes[0].args, ["a1", "a2", "a3", "a4"]);
    }

    #[test]
    fn macro_expansion() {
        let cfg = "$(foo) = bar\nb $(foo)";
        let ast = read(cfg).unwrap();
        assert_eq!(ast.nodes.len(), 1);
        assert_eq!(ast.nodes[0].name, "b");
        assert_eq!(ast.nodes[0].args, ["bar"]);
    }

    #[test]
    fn macro_in_string() {
        let cfg = "$(foo) = bar\nb aaa/$(foo)/bbb";
        let ast = read(cfg).unwrap();
        assert_eq!(ast.nodes[0].args, ["aaa/bar/bbb"]);
    }

    #[test]
    fn invalid_directive_name_fails() {
        assert!(read("a-a4@%8 whatever").is_err());
    }

    #[test]
    fn env_expansion() {
        std::env::set_var("CHATMAIL_CONFIG_TEST_VAR", "xyzzy");
        let ast = read("a {env:CHATMAIL_CONFIG_TEST_VAR}").unwrap();
        assert_eq!(ast.nodes[0].args, ["xyzzy"]);
    }
}
