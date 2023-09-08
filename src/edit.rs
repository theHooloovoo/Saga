
use std::{
    num::{ParseFloatError, ParseIntError},
    str::{
        FromStr,
        SplitAsciiWhitespace as SplitAscii,
    },
};

use super::{
    MainError,
    events::{Dates, DtParseError, Event, Node, Query},
};

#[derive(Debug)]
pub enum ValueType { Node, Event }

pub type EvalResult = Result<(), EvalError>;
#[derive(Debug)]
pub enum EvalError {
    NotApplicable(ValueType, Command),
    IndexError{index:usize, len:usize},
}

#[derive(Debug, PartialEq)]
pub enum ParseError {
    MissingCommand,
    MissingArgument,
    ExtraArgument(String, String),
    UnknownCommand(String, Option<String>),
    NotAFloat(ParseFloatError),
    NotAInt(ParseIntError),
    NotADT(DtParseError),
}

#[derive(Debug, PartialEq)]
enum Mod { Add, Sub, Edit }

/// Strips the modifier (if present), converts it to Modifier, and packages
/// it with the rest of the string.
fn get_mod<'a>(word: &'a str) -> (Mod, &'a str) {
    let add = word.strip_prefix('+').map(|s|(Mod::Add, s));
    let sub = word.strip_prefix('-').map(|s|(Mod::Sub, s));
    add.or(sub).unwrap_or( (Mod::Edit, word) )
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Exit,
    Help,
    NameSub,
    NameEdit(Option<String>),
    DescAdd(Option<String>),
    DescSub(usize),
    DescEdit(usize, Option<String>),
    LineEdit(Option<Option<f64>>),
    Offset(f64),
    Scale(f64),
    DateEdit(Dates),
    // NodeAdd(NodePath, Box<Node>),
    // NodeSub(usize),
    // Copy(NodePath),              // from <selected@path> and push into <register>,
    // Cut(NodePath),               // from <selected@path> and push into <register>.
    // Paste(NodePath, NodePath),   // from <reg[index]> to <selected@path>.
    // Move(NodePath, NodePath),    // from <reg[index]> to <reg[index]>.
}

impl Command {

    pub fn is_exit(&self) -> bool {
        match self {
            Command::Exit => true,
            _ => false,
        }
    }

    pub fn is_help(&self) -> bool {
        match self {
            Command::Help => true,
            _ => false,
        }
    }

    /// Wrapper that decides whether to use eval_node() or eval_query().
    pub fn eval_query(&self, query: &mut Query) -> EvalResult {
        match query {
            Query::Node(node) => self.eval_node(node),
            Query::Event(event) => self.eval_event(event),
        }
    }

    pub fn eval_node(&self, node: &mut Node) -> EvalResult {
        match self {
            // Non-supported Node commands ================
            Command::Exit        |
            Command::Help        |
            Command::DateEdit(_) => {
                Err(EvalError::NotApplicable(ValueType::Event, self.clone()))
            },
            // Name Commands ==============================
            Command::NameSub => {
                node.set_name(None);
                Ok(())
            },
            Command::NameEdit(opt_name) => {
                node.set_name(opt_name.as_ref().map(|s|s.as_str()));
                Ok(())
            },
            // Line Commands ==============================
            Command::LineEdit(opt_opt_f64) => {
                node.set_line(*opt_opt_f64);
                Ok(())
            },
            // Offset Commands ============================
            Command::Offset(n) => {
                node.set_offset(n);
                Ok(())
            },
            // Scale Commands =============================
            Command::Scale(n) => {
                node.set_scale(n);
                Ok(())
            },
            // Pass the buck to the child event.
            Command::DescAdd(_) |
            Command::DescSub(_) |
            Command::DescEdit(_,_) => {
                Err(EvalError::NotApplicable(ValueType::Node, self.clone()))
            },
        }
    }

    pub fn eval_event(&self, event: &mut Event) -> EvalResult {
        match self {
            Command::Exit        |
            Command::Help        |
            Command::Offset(_)   |
            Command::Scale(_)    |
            Command::NameSub     |
            Command::LineEdit(_) => {
                Err(EvalError::NotApplicable(ValueType::Event, self.clone()))
            },
            Command::NameEdit(opt_name) => {
                match opt_name {
                    Some(name) => {
                        event.set_name(name);
                        Ok(())
                    },
                    None => {
                        Err(EvalError::NotApplicable(ValueType::Node, self.clone()))
                    },
                }
            },
            Command::DescAdd(opt_str) => {
                match opt_str {
                    Some(s) => {
                        event.add_description(&s);
                        Ok(())
                    },
                    None => {
                        println!("TODO: Find a way to call a text editor!");
                        Ok(())
                    }
                }
            },
            Command::DescSub(index) => event.delete_description(*index),
            Command::DescEdit(index, opt_str) => {
                match opt_str {
                    Some(desc) => event.change_description(*index, &desc),
                    None => {
                        println!("TODO: Find a way to call a text editor!");
                        Ok(())
                    },
                }
            },
            Command::DateEdit(dates) => {
                event.set_dates(&dates);
                Ok(())
            },
        }
    }
}

/// Used by serde to read struct from file.
impl FromStr for Command {
    type Err = ParseError;
    fn from_str(query: &str) -> Result<Self, Self::Err> {
        /// If stream is unfinished, returns `Some` containing each token
        /// joined with a single space.
        fn tail(stream: &mut SplitAscii<'_>) -> Option<String> {
            let mut sentence: Vec<&str> = Vec::with_capacity(10);   // idk...
            while let Some(word) = stream.next() {
                sentence.push(word);
            }
            match sentence.is_empty() {
                true => None,
                false => Some(sentence[..].join(" ")),
            }
        }
        /// Parses the next token in the stream, if it's there.
        fn parse_next<T>(stream: &mut SplitAscii<'_>) -> Result<Option<T>, <T as FromStr>::Err>
        where T: std::str::FromStr {
            stream.next()
                .map(|token|token.parse::<T>())
                .transpose()
        }
        let mut tokens = query.split_ascii_whitespace();
        let (modifier, head) = get_mod(
            tokens.next().ok_or(ParseError::MissingCommand)?
        );
        // Decide what kind of Command we were given.
        let result = match (head, modifier) {
            // Exit =======================================
            ("exit", _) => Ok(Command::Exit),
            // Help =======================================
            ("help", _) => Ok(Command::Help),
            // Date =======================================
            ("date", Mod::Edit) => {
                let dt = tail(&mut tokens)
                    .ok_or(ParseError::MissingArgument)?
                    .parse::<Dates>()
                    .map_err(|e|ParseError::NotADT(e))?;
                Ok(Command::DateEdit(dt))
            },
            // Name =======================================
            ("name", Mod::Sub) => Ok(Command::NameSub),
            ("name", _) => {
                Ok(Command::NameEdit(tail(&mut tokens)))
            },
            // Description ================================
            ("desc", Mod::Sub) => {
                let n = parse_next::<usize>(&mut tokens)
                    .map_err(|e|ParseError::NotAInt(e))?
                    .ok_or(ParseError::MissingArgument)?;
                Ok(Command::DescSub(n))
            },
            ("desc", Mod::Add) => {
                Ok(Command::DescAdd(tail(&mut tokens)))
            },
            ("desc", Mod::Edit) => {
                let index = parse_next::<usize>(&mut tokens)
                    .map_err(|e|ParseError::NotAInt(e))?
                    .ok_or(ParseError::MissingArgument)?;
                Ok(Command::DescEdit(index, tail(&mut tokens)))
            },
            // Line =======================================
            ("line", Mod::Sub) => Ok(Command::LineEdit(None)),
            ("line", _) => {
                let opt_n = parse_next::<f64>(&mut tokens)
                    .map_err(|e|ParseError::NotAFloat(e))?;
                Ok(Command::LineEdit(Some(opt_n)))
            },
            // Offset =====================================
            ("offset", Mod::Sub) => Ok(Command::Offset(0.0)),
            ("offset", _) => {
                let n = parse_next::<f64>(&mut tokens)
                    .map_err(|e|ParseError::NotAFloat(e))?
                    .ok_or(ParseError::MissingArgument)?;
                Ok(Command::Offset(n))
            },
            // Scale ======================================
            ("scale", Mod::Sub) => Ok(Command::Scale(1.0)),
            ("scale", _) => {
                let n = parse_next::<f64>(&mut tokens)
                    .map_err(|e|ParseError::NotAFloat(e))?
                    .ok_or(ParseError::MissingArgument)?;
                Ok(Command::Scale(n))
            },
            (unknown, _) => {
                let (start,end) = (unknown.to_string(), tail(&mut tokens));
                Err(ParseError::UnknownCommand(start, end))
            },
        };
        // Fail if we didn't eat all the tokens.
        let tail = tokens.collect::<Vec<&str>>();
        match tail.is_empty() {
            true => result,
            false => Err(ParseError::ExtraArgument(head.to_string(), tail[..].join(" "))),
        }
    }
}

impl From<ParseError> for MainError {
    fn from(err: ParseError) -> Self {
        MainError::CommandParse(err)
    }
}

impl From<EvalError> for MainError {
    fn from(err: EvalError) -> Self {
        MainError::Eval(err)
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, get_mod, Mod, ParseError};
    use super::super::events::Dates;

    #[test]
    fn test_get_mod() {
        let ok_cases = [
            ("hello",  (Mod::Edit, "hello")),
            ("+hello", (Mod::Add, "hello")),
            ("-hello", (Mod::Sub, "hello")),
        ];
        for (left, right) in ok_cases.iter() {
            assert_eq!(get_mod(left), *right);
        }
    }

    #[test]
    fn test_command_parsing() {
        let ok_cases = [
            ("exit", Command::Exit),
            ("help", Command::Help),
            ("line", Command::LineEdit(Some(None))),
            ("line 5", Command::LineEdit(Some(Some(5.0)))),
            ("+line", Command::LineEdit(Some(None))),
            ("+line 5", Command::LineEdit(Some(Some(5.0)))),
            ("-name", Command::NameSub),
            ("name", Command::NameEdit(None)),
            ("name hello", Command::NameEdit(Some("hello".to_string()))),
            ("name hello world", Command::NameEdit(Some("hello world".to_string()))),
            ("+desc", Command::DescAdd(None)),
            ("+desc TEXT", Command::DescAdd(Some("TEXT".to_string()))),
            ("+desc Lorem Ipsum Dolor", Command::DescAdd(Some("Lorem Ipsum Dolor".to_string()))),
            ("desc 5 TEXT", Command::DescEdit(5usize, Some("TEXT".to_string()))),
            ("desc 5", Command::DescEdit(5usize, None)),
            ("-scale", Command::Scale(1.0)),
            ("scale 0.5", Command::Scale(0.5)),
            ("+scale 0.5", Command::Scale(0.5)),
            ("-offset", Command::Offset(0.0)),
            ("offset 2.0", Command::Offset(2.0)),
            ("+offset 2.0", Command::Offset(2.0)),
            ("date 1/1/1990 0:0 - 1/1/1991 0:0", Command::DateEdit("1/1/1990 0:0 - 1/1/1991 0:0".parse::<Dates>().unwrap())),
            ("date 1/1/1990 0:0", Command::DateEdit("1/1/1990 0:0".parse::<Dates>().unwrap())),
        ];
        for (left, right) in ok_cases.iter() {
            println!("{}", left);
            assert_eq!(left.parse::<Command>().unwrap(), *right);
        }
        let err_cases = [
            ( "", ParseError::MissingCommand),
            ( "+offset", ParseError::MissingArgument),
            (
                "booty buttcheeks",
                ParseError::UnknownCommand("booty".to_string(), Some("buttcheeks".to_string()))
            ),
            (
                "exit world",
                ParseError::ExtraArgument("exit".to_string(), "world".to_string())
            ),
            (
                "help world",
                ParseError::ExtraArgument("help".to_string(), "world".to_string())
            ),
            (
                "-name hello",
                ParseError::ExtraArgument("name".to_string(), "hello".to_string())
            ),
            (
                "+line 5 4",
                ParseError::ExtraArgument("line".to_string(), "4".to_string())
            ),
            (
                "+line hello",
                ParseError::NotAFloat("hello".parse::<f64>().unwrap_err())
            ),
            (
                "desc 3.14",
                ParseError::NotAInt("3.14".parse::<usize>().unwrap_err())
            ),
        ];
        for (left, right) in err_cases.iter() {
            println!("{}", left);
            assert_eq!(left.parse::<Command>().unwrap_err(), *right);
        }
    }
}
