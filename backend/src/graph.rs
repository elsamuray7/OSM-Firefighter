use std::{cmp::Ordering,
          fmt::Formatter,
          fs::File,
          io::{prelude::*, BufReader},
          num::{ParseIntError, ParseFloatError}};

use serde::Serialize;

use crate::binary_minheap::BinaryMinHeap;

/// Type alias for the result of a run of the Dijkstra algorithm
type DijkstraResult = Vec<usize>;

/// Struct to hold the grid bounds of a graph or part of a graph
#[derive(Debug, Serialize)]
pub struct GridBounds {
    pub min_lat: f64,
    pub max_lat: f64,
    pub min_lon: f64,
    pub max_lon: f64,
}

impl GridBounds {
    /// Returns true if this grid bounds are located within `other`
    pub fn is_located_in(&self, other: &GridBounds) -> bool {
        self.min_lat >= other.min_lat && self.max_lat <= other.max_lat
            && self.min_lon >= other.min_lon && self.max_lon <= other.max_lon
    }
}

/// Compass directions related to grid bounds
pub enum CompassDirection {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
    Zero,
}

/// A graph node with id, latitude and longitude
#[derive(Debug, Serialize, Default)]
pub struct Node {
    pub id: usize,
    pub lat: f64,
    pub lon: f64,
}

impl Node {
    /// Returns true if this node is located within the given grid bounds
    pub fn is_located_in(&self, gb: &GridBounds) -> bool {
        self.lat >= gb.min_lat && self.lat <= gb.max_lat
            && self.lon >= gb.min_lon && self.lon  <= gb.max_lon
    }

    /// Get the compass direction of this node relative to the given grid bounds
    pub fn get_relative_compass_direction(&self, gb: &GridBounds) -> CompassDirection {
        if self.lon >= gb.min_lon && self.lon <= gb.max_lon && self.lat > gb.max_lat {
            CompassDirection::North
        } else if self.lon > gb.max_lon && self.lat > gb.max_lat {
            CompassDirection::NorthEast
        } else if self.lon > gb.max_lon && self.lat >= gb.min_lat && self.lat <= gb.max_lat {
            CompassDirection::East
        } else if self.lon > gb.max_lon && self.lat < gb.min_lat {
            CompassDirection::SouthEast
        } else if self.lon >= gb.min_lon && self.lon <= gb.max_lon && self.lat < gb.min_lat {
            CompassDirection::South
        } else if self.lon < gb.min_lon && self.lat < gb.min_lat {
            CompassDirection::SouthWest
        } else if self.lon < gb.min_lon && self.lat >= gb.min_lat && self.lat <= gb.max_lat {
            CompassDirection::West
        } else if self.lon < gb.min_lon && self.lat > gb.max_lat {
            CompassDirection::NorthWest
        } else {
            CompassDirection::Zero
        }
    }
}

/// A directed graph edge with source and target
#[derive(Debug, Serialize, Default)]
pub struct Edge {
    pub src: usize,
    pub tgt: usize,
    pub dist: usize,
}

/// A directed graph with nodes, edges and node offsets
#[derive(Debug, Serialize, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub offsets: Vec<usize>,
    pub num_nodes: usize,
    pub num_edges: usize,
}

/// Unstable float comparison.
/// # Returns
/// * `a < b`: `Ordering::Less`
/// * `a >= b`: `Ordering::Greater`
fn unstable_cmp_f64(a: f64, b: f64) -> Ordering {
    if a < b { Ordering::Less } else { Ordering::Greater }
}

impl Graph {
    /// Create a new directed graph without any nodes or edges
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            offsets: Vec::new(),
            num_nodes: 0,
            num_edges: 0,
        }
    }

    /// Parse node and edge data from a file into a directed graph
    fn parse_graph(&mut self, graph_file_path: &str) -> Result<(), ParseError> {
        let graph_file = File::open(graph_file_path)?;
        let graph_reader = BufReader::new(graph_file);

        let mut lines = graph_reader.lines();
        let mut line_no = 0;

        loop {
            let line = lines.next()
                .expect(&format!("Unexpected EOF while parsing header in line {}", line_no))?;
            line_no += 1;

            if !line.starts_with("#") {
                break;
            }
        }

        self.num_nodes = lines.next()
            .expect("Unexpected EOF while parsing number of nodes")?
            .parse()?;
        self.num_edges = lines.next()
            .expect("Unexpected EOF while parsing number of edges")?
            .parse()?;
        line_no += 3;

        self.nodes.reserve_exact(self.num_nodes);
        for i in 0..self.num_nodes {
            let line = lines.next()
                .expect(&format!("Unexpected EOF while parsing nodes in line {}", line_no))?;
            let mut split = line.split(" ");
            line_no += 1;
            split.next(); // id
            split.next(); // second id

            let node = Node {
                id: i,
                lat: split.next()
                    .expect(&format!("Unexpected EOL while parsing node latitude in line {}",
                                     line_no))
                    .parse()?,
                lon: split.next()
                    .expect(&format!("Unexpected EOL while parsing node longitude in line {}",
                                     line_no))
                    .parse()?,
            };
            self.nodes.push(node);
        }

        let mut last_src: i64 = -1;
        let mut offset: usize = 0;
        self.edges.reserve_exact(self.num_edges);
        self.offsets.resize(self.num_nodes + 1, 0);
        for _ in 0..self.num_edges {
            let line = lines.next()
                .expect(&format!("Unexpected EOF while parsing edges in line {}", line_no))?;
            let mut split = line.split(" ");
            line_no += 1;

            let edge = Edge {
                src: split.next()
                    .expect(&format!("Unexpected EOL while parsing edge source in line {}",
                                     line_no))
                    .parse()?,
                tgt: split.next()
                    .expect(&format!("Unexpected EOL while parsing edge target in line {}",
                                     line_no))
                    .parse()?,
                dist: split.next()
                    .expect(&format!("Unexpected EOL while parsing edge weight in line {}",
                                     line_no))
                    .parse()?,
            };

            if edge.src as i64 > last_src {
                for j in (last_src + 1) as usize..=edge.src {
                    self.offsets[j] = offset;
                }
                last_src = edge.src as i64;
            }
            offset += 1;

            self.edges.push(edge);
        }
        self.offsets[self.num_nodes] = self.num_edges;

        Ok(())
    }

    /// Create a directed graph from a file that contains node and edge data
    pub fn from_file(file_path: &str) -> Self {
        let mut graph = Graph::new();
        match graph.parse_graph(file_path) {
            Ok(_) => (),
            Err(err) => panic!("Failed to create graph from files at {}: {}", file_path,
                               err.to_string())
        }
        graph
    }

    /// Get the number of outgoing edges of the node with id `node_id`
    pub fn get_degree(&self, node_id: usize) -> usize {
        self.offsets[node_id + 1] - self.offsets[node_id]
    }

    /// Run an one-to-all Dijkstra from the source node with id `src_id`
    pub fn run_dijkstra(&self, src_id: usize) -> DijkstraResult {
        let mut distances = vec![usize::MAX; self.num_nodes];
        distances[src_id] = 0;

        let mut pq = BinaryMinHeap::with_capacity(self.num_nodes);
        pq.push(src_id, &distances);

        while !pq.is_empty() {
            let node = pq.pop(&distances);

            for i in self.offsets[node]..self.offsets[node +1] {
                let edge = &self.edges[i];
                let dist = distances[node] + edge.dist;

                if dist < distances[edge.tgt] {
                    distances[edge.tgt] = dist;

                    if pq.contains(edge.tgt) {
                        pq.decrease_key(edge.tgt, &distances);
                    } else {
                        pq.push(edge.tgt, &distances);
                    }
                }
            }
        }

        distances
    }

    /// Returns this graphs grid bounds, i.e. the minimal/maximal latitude/longitude
    /// of this graph
    pub fn get_grid_bounds(&self) -> GridBounds {
        let latitudes: Vec<_> = self.nodes.iter()
            .map(|n| n.lat)
            .collect();
        let longitudes: Vec<_> = self.nodes.iter()
            .map(|n| n.lon)
            .collect();

        GridBounds {
            min_lat: *latitudes.iter()
                .min_by(|&lat1, &lat2| unstable_cmp_f64(*lat1, *lat2))
                .unwrap_or(&f64::NAN),
            max_lat: *latitudes.iter()
                .max_by(|&lat1, &lat2| unstable_cmp_f64(*lat1, *lat2))
                .unwrap_or(&f64::NAN),
            min_lon: *longitudes.iter()
                .min_by(|&lon1, &lon2| unstable_cmp_f64(*lon1, *lon2))
                .unwrap_or(&f64::NAN),
            max_lon: *longitudes.iter()
                .max_by(|&lon1, &lon2| unstable_cmp_f64(*lon1, *lon2))
                .unwrap_or(&f64::NAN),
        }
    }
}

#[derive(Debug)]
enum ParseError {
    IO(std::io::Error),
    ParseInt(ParseIntError),
    ParseFloat(ParseFloatError),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IO(err) => write!(f, "{}", err.to_string()),
            Self::ParseInt(err) => write!(f, "{}", err.to_string()),
            Self::ParseFloat(err) => write!(f, "{}", err.to_string()),
        }
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            Self::IO(ref err) => Some(err),
            Self::ParseInt(ref err) => Some(err),
            Self::ParseFloat(ref err) => Some(err),
        }
    }
}

impl From<std::io::Error> for ParseError {
    fn from(err: std::io::Error) -> Self {
        Self::IO(err)
    }
}

impl From<ParseIntError> for ParseError {
    fn from(err: ParseIntError) -> Self {
        Self::ParseInt(err)
    }
}

impl From<ParseFloatError> for ParseError {
    fn from(err: ParseFloatError) -> Self {
        Self::ParseFloat(err)
    }
}

#[cfg(test)]
mod test {
    use crate::graph::Graph;

    #[test]
    fn test_nodes_edges() {
        let graph = Graph::from_file("data/bbgrund_undirected.fmi");

        assert_eq!(graph.nodes.len(), 350);
        assert_eq!(graph.edges.len(), 706);
    }

    #[test]
    fn test_grid_bounds() {
        let graph = Graph::from_file("data/bbgrund_undirected.fmi");

        let gb = graph.get_grid_bounds();
        assert!(gb.min_lat >= 48.67);
        assert!(gb.max_lat < 48.68);
        assert!(gb.min_lon >= 8.99);
        assert!(gb.max_lon < 9.02);
    }

    #[test]
    fn test_node() {
        let graph = Graph::from_file("data/bbgrund_undirected.fmi");

        let edges_with_src_70: Vec<_> = graph.edges.iter()
            .filter(|&e| e.src == 70)
            .collect();
        assert_eq!(edges_with_src_70.len(), 3);
    }
}