
use std::str::FromStr;

use chrono::{NaiveDateTime};
use serde::{Serialize, Deserialize};

use super::saga::Color;

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
#[derive(Clone, Debug)]
pub struct Dates {
    start: Dt,
    end: Option<Dt>,
}

#[derive(Debug)]
pub struct DateParseError {
}

pub struct Line {
    start: f64,
    end: f64,
    interval: Option<f64>,
}

pub type NodePath<'a> = &'a [usize];

/// Created when following a Node down a path fails.
#[derive(Debug)]
pub struct PathFail {
    path: Vec<usize>,
    at: usize,
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

    /// Searches for a node at the given address. If not found, returns the part of the path that
    /// was successfully traversed.
    pub fn find_mut<'a,'b>(&'a mut self, addr: &'b [usize]) -> Result<&'a mut Node, PathFail/* &'b [usize] */> {
        println!("  > find_mut({:?})", addr);
        let mut steps = 0_usize;        // Index into addr.
        let mut ptr: &mut Node = self;  // Current node in the graph traversal.
        while steps < addr.len() {
            let index = addr[steps];
            ptr = match ptr.value {
                Value::List(ref mut list) => {  // Attempt to follow the path given.
                    if index > list.len() {
                        return Err(PathFail{path:Vec::from(addr), at:steps});
                    }
                    &mut list[index-1]  // User expects index origin of 1.
                },
                Value::Event(_) if steps == addr.len() => { return Ok(ptr); },
                Value::Event(_) => { return Err(PathFail{path:Vec::from(addr), at:steps}); },
            };
            steps += 1;
        }
        Ok(ptr)
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
            Value::Event(_) => Box::new(std::iter::once(value)),
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

    pub fn lines(&self, range: (i64, i64)) -> Vec<Line> {
        let iter: Vec<Line> = self.iter_nodes().map(|node|{
            let value: Option<Line> = match (node.location(range), node.line) {
                (Some((start,end)), Some(opt_n)) => Some(Line{start:start,end:end,interval:opt_n}),
                _ => None,
            };
            value
        }).filter_map(|value|value).collect();
        iter
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
                    start as f64 - a / width,
                    start as f64 - b / width,
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

    /// Builder method to set vertical offset.
    fn with_offset(mut self, y: f64) -> Node {
        self.offset = y;
        self
    }

    /// Builder method to set scaling of the certical offset.
    fn with_y_scale(mut self, y: f64) -> Node {
        self.y_scale = y;
        self
    }

    /// Builder method to set horizontal line with optional tick marks.
    fn with_line(mut self, line: Option<f64>) -> Node {
        self.line = Some(line);
        self
    }

    fn without_line(mut self) -> Node {
        self.line = None;
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

impl Line {
    /// Getter for the (start,end) fields of this struct.
    pub fn bounds(&self) -> (f64, f64) { (self.start, self.end) }

    /// Getter for the interval field of this struct.
    pub fn interval(&self) -> Option<f64> { self.interval }
}

#[cfg(test)]
mod tests {
    use crate::events::{Dt, Event, Node};

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
        let lines  = test_node.lines(range);
        assert_eq!(event_iter.len(), 7);
        assert_eq!(node_iter.len(), 10);
        assert_eq!(lines.len(), 2);
        let ok_queries: Vec<Vec<usize>> = vec![
            vec![],
            vec![1],
            vec![2],
            vec![3],
            vec![3,1],
            vec![3,1,1],
            vec![3,1,2],
            vec![3,2],
            vec![3,3],
            vec![3,4],
        ];
        for query in ok_queries.iter() {
            println!("Testing {:?}", query);
            assert!(test_node.find_mut(&query[..]).is_ok());
        }
        let err_queries: Vec<Vec<usize>> = vec![
            vec![1,1],
            vec![2,1],
            vec![3,1,1,1],
            vec![3,1,3],
            vec![3,2,1],
            vec![4],
        ];
        for query in err_queries.iter() {
            println!("Testing {:?}", query);
            assert!(test_node.find_mut(&query[..]).is_err());
        }
    }
}

