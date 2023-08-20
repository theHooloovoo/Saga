
use std::collections::HashMap;
use std::io::{
    Error as IoError,
    Write,
};

pub type DtParseError = chrono::format::ParseError;
use serde::{Serialize, Deserialize};
use svg::{
    Document,
    Node as SvgNode,
    node::element::{path::Data,Path as SvgPath}
};

use super::events::{Dates, Event, PathFail, Node};

/// Temp error type.
pub enum Bogus {
    PathParse(ParseIntError),
    PathFind(PathFail),
    DtParse(DtParseError),
    IoError(IoError),
}

pub type Colors = Vec<Color>;
#[derive(Debug, Serialize, Deserialize)]
pub struct Color {
    r: u8,
    g: u8,
    b: u8,
}

/// Root-Level wrapper for Node, that contains drawing/formatting data.
#[derive(Serialize, Deserialize)]
pub struct SagaDoc {
    x: f64,
    y: f64,
    padding: f64,
    color_schemes: HashMap<String, Colors>,
    // Font,
    data: Node,
}

impl SagaDoc {
    pub fn get_data(&self) -> &Node { &self.data }

    pub fn get_data_mut(&mut self) -> &mut Node { &mut self.data }

    pub fn draw(&self) -> Document {
        // Bail if we have nothing.
        if self.data.is_empty() { return Document::new(); }
        // Compose then zip iterators.
        let range = self.data.range();
        if range.1 - range.0 == 0 { return Document::new(); }
        let events = self.data.iter();
        let depths = self.data.depth();
        let scales = self.data.transform_iter(0f64, 1f64);
        // Construct SVG document, we'll be pushing drawing commands into it.
        let mut document = Document::new()
            .set("viewbox", (0,0,self.x,self.y))
            .set("width",  format!("{}px", self.x))
            .set("height", format!("{}px", self.y))
            .set("background-color", "#ff3400");
        for ((event,depth),(offset,scale)) in events.zip(depths).zip(scales) {
            // Transform the data points into screen space coords.
            let locs = event.location(range);
            let x_start = locs.0 as f64 * self.x;
            let x_end = locs.1.map(|some|some as f64 * self.x);
            let y = offset * scale * self.y * depth as f64;
            let height = 0.2 * self.y; // TODO: Add height:f64 to Node.
            // Start making the path.
            let data = match x_end {
                Some(some_end) => { // If span of time...
                    Data::new()
                        .move_to((x_start,  y))
                        .line_to((some_end, y))
                        .line_to((some_end, y+height))
                        .line_to((x_start,  y+height))
                        .close()
                },
                None => {   // If single point in time...
                    Data::new()
                        .move_to((x_start,  y))
                        .line_to((x_start,  y+height))
                        .close()
                },
            };
            let path = SvgPath::new()
                .set("fill", "#C3B2A4")
                .set("stroke", "#2e3d50")
                .set("stroke-width",2)
                .set("d", data);
            document.append(path);
        }
        document
        // println!("Looking at {}!", file_path);
        /*
        match svg::save(file_path, &document) {
            Ok(_) => println!("  > Wrote {} successfully.", file_path),
            Err(e) => println!("  > Err! failed to write {}. {:?}", file_path, e),
        }
        */
    }

    pub fn add_event(&mut self, query: &str) -> Result<(), Bogus> {
        let path = parse_to_int_path(query)
            .map_err(|e|Bogus::PathParse(e))?;
        let mut selected = self.data.find_mut(&path[..])
            .map_err(|e|Bogus::PathFind(e))?;
        let name = get_user("Name")
            .map_err(|e|Bogus::IoError(e))?;
        let date = get_user("Date")
            .map_err(|e|Bogus::IoError(e))?
            .parse::<Dates>()
            .map_err(|e|Bogus::DtParse(e))?;
        let mut event = Event::new(&name, date);
        // Get desc's for as long as the user is willing to give them.
        while let Ok(Some(input)) = ask_user("Description [Y/n]") {
            event.with_desc(&input);
        }
        selected.push_event(event);
        Ok(())
    }

    pub fn catenate(list: Vec<SagaDoc>) -> SagaDoc {
        let res: (f64, f64) = list.iter()
            .map(|saga|(saga.x, saga.y))
            .fold((f64::MAX, f64::MIN), |acc, this|{
                ( acc.0.min(this.0), acc.1.max(this.1),)
            });
        todo!();
    }

}

/// Takes user's input after printing a prompt.
pub fn get_user(prompt: &str) -> std::io::Result<String> {
    print!("{} > ", prompt);
    std::io::stdout().flush()?;     // Ensure we print before we read stdin.
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    Ok(input)
}

/// Asks yes/no question to user:
///   - Answers beginning with 'y' or 'Y' allows the user
///     to give a follow up answer, returning Ok(Some(_)).
///   - Answers beginning with anything else returns Ok(None).
///   - Returns Err(_) if an Io error occured.
pub fn ask_user(prompt: &str) -> std::io::Result<Option<String>> {
    print!("{} > ", prompt);
    std::io::stdout().flush()?;     // Ensure we print before we read stdin.
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    if input.starts_with(&['y', 'Y']) == false { return Ok(None); } // Easy return.
    get_user("").map(|ok|Some(ok))  // Functional Composition! Kinda...     // TODO: Test that this works as intended.
    /*
    print!("    > ");
    std::io::stdout().flush()?;     // Ensure we print before we read stdin.
    input.clear();                  // Reuse input.
    std::io::stdin().read_line(&mut input)?;
    Ok(Some(input))
    */
}

impl From<Bogus> for super::MainError {
    fn from(error: Bogus) -> Self {
        use super::MainError;
        match error {
            Bogus::PathParse(e) => MainError::BadPathParse(e),
            Bogus::PathFind(e)  => MainError::NodeNotFound(e),
            Bogus::DtParse(e)   => MainError::BadDateTimeParse(e),
            Bogus::IoError(e)   => MainError::FileIO(e),
        }
    }
}

use std::num::ParseIntError;
pub fn parse_to_int_path(query: &str) -> Result<Vec<usize>, ParseIntError> {
    if query.trim().len() == 0 { return Ok(vec![]); }
    query
        .split(":")
        .map(|s|s.trim())
        .map(|s|s.parse::<usize>())
        .try_collect::<Vec<usize>>()
}

#[cfg(test)]
mod tests {
    use super::super::saga::parse_to_int_path;

    #[test]
    fn test_node_querying() {
        let ok_queries = [
            "",
            "1",
            "1:2",
            "1 : 2",
            "1: 5:5 :3",
        ];
        for query in ok_queries.iter() {
            println!("Testing {}", query);
            assert!(parse_to_int_path(query).is_ok());
        }
    }
}
