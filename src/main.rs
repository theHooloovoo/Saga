
#![feature(iterator_try_collect)]

// Maybe call this crate SAGA?

use std::collections::HashMap;
use std::num::ParseIntError;
use std::path::PathBuf;

use clap::{arg, command, value_parser, ArgAction, ArgMatches, Command, Parser};
use serde::{Serialize, Deserialize};
use svg::{
    Document,
    Node as SvgNode,
    node::element::{path::Data,Path as SvgPath}
};

mod events;
use events::{Dates, DtParseError, PathFail, Node};
mod saga;
use saga::{parse_to_int_path,SagaDoc};

use saga::ask_user;

pub type MainResult = Result<(), MainError>;
#[derive(Debug)]
pub enum MainError {
    // TODO: Add file path to this.
    NotASagaDoc(serde_json::Error),
    FileIO(std::io::Error),
    IntoOSString(std::ffi::OsString),
    BadPathParse(ParseIntError),
    BadDateTimeParse(DtParseError),
    NodeNotFound(PathFail),
}

fn main() -> MainResult {
    let arg_parser = build_arg_parser();
    let matches = arg_parser.get_matches();
    match matches.subcommand() {
        Some(("add",   sub_matches))  => arg_add(sub_matches),
        Some(("print", sub_matches))  => arg_print(sub_matches),
        Some(("cat", sub_matches))    => arg_catenate(sub_matches),
        Some(("render", sub_matches)) => arg_render(sub_matches),
        _ => {
            todo!();
        },
    }
}

fn build_arg_parser() -> Command {
    command!()
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("add")
                .about("Adds and event to the given file at the listed location.")
                .arg(arg!(<FILE>))
                .arg(arg!(<INT_LIST>)),
        )
        .subcommand(
            Command::new("cat")
                .about("Catenate all the listed files together into DEST.")
                .arg(arg!(<FILE> ...))
                .arg(arg!(<DEST> )),
        )
        .subcommand(
            Command::new("render")
                .about("Generate an SVG file for each given FILE.")
                .arg(arg!(<FILE> ...)),
        )
        .subcommand(
            Command::new("print")
                .about("Get a rough overview of each given FILE.")
                .arg(arg!(<FILE> ...)),
        )
}

fn arg_add(sub_matches: &ArgMatches) -> MainResult {
    // Extract the raw data.
    let query: &str = sub_matches.get_one::<String>("INT_LIST")
        .expect("Clap guarantees that this should be here.");
    let fp: &str = sub_matches.get_one::<String>("FILE")
        .expect("Clap guarantees that this should be here.");
    // Wrangle it into the correct form. 
    let contents = open_file(fp).map_err(|e|MainError::FileIO(e))?;
    let mut saga: SagaDoc = serde_json::from_str(&contents)
        .map_err(|e|MainError::NotASagaDoc(e))?;
    // Do our editting.
    saga.add_event(&query)?;
    // Then write the changes to the disk.
    Ok(())
}

fn arg_print(sub_matches: &ArgMatches) -> MainResult {
    // Assume all of the paths are valid files that have been parsed correctly.
    open_saga_docs(sub_matches, "FILE")?.iter().for_each(|(fp, parsed_doc)|{
        println!("\n{}", fp);
        let node = parsed_doc.get_data();
        let range = node.range();
        if !node.is_empty() { println!("[{:<12}]", Dates::from(range)); }
        node.iter().zip(node.depth()).for_each(|(event, d)|{
            let loc_str = match event.location(range) {
                (a,None) => format!("[{:.2}]", a),
                (a,Some(b)) => format!("[{:.2}, {:.2}]", a, b),
            };
            println!("  - {:<2} {:<12} {:<35} {}", d, loc_str, event.date_string(), event.name());
        });
    });
    Ok(())
}

fn arg_catenate(sub_matches: &ArgMatches) -> MainResult {
    let saga_docs = open_saga_docs(sub_matches, "FILE")?;
    let dest = sub_matches.get_one::<String>("DEST")
        .map(|s|open_file(s))
        .expect("Clap guarantees that this should be here.")
        .map_err(|e|MainError::FileIO(e))?;
    Ok(())
}

fn arg_render(sub_matches: &ArgMatches) -> MainResult {
    for (fp,saga) in open_saga_docs(sub_matches, "FILE")?.iter() {
        let svg = saga.draw();
        let mut fp_svg = PathBuf::from(fp);
        fp_svg.set_extension("svg");
        println!("Looking at {:?}!", fp_svg);
        svg::save(fp_svg, &svg)
            .map_err(|e|MainError::FileIO(e))?;
    }
    Ok(())
}

fn open_saga_docs<'a>(sub_matches: &'a ArgMatches, tag: &str) -> Result<Vec<(&'a str, SagaDoc)>, MainError> {
    Ok(sub_matches.get_many::<String>(tag)
        .expect("Flying on a prayer.")
        .map(|fp|(fp, open_file(fp)))
        .map(|(fp,res)|res.map(|f|(fp,f)))  // Wrap fp inside the Result, so we can call try on it.
        .try_collect::<Vec<_>>()
        .map_err(|e|MainError::FileIO(e))?
        .iter() // Re-iterate after collecting.
        .map(|(fp,file)|(fp, serde_json::from_str::<SagaDoc>(file) ))
        .map(|(fp,res)|res.map(|r|(fp.as_str(),r)))  // Wrap fp inside the Result, so we can call try on it.
        .try_collect::<Vec<_>>()
        .map_err(|e|MainError::NotASagaDoc(e))?)
}

fn open_file(file_path: &str) -> std::io::Result<String> {
    use std::fs::File;
    use std::io::Read;
    let mut file = File::open(file_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
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
            vec!["saga", "add", "file", "path"],
        ];
        for sentence in ok_cases.iter() {
            let parse = arg_parser.clone().try_get_matches_from(sentence);
            assert!(parse.is_ok(), "{:?}", sentence);
        }
    }
}

