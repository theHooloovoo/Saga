
#![feature(iterator_try_collect)]

//  Project TODO's
//    - Use iced to turn into web app and embed into website.
//      - Decide a UI layout.
//      - Decide a way to display svg's in a live manner.
//    - Maybe refactor Node to contain a vector of Values instead of Value
//      possibly being a list of Nodes? Current implementation just seems to
//      add too much nesting.
//    - Refactor the Saga::draw function into something that is more a
//      composition of functions. Specifically, use fold() to build up a data
//      path for the drawing strokes.
//    - Add support for styling.
//    - Add --verbose (-v) flag to print subcommand.

use std::{num::ParseIntError, path::PathBuf};

use clap::{arg, command, ArgMatches, Command as ClapCommand};
use serde_json::Error as JsonError;

mod events;
use events::{DtParseError, PathFail};
mod saga;
use saga::SagaDoc;
mod edit;
use edit::{Command as EvalCommand, EvalError, ParseError};
mod app;
use app::App;

pub type MainResult = Result<(), MainError>;

#[derive(Debug)]
pub enum MainError {
    // TODO: Add file path to this.
    NotASagaDoc(serde_json::Error),
    SerializeFail(JsonError),
    FileIO(std::io::Error),
    IntoOSString(std::ffi::OsString),
    BadPathParse(ParseIntError),
    BadDateTimeParse(DtParseError),
    NodeNotFound(PathFail),
    CommandParse(ParseError),
    Eval(EvalError),
    AddToEvent,
}

fn main() -> MainResult {
    let arg_parser = build_arg_parser();
    let matches = arg_parser.get_matches();
    match matches.subcommand() {
        Some(("new",     sub_matches)) => arg_new(sub_matches),
        Some(("add",     sub_matches)) => arg_add(sub_matches),
        Some(("edit",    sub_matches)) => arg_edit(sub_matches),
        Some(("grep",    _          )) => todo!("Feature Coming Soon!"),
        Some(("print",   sub_matches)) => arg_print(sub_matches),
        Some(("cat",     sub_matches)) => arg_catenate(sub_matches),
        Some(("render",  sub_matches)) => arg_render(sub_matches),
        Some(("editor",  _          )) => todo!("Feature Coming Soon!"),
        Some(("web_app", _          )) => todo!("Feature Coming Soon!"),
        _ => { unreachable!(); },
    }
}

fn build_arg_parser() -> ClapCommand {
    command!()
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            ClapCommand::new("new")
                .about("Create a new Saga document, saved at the given FILE")
                .arg(arg!(<FILE>)),
        )
        .subcommand(
            ClapCommand::new("add")
                .about("Adds and event to the given file at the listed location.")
                .arg(arg!(<FILE>))
                .arg(arg!(<INT_LIST>)),
        )
        .subcommand(
            ClapCommand::new("edit")
                .about("Adds and event to the given file at the listed location.")
                .arg(arg!(<FILE>))
                .arg(arg!(<INT_LIST>))
                .arg(arg!(<COMMAND> ...)),
        )
        .subcommand(
            ClapCommand::new("grep")
                .about("Adds and event to the given file at the listed location.")
                .arg(arg!(<QUERY>))
                .arg(arg!(<FILE> ...))
        )
        .subcommand(
            ClapCommand::new("cat")
                .about("Catenate all the listed files together into DEST.")
                .arg(arg!(<FILE> ...))
                .arg(arg!(<DEST> )),
        )
        .subcommand(
            ClapCommand::new("render")
                .about("Generate an SVG file for each given FILE.")
                .arg(arg!(<FILE> ...)),
        )
        .subcommand(
            ClapCommand::new("print")
                .about("Get a rough overview of each given FILE.")
                .arg(arg!(<FILE> ...)),
        )
        .subcommand(
            ClapCommand::new("web_app")
                .about("Get a rough overview of each given FILE.")
        )
}

fn arg_new(sub_matches: &ArgMatches) -> MainResult {
    // Extract the raw data.
    let fp: &str = sub_matches.get_one::<String>("FILE")
        .expect("Clap guarantees that this should be here.");
    // Create the new document.
    let saga: SagaDoc = SagaDoc::blank();
    let contents = saga_serialize(&saga)?;
    // Then write the changes to the disk.
    write_to_file(fp, &contents)?;
    Ok(())
}

fn arg_add(sub_matches: &ArgMatches) -> MainResult {
    // Extract the raw data.
    let query: &str = sub_matches.get_one::<String>("INT_LIST")
        .expect("Clap guarantees that this should be here.");
    let fp: &str = sub_matches.get_one::<String>("FILE")
        .expect("Clap guarantees that this should be here.");
    // Wrangle it into the correct form. 
    let mut contents = open_file(fp)?;
    let mut saga: SagaDoc = saga_deserialize(&contents)?;
    // Do our editting.
    saga.add_event(&query)?;
    // Then write the changes to the disk.
    contents.clear();
    contents = saga_serialize(&saga)?;
    write_to_file(fp, &contents)?;
    Ok(())
}

fn arg_edit(sub_matches: &ArgMatches) -> MainResult {
    // Extract the raw data.
    let fp: &str = sub_matches.get_one::<String>("FILE")
        .expect("Clap guarantees that this should be here.");
    let query: Vec<usize> = sub_matches.get_one::<String>("INT_LIST")
        .map(|s|saga::parse_to_int_path(s))
        .expect("Clap guarantees that this should be here.")?;
    let command: EvalCommand = sub_matches.get_many::<String>("COMMAND")
        .expect("Clap guarantees that this should be here.")
        .map(|s|s.to_string())
        .collect::<Vec<String>>()
        .join(" ")
        .parse::<EvalCommand>()?;
    // Wrangle it into the correct form. 
    let mut contents = open_file(fp)?;
    let mut saga: SagaDoc = saga_deserialize(&contents)?;
    let mut query = saga.get_data_mut().query(&query[..])?;
    // Commit changes to the document's data node.
    command.eval_query(&mut query)?;
    // Write back to file.
    contents = saga_serialize(&saga)?;
    write_to_file(fp, &contents)?;
    Ok(())
}

fn arg_print(sub_matches: &ArgMatches) -> MainResult {
    // Assume all of the paths are valid files that have been parsed correctly.
    open_saga_docs(sub_matches, "FILE")?.iter().for_each(|(fp, parsed_doc)|{
        println!("\n{}", fp);
        let s = parsed_doc.print(false);
        println!("{}", s);
    });
    Ok(())
}

fn arg_catenate(sub_matches: &ArgMatches) -> MainResult {
    // Get the file, parse the, then catenate them down.
    let saga_docs = open_saga_docs(sub_matches, "FILE")?
        .into_iter()
        .map(|(_,doc)|doc)
        .collect();
    let doc = SagaDoc::catenate(saga_docs);
    let contents = saga_serialize(&doc)?;
    let dest: &str = sub_matches.get_one::<String>("DEST")
        .expect("Clap guarantees that this should be here.");
    write_to_file(dest, &contents)?;
    Ok(())
}

fn arg_render(sub_matches: &ArgMatches) -> MainResult {
    for (fp,saga) in open_saga_docs(sub_matches, "FILE")?.iter() {
        let svg = saga.draw();
        let mut fp_svg = PathBuf::from(fp);
        fp_svg.set_extension("svg");
        svg::save(&fp_svg, &svg)
            .map_err(|e|MainError::FileIO(e))?;
        println!("Wrote {:?} successfully.", &fp_svg);
    }
    Ok(())
}

/// Util function used by the arg_* class of functions.
fn open_saga_docs<'a>(sub_matches: &'a ArgMatches, tag: &str) -> Result<Vec<(&'a str, SagaDoc)>, MainError> {
    // TODO rewrite this such that the Err variant returns the error AND the file path that caused it.
    Ok(sub_matches.get_many::<String>(tag)
        .expect("Flying on a prayer.")
        .map(|fp|(fp, open_file(fp)))
        .map(|(fp,res)|res.map(|f|(fp,f)))  // Wrap fp inside the Result, so we can call try on it.
        .try_collect::<Vec<_>>()?
        .iter() // Re-iterate after collecting.
        .map(|(fp,file)|(fp, serde_json::from_str::<SagaDoc>(file) ))
        .map(|(fp,res)|res.map(|r|(fp.as_str(),r)))  // Wrap fp inside the Result, so we can call try on it.
        .try_collect::<Vec<_>>()
        .map_err(|e|MainError::NotASagaDoc(e))?)
}

/// Util function used by the arg_* class of functions.
fn open_file(file_path: &str) -> Result<String, MainError> {
    use std::fs::File;
    use std::io::Read;
    let mut file = File::open(file_path)
        .map_err(|e|MainError::FileIO(e))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|e|MainError::FileIO(e))?;
    Ok(contents)
}

fn saga_deserialize(input: &str) -> Result<SagaDoc, MainError> {
    serde_json::from_str::<SagaDoc>(&input)
        .map_err(|e|MainError::NotASagaDoc(e))
}

fn saga_serialize(input: &SagaDoc) -> Result<String, MainError> {
    serde_json::to_string(&input)
        .map_err(|e|MainError::SerializeFail(e))
}

fn write_to_file(dest: &str, contents: &str) -> MainResult {
    std::fs::write(dest, contents)
        .map_err(|e|MainError::FileIO(e))
}

#[cfg(test)]
mod tests {
    use super::build_arg_parser;

    #[test]
    fn test_arg_parsing() {
        let arg_parser = build_arg_parser();
        let ok_cases = [
            vec!["saga", "print", "file1"],
            vec!["saga", "print", "file1", "file2"],
            vec!["saga", "print", "file1", "file2", "file3"],
            vec!["saga", "cat", "file1", "dest"],
            vec!["saga", "cat", "file1", "file2", "dest"],
            vec!["saga", "cat", "file1", "file2", "file3", "dest"],
            vec!["saga", "render", "file1"],
            vec!["saga", "render", "file1", "file2"],
            vec!["saga", "render", "file1", "file2", "file3"],
            vec!["saga", "add", "file1", "path"],
            vec!["saga", "edit", "file1", "1:2:4", "line"],
        ];
        for sentence in ok_cases.iter() {
            let parse = arg_parser.clone().try_get_matches_from(sentence);
            assert!(parse.is_ok(), "{:?}", sentence);
        }
    }
}

