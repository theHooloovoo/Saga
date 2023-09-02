
use std::{
    collections::HashMap,
    io::{
        Error as IoError,
        Write,
    },
    num::ParseIntError,
};

pub type DtParseError = chrono::format::ParseError;
use serde::{Serialize, Deserialize};
use svg::{
    Document, Node as SvgNode,
    node::element::{path::Data,Path as SvgPath}
};

use super::events::{Dates, Event, PathFail, Node, Query};

/// Temp error type.
pub enum SagaDocError {
    PathParse(ParseIntError),
    PathFind(PathFail),
    AddToEvent,
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

    pub fn blank() -> SagaDoc {
        SagaDoc {
            x: 0.0,
            y: 0.0,
            padding: 0.0,
            color_schemes: HashMap::new(),
            data:   Node::from_vec(vec![]),
        }
    }

    pub fn get_data_mut(&mut self) -> &mut Node { &mut self.data }

    pub fn draw(&self) -> Document {
        // Bail if we have nothing.
        if self.data.is_empty() { return Document::new(); }
        // Compose then zip iterators.
        let range = self.data.range();
        if range.1 - range.0 == 0 { return Document::new(); }
        let slide_y: f64 = 0.1 * self.y;
        let events = self.data.iter_events();
        let depths = self.data.depth();
        let scales = self.data.transform_iter(0.0, 1f64);
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
                        .move_to((x_start,  y + slide_y))
                        .line_to((some_end, y + slide_y))
                        .line_to((some_end, y + slide_y + height))
                        .line_to((x_start,  y + slide_y + height))
                        .close()
                },
                None => {   // If single point in time...
                    Data::new()
                        .move_to((x_start, y + slide_y))
                        .line_to((x_start, y + slide_y + height))
                        .close()
                },
            };
            let path = SvgPath::new()
                .set("fill", "#C3B2A4")
                .set("stroke", "#2e3d50")
                .set("stroke-width", 2)
                .set("d", data);
            document.append(path);
        }
        self.paint_lines(&mut document, &range, slide_y);
        document
    }

    fn paint_lines(&self, doc: &mut Document, range: &(i64, i64), slide: f64) {
        for line in self.data.lines(range).iter() {
            println!("> Line.y: {}", line.y);
            let data = Data::new()
                .move_to((line.start * self.x, line.y * self.y + slide))
                .line_to((line.end   * self.x, line.y * self.y + slide))
                .close();
            let path = SvgPath::new()
                .set("fill", "#C3B2A4")
                .set("stroke", "#000000")
                .set("stroke-width",5)
                .set("d", data);
            doc.append(path);
        }        
    }

    /// Interactively build an event and place it at the requested location.
    pub fn add_event(&mut self, query: &str) -> Result<(), SagaDocError> {
        let path = parse_to_int_path(query)?;
        match self.data.query(&path[..])? {
            Query::Node(node) => {
                let name = get_user("Name")?;
                let date_input: String = get_user("Date")?;
                println!("[{}]", date_input);
                let date = date_input.parse::<Dates>()?;
                let mut event = Event::new(&name, date);
                // Get desc's for as long as the user is willing to give them.
                while let Ok(Some(input)) = ask_user("Description [Y/n]") {
                    event.with_desc(&input);
                }
                node.push_event(event);
                Ok(())
            },
            Query::Event(_) => Err(SagaDocError::AddToEvent),
        }
    }

    /// Creates a new SagaDoc who's value is a list of the values of each
    /// SagaDoc in the given vector.
    pub fn catenate(list: Vec<SagaDoc>) -> SagaDoc {
        let mut doc = SagaDoc::blank();
        list.into_iter().for_each(|mut item|{
            // Build up new doc data.
            doc.x = doc.x.max(item.x);
            doc.y = doc.y.max(item.y);
            doc.padding = doc.padding.max(item.padding);
            item.color_schemes
                .drain()
                .for_each(|(k,v)|{doc.color_schemes.insert(k,v);});
        });
        doc
    }

    pub fn print(&self, verbose: bool) -> String {
        self.data.print(0_usize, verbose)
    }

}

/// Takes user's input after printing a prompt.
pub fn get_user(prompt: &str) -> Result<String, SagaDocError> {
    print!("{} > ", prompt);
    std::io::stdout().flush()     // Ensure we print before we read stdin.
        .map_err(|e|SagaDocError::IoError(e))?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)
        .map_err(|e|SagaDocError::IoError(e))?;
    Ok(input.trim_end().to_string())
}

/// Asks yes/no question to user:
///   - Answers beginning with 'y' or 'Y' allows the user
///     to give a follow up answer, returning Ok(Some(_)).
///   - Answers beginning with anything else returns Ok(None).
///   - Returns Err(_) if an Io error occured.
pub fn ask_user(prompt: &str) -> Result<Option<String>, SagaDocError> {
    print!("{} > ", prompt);
    std::io::stdout().flush()     // Ensure we print before we read stdin.
        .map_err(|e|SagaDocError::IoError(e))?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)
        .map_err(|e|SagaDocError::IoError(e))?;
    if input.starts_with(&['y', 'Y']) == false { return Ok(None); } // Easy return.
    get_user("").map(|ok|Some(ok))  // Functional Composition! Kinda...     // TODO: Test that this works as intended.
}

impl From<SagaDocError> for super::MainError {
    fn from(error: SagaDocError) -> Self {
        use super::MainError;
        match error {
            SagaDocError::PathParse(e) => MainError::BadPathParse(e),
            SagaDocError::PathFind(e)  => MainError::NodeNotFound(e),
            SagaDocError::DtParse(e)   => MainError::BadDateTimeParse(e),
            SagaDocError::IoError(e)   => MainError::FileIO(e),
            SagaDocError::AddToEvent   => MainError::AddToEvent,
        }
    }
}

pub fn parse_to_int_path(query: &str) -> Result<Vec<usize>, SagaDocError> {
    if query.trim().len() == 0 { return Ok(vec![]); }
    query
        .split(":")
        .map(|s|s.trim())
        .map(|s|s.parse::<usize>())
        .try_collect::<Vec<usize>>()
        .map_err(|e|SagaDocError::PathParse(e))
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

