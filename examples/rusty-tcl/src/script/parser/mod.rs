//! Tcl command parser.

use super::{
    TclError,
    ast::{TclCommand, TclWord},
    lexer::{TclToken, split_tcl_list, tokenize_words},
};

/// Tcl 命令解析器。
pub struct Parser;

impl Parser {
    /// Parse Tcl source into commands.
    pub fn parse(source: &str) -> Result<Vec<TclCommand>, TclError> {
        let word_commands = tokenize_words(source);
        if word_commands.is_empty() {
            return Err(TclError::EmptyScript);
        }
        word_commands.into_iter().map(parse_command).collect()
    }
}

fn parse_command(words: Vec<TclToken>) -> Result<TclCommand, TclError> {
    let Some(cmd) = words.first()
    else {
        return Err(TclError::EmptyCommand);
    };
    match cmd.text.as_str() {
        "set" => {
            let name = words.get(1).ok_or(TclError::MissingArgument)?.text.clone();
            let value = words.get(2).cloned().map(TclToken::into_word).unwrap_or(TclWord { text: String::new(), braced: false });
            Ok(TclCommand::Set { name, value })
        }
        "puts" => Ok(TclCommand::Puts { words: words.into_iter().skip(1).map(TclToken::into_word).collect() }),
        "expr" => {
            let expression = words.get(1).ok_or(TclError::MissingArgument)?.clone().into_word();
            Ok(TclCommand::Expr { expression })
        }
        "incr" => {
            let name = words.get(1).ok_or(TclError::MissingArgument)?.text.clone();
            let delta = words.get(2).cloned().map(TclToken::into_word);
            Ok(TclCommand::Incr { name, delta })
        }
        "if" => {
            let test = words.get(1).ok_or(TclError::MissingArgument)?.text.clone();
            let body = words.get(2).ok_or(TclError::MissingArgument)?.text.clone();
            Ok(TclCommand::If { test, body })
        }
        "while" => {
            let test = words.get(1).ok_or(TclError::MissingArgument)?.text.clone();
            let body = words.get(2).ok_or(TclError::MissingArgument)?.text.clone();
            Ok(TclCommand::While { test, body })
        }
        "for" => {
            let init = words.get(1).ok_or(TclError::MissingArgument)?.text.clone();
            let test = words.get(2).ok_or(TclError::MissingArgument)?.text.clone();
            let next = words.get(3).ok_or(TclError::MissingArgument)?.text.clone();
            let body = words.get(4).ok_or(TclError::MissingArgument)?.text.clone();
            Ok(TclCommand::For { init, test, next, body })
        }
        "foreach" => {
            let vars_token = words.get(1).ok_or(TclError::MissingArgument)?;
            let vars = if vars_token.braced || vars_token.text.contains(char::is_whitespace) {
                split_tcl_list(&vars_token.text)
            }
            else {
                vec![vars_token.text.clone()]
            };
            if vars.is_empty() {
                return Err(TclError::MissingArgument);
            }
            let list = words.get(2).ok_or(TclError::MissingArgument)?.clone().into_word();
            let body = words.get(3).ok_or(TclError::MissingArgument)?.text.clone();
            Ok(TclCommand::Foreach { vars, list, body })
        }
        "proc" => {
            let name = words.get(1).ok_or(TclError::MissingArgument)?.text.clone();
            let params = split_tcl_list(&words.get(2).ok_or(TclError::MissingArgument)?.text);
            let body = words.get(3).ok_or(TclError::MissingArgument)?.text.clone();
            Ok(TclCommand::Proc { name, params, body })
        }
        "return" => Ok(TclCommand::Return { value: words.get(1).cloned().map(TclToken::into_word) }),
        "list" => Ok(TclCommand::List { words: words.into_iter().skip(1).map(TclToken::into_word).collect() }),
        "llength" => {
            let list = words.get(1).ok_or(TclError::MissingArgument)?.clone().into_word();
            Ok(TclCommand::Llength { list })
        }
        "lindex" => {
            let list = words.get(1).ok_or(TclError::MissingArgument)?.clone().into_word();
            let index = words.get(2).ok_or(TclError::MissingArgument)?.clone().into_word();
            Ok(TclCommand::Lindex { list, index })
        }
        _ => {
            let name = cmd.text.clone();
            let args = words.into_iter().skip(1).map(TclToken::into_word).collect();
            Ok(TclCommand::Call { name, args })
        }
    }
}
