use std::collections::HashMap;

use crate::intspan::IntSpan;

/// The type of data in a data file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DataType {
    Highlight,
    Link,
    Plot,
    Text,
    Tile,
    Connector,
}

/// A single data point (one line in a data file).
#[derive(Debug, Clone)]
pub struct Datum {
    /// Core data fields.
    pub chr: String,
    pub start: i64,
    pub end: i64,
    pub set: IntSpan,
    /// For links: the link ID.
    pub id: Option<String>,
    /// For plots: the numeric value(s).
    pub value: Option<f64>,
    /// For text: the label.
    pub label: Option<String>,
    /// Key-value options from the data line.
    pub param: HashMap<String, String>,
}

/// A link: a pair (or group) of data points with the same ID.
#[derive(Debug, Clone)]
pub struct Link {
    pub id: String,
    pub points: Vec<Datum>,
    pub param: HashMap<String, String>,
}

/// A named data set (e.g., one <link segdup> block or <highlight> block).
#[derive(Debug, Clone)]
pub struct DataSet {
    pub name: String,
    pub data_type: DataType,
    pub data: Vec<Datum>,
    /// For links: grouped by ID.
    pub links: Vec<Link>,
    /// Parameters from the config block.
    pub param: HashMap<String, String>,
}
