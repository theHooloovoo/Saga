
use std::str::FromStr;

use chrono::{NaiveDateTime};
use serde::{Serialize, Deserialize};

use super::MainError;
use super::saga::{Color, SagaDocError};
use super::edit::{EvalError, EvalResult};

pub const FORMAT: &'static str = "%d/%m/%Y %H:%M";
pub type Dt = NaiveDateTime;
pub type DtParseError = chrono::format::ParseError;

/// Main packaging struct. Essentially used to store nested/listed Events
/// from something like a JSON or TOML file.
#[derive(Serialize, Deserialize)]
pub struct Node {
    #[serde(flatten)]
    value: Value,
    name: Option<String>,
    style_override: Option<String>,
    color_override: Option<Color>,
    offset: f64,
    y_scale: f64,
    line: Option<Option<f64>>,  // (None|Draw Line|Draw Line with tick marks).
}

/// Internal enum used to store either more Nodes or leaf-like Events.
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "next")]
pub enum Value {
    Event(Event),
    List(Vec<Node>),
}

/// Main Struct for this program.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    name: String,
    descriptions: Vec<String>,
    #[serde(with = "serde_with::rust::display_fromstr")]
    datetime: Dates,
}

/// Used to represent either one point in time, or a timespan.
#[derive(Clone, Debug, PartialEq)]
pub struct Dates {
    start: Dt,
    end: Option<Dt>,
}

pub struct Line {
    pub start: f64,
    pub end: f64,
    pub interval: Option<f64>,
    pub y: f64,
}

/// Created when following a Node down a path fails.
#[derive(Debug)]
#[allow(dead_code)]
pub struct PathFail {
    path: Vec<usize>,
    at: usize,
}

pub enum Query<'a> {
    Node(&'a mut Node),
    Event(&'a mut Event),
}

impl Node {
    /// Wraps a list of Nodes into one new Node. 
    pub fn from_vec(list: Vec<Node>) -> Node {
        Node {
            name: None,
            value: Value::List(list),
            style_override: None,
            color_override: None,
            offset: 0f64,
            y_scale: 1f64,
            line: None,
        }
    }

    /// Searches the node tree for the given address. If not found, returns the remainder of the
    /// path that wasn't searched.
    // TODO: Rework PathFail into something more useful.
    pub fn query<'a>(&'a mut self, path: &[usize]) -> Result<Query<'a>, PathFail> {
        if path.len() == 0 { return Ok(Query::Node(self)); }
        match &mut self.value {
            Value::List(ref mut list) => {
                match path[0] <= list.len() {
                    // Subtract 1 because the user expects an index-origin on 1.
                    true => list[path[0]-1].query(&path[1..]),    // Recurse...
                    false => Err(PathFail{path:path.to_vec(),at:path.len()}),
                }
            },
            Value::Event(ref mut event) => {
                match path.len() == 1 && path[0] == 1 {
                    true => Ok(Query::Event(event)),
                    false => Err(PathFail{path:path.to_vec(),at:path.len()}),
                }
            },
        }
    }

    /// Produces an Iterator over all of the Events contained in Self.
    pub fn iter_nodes<'a>(&'a self) -> Box<dyn Iterator<Item=&'a Node> + 'a> {
        let this = Box::new(std::iter::once(self));
        match &self.value {
            Value::Event(_) => Box::new(std::iter::once(self)),
            Value::List(vec) => {
                let list = vec.iter()
                    .map(|node|node.iter_nodes())
                    .flatten();
                Box::new(this.chain(list))
            },
        }
    }

    /// Produces an Iterator over all of the Events contained in Self.
    pub fn iter<'a>(&'a self) -> Box<dyn Iterator<Item=&'a Event> + 'a> {
        match &self.value {
            Value::Event(event) => Box::new(std::iter::once(event)),
            Value::List(vec) => Box::new(
                vec .iter()
                    .map(|node|node.iter())
                    .flatten()
            ),
        }
    }

    /// Produces an Iterator of depth values intended to be zipped with self.iter().
    pub fn depth(&self) -> Box<dyn Iterator<Item = u32> + '_> {
        self.depth_iter(0)
    }

    fn depth_iter(&self, depth: u32) -> Box<dyn Iterator<Item = u32> + '_> {
        match &self.value {
            // Events are stored in their own Node, so compensate by -1.
            Value::Event(_) => Box::new(std::iter::once(depth-1)),
            Value::List(vec) => Box::new(
                vec .iter()
                    .map(move |node|node.depth_iter(depth+1))
                    .flatten()
            ),
        }
    }

    /// Returns an Iterator over y-axis (Offset, Scaling) pairs.
    pub fn transform_iter(&self, offset: f64, scale: f64) -> Box<dyn Iterator<Item = (f64, f64)> + '_> {
        let value = (self.offset + offset, self.y_scale * scale);
        match &self.value {
            Value::Event(_) => Box::new(std::iter::once((offset, scale))),
            Value::List(vec) => Box::new(
                vec .iter()
                    .map(move |node|node.transform_iter(value.0 * scale, value.1))
                    .flatten()
            ),
        }
    }

    /// Returns true if self doesn't contain any Events.
    pub fn is_empty(&self) -> bool {
        self.iter().collect::<Vec<&Event>>().is_empty()
    }

    /// Returns the timestamp set that contains all of the dates contained by self.
    pub fn range(&self) -> (i64, i64) {
        use chrono::{MAX_DATETIME, MIN_DATETIME};
        let max_min = (
            MAX_DATETIME.timestamp(),
            MIN_DATETIME.timestamp(),
        );
        self.iter()
            .map(|event|&event.datetime)
            .fold(max_min,|range,dt|dt.expand_range(range))
    }

    pub fn lines(&self, grand_range: &(i64, i64)) -> Vec<Line> {
        let iter = self.iter_nodes();
        let iter_trans = self.transform_iter(0.0, 1.0);
        let iter_depth = self.depth();
        iter.zip(iter_trans).zip(iter_depth).map(|((node,(offset,scale)),depth)|{
            let y = offset * scale * depth as f64;
            match (node.line, node.location(*grand_range)) {
                (Some(int), Some((a,b))) => {
                    Some(Line { start:a, end:b, interval:int, y:y})
                },
                _ => None,
            }
        })  .filter(|opt|opt.is_some()) // Drop all Nones...
            .map(|opt|opt.unwrap())     // ...and retain only Ok's.
            .collect::<Vec<_>>()
    }

    pub fn print(&self, depth: usize, verbose: bool) -> String {
        let pad = padding("  ", depth);
        let start = match self.name {
            Some(ref name) => format!("{}Node: {}", pad, name),
            None => format!("{}Node: (No name)", pad),
        };
        let mut lines = vec![
            start,
        ];
        if verbose {
            lines.push(format!("{}  Offset:  {}", pad, self.offset));
            lines.push(format!("{}  Scaling: {}", pad, self.y_scale));
            if let Some(line) = self.line {
                lines.push(format!("{}  Line: {:?}", pad, line));
            }
        }
        let children = match self.value {
            Value::List(ref node_list) => node_list
                .iter()
                .map(|n|n.print(depth+1, verbose))
                .collect::<Vec<String>>()
                .join("\n"),
            Value::Event(ref event) => event.print(&pad, verbose),
        };
        lines.push(children);
        lines.join("\n")
    }

    /// Returns the location of this Node's time range.
    fn location(&self, range: (i64, i64)) -> Option<(f64, f64)> {
        let (start,end) = range;
        let width = (end - start) as f64;
        match width {
            n if n == 0.0 => { None },
            _ => {
                let (a,b) = (range.0 as f64, range.1 as f64);
                Some((
                    (a - start as f64) / width,
                    (b - start as f64) / width,
                ))
            },
        }
    }

    /// Converts self.value in Value::List if not already and pushes value.
    pub fn push_event(&mut self, event: Event) {
        match &mut self.value {
            Value::Event(other) => {
                let list = vec![
                    other.clone().into_node(), 
                    event.into_node()
                ];
                self.value = Value::List(list);
            },
            Value::List(list) => { list.push(event.into_node()); },
        }
    }

    pub fn get_value_mut(&mut self) -> &mut Value {
        &mut self.value
    }
    
    /// Sets the name of self.
    pub fn set_name(&mut self, name: Option<&str>) {
        self.name = name.map(|s|s.to_string());
    }

    /// Sets the vertical offset of the element in the render.
    pub fn set_offset(&mut self, y: &f64) {
        self.offset = *y;
    }

    /// Sets the scaling factor for the vertical offset.
    pub fn set_scale(&mut self, y: &f64) {
        self.y_scale = *y;
    }

    /// Sets the Line.
    pub fn set_line(&mut self, line: Option<Option<f64>>) {
        self.line = line.clone();
    }

    /// Builder Method. TODO: Probably don't need, except for building explicit struct in test.
    pub fn with_line(mut self, line: Option<f64>) -> Self {
        self.line = Some(line);
        self
    }
}

impl Event {
    /// Creates a new Event with a name and dates.
    pub fn new(name: &str, dt: Dates) -> Event {
        Event {
            name: name.to_string(),
            descriptions: vec![],
            datetime: dt,
        }
    }

    pub fn print(&self, padding: &str, verbose: bool) -> String {
        let payload = format!(
            "{}Event: {}, {:?}",
            padding,
            self.name,
            self.datetime,
        );
        payload
    }

    /// Wrapper to convert this event into a node.
    pub fn into_node(self) -> Node {
        Node {
            name: None,
            value: Value::Event(self),
            style_override: None,
            color_override: None,
            offset: 0f64,
            y_scale: 1f64,
            line: None,
        }
    }

    /// Getter for name.
    pub fn name(&self) -> &str { &self.name }

    /// Set name.
    pub fn set_name(&mut self, new: &str) { self.name = new.to_string(); }

    /// Set dates.
    pub fn set_dates(&mut self, new: &Dates) { self.datetime = new.clone(); }

    /// Getter for dates.
    pub fn date_string(&self) -> String {
        format!("{}", self.datetime)
    }

    /// Adds the given string to this events list of descriptions.
    pub fn with_desc(&mut self, desc: &str) {
        self.descriptions.push(desc.to_string());
    }

    /// Returns the location of an event within the context of a given range of timestamps.
    /// If self is within the range of timestamps then the output will be in [0,1].
    pub fn location(&self, range: (i64, i64)) -> (f64, Option<f64>) {
        let (start, end) = range;
        let span = (end - start) as f64;
        let f = |date: &Dt| { (date.timestamp() - start) as f64 / span };
        (
            f(&self.datetime.start),
            self.datetime.end.as_ref().map(f),
        )
    }

    /// Adds the description to self.
    pub fn add_description(&mut self, new: &str) {
        self.descriptions.push(new.to_string());
    }

    /// Replaces the description at the given index.
    pub fn change_description(&mut self, index: usize, new: &str) -> EvalResult {
        match index < self.descriptions.len() {
            true => {
                self.descriptions[index] = new.to_string();
                Ok(())
            },
            false => Err(EvalError::IndexError{index:index, len:self.descriptions.len()}),
        }
    }

    /// Deletes the description at the given index.
    pub fn delete_description(&mut self, index: usize) -> EvalResult {
        match index < self.descriptions.len() {
            true => {
                self.descriptions.remove(index);
                Ok(())
            },
            false => Err(EvalError::IndexError{index:index, len:self.descriptions.len()}),
        }
    }
}

impl Dates {
    /// Converts a set of timestamps into a Dates struct.
    pub fn from(range: (i64, i64)) -> Dates {
        let (start, end) = range;
        Dates {
            start: Dt::from_timestamp_millis(start).unwrap(),
            end: Dt::from_timestamp_millis(end),
        }
    }

    /// Produces a set of timestamps from Self.
    fn stamps(&self) -> (i64, Option<i64>) {
        (
            self.start.timestamp(),
            self.end.as_ref().map(|some|some.timestamp()),
        )
    }

    /// Compares self to a set of timestamps and returns the timestamps that contain both.
    fn expand_range(&self, range: (i64, i64)) -> (i64, i64) {
        let (min, max) = range;
        let (start, end) = self.stamps();
        (
            end.unwrap_or(i64::MAX).min(min.min(start)),
            end.unwrap_or(i64::MIN).max(max.max(start)),
        )
    }
}

/// Used by serde to write struct to file.
impl std::fmt::Display for Dates {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let left = self.start.format(FORMAT).to_string();
        let right = self.end
            .as_ref()
            .map(|some|format!(" - {}", some.format(FORMAT).to_string()))
            .unwrap_or( "".to_string() );
        let result = format!("{}{}", left, right);
        write!(f, "{}", result)
    }
}

/// Used by serde to read struct from file.
impl FromStr for Dates {
    type Err = DtParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (left,right) = match s.split_once('-') {
            Some((left,right)) => {
                let start = Dt::parse_from_str(left.trim(), FORMAT)?;
                let end   = Dt::parse_from_str(right.trim(), FORMAT)?;
                (start,Some(end))
            },
            None => { (Dt::parse_from_str(s, FORMAT)?,None) },
        };
        Ok(Dates { start: left, end: right })
    }
}

impl From<DtParseError> for SagaDocError {
    fn from(dt_err: DtParseError) -> Self {
        SagaDocError::DtParse(dt_err)
    }
}

impl From<PathFail> for SagaDocError {
    fn from(path_fail: PathFail) -> Self {
        SagaDocError::PathFind(path_fail)
    }
}

impl From<PathFail> for MainError {
    fn from(path_fail: PathFail) -> Self {
        MainError::NodeNotFound(path_fail)
    }
}

fn padding(pad: &str, n: usize) -> String {
    std::iter::once(pad).cycle().take(n).collect()
}

#[cfg(test)]
mod tests {
    use crate::events::{Dates,Dt, Event, Node, Query};

    #[test]
    fn test_date_parsing() {
        let ok_tests = [
            "01/01/1990 0:0",
            "1/1/1990 0:0",
        ];
        for query in ok_tests.iter() {
            assert!(query.parse::<Dates>().is_ok());
        }
    }

    #[test]
    fn test_node_querying() {
        let mut test_node = Node::from_vec(vec![
            Event::new("First Event",  "08/12/1997 0:0 - 26/12/1997 0:0".parse().unwrap()).into_node(),
            Event::new("Second Event", "01/12/1997 0:0 - 09/12/1997 0:0".parse().unwrap()).into_node(),
            Node::from_vec(vec![
                Node::from_vec(vec![
                    Event::new("Third Event",  "03/12/1997 0:0 - 04/12/1997 0:0".parse().unwrap()).into_node(),
                    Event::new("Fourth Event", "04/12/1997 0:0 - 06/12/1997 0:0".parse().unwrap()).into_node(),
                ]),
                Event::new("Fifth Event",  "03/12/1997 0:0 - 04/12/1997 0:0".parse().unwrap()).into_node(),
                Event::new("Sixth Event", "04/12/1997 0:0 - 06/12/1997 0:0".parse().unwrap()).into_node(),
                Event::new("Seventh Event",  "07/12/1997 0:0 - 09/12/1997 0:0".parse().unwrap()).into_node(),
            ]).with_line(Some(5.0)),
        ]).with_line(None);
        let range = test_node.range();
        let event_iter = test_node.iter().collect::<Vec<&Event>>();
        let node_iter  = test_node.iter_nodes().collect::<Vec<&Node>>();
        let lines  = test_node.lines(&range);
        assert_eq!(event_iter.len(), 7);
        assert_eq!(node_iter.len(), 10);
        assert_eq!(lines.len(), 2);
        let ok_queries: Vec<(Vec<usize>, bool)> = vec![
            (vec![],  true),
            (vec![1], true),
            (vec![1,1], false),
            (vec![2], true),
            (vec![3], true),
            (vec![3,1], true),
            (vec![3,1,1], true),
            (vec![3,1,2], true),
            (vec![3,2], true),
            (vec![3,3], true),
            (vec![3,4], true),
        ];
        for (query, is_node) in ok_queries.iter() {
            println!("Testing {:?}", query);
            let query = test_node.query(&query[..]).unwrap();
            let tag = match query {
                Query::Node(_) => true,
                Query::Event(_) => false,
            };
            assert_eq!(*is_node, tag);
        }
        let err_queries: Vec<Vec<usize>> = vec![
            vec![1,1,1],
            vec![2,1,1],
            vec![3,1,1,1,1],
            vec![3,1,3,1],
            vec![3,2,1,1],
            vec![4,1],
        ];
        for query in err_queries.iter() {
            println!("Testing {:?}", query);
            assert!(test_node.query(&query[..]).is_err());
        }
    }
}

