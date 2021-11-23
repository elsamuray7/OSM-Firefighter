use std::{collections::BTreeMap,
          // fmt::Formatter,
          sync::{Arc, RwLock}};

use log;
use rand::prelude::*;
use serde::{Serialize, Deserialize};

use crate::firefighter::{strategy::{OSMFStrategy, Strategy},
                         TimeUnit,
                         view::{View, Coords}};
use crate::graph::{Graph, GridBounds};

/// Settings for a firefighter problem instance
#[derive(Debug, Deserialize)]
pub struct OSMFSettings {
    pub graph_name: String,
    pub strategy_name: String,
    num_roots: usize,
    pub num_ffs: usize,
    pub strategy_every: u64,
}

/// Node data related to the firefighter problem
#[derive(Debug, Serialize)]
pub struct NodeData {
    pub node_id: usize,
    time: TimeUnit,
}

/// Storage for node data
#[derive(Debug, Serialize)]
pub struct NodeDataStorage {
    burning: BTreeMap<usize, NodeData>,
    defended: BTreeMap<usize, NodeData>,
}

impl NodeDataStorage {
    /// Create a new node data storage
    fn new() -> Self {
        Self {
            burning: BTreeMap::new(),
            defended: BTreeMap::new(),
        }
    }

    /// Is node with id `node_id` a fire root?
    pub fn is_root(&self, node_id: &usize) -> bool {
        match self.burning.get(node_id) {
            Some(nd) => nd.time == 0,
            None => false
        }
    }

    /// Is node with id `node_id` burning?
    fn is_burning(&self, node_id: &usize) -> bool {
        self.burning.contains_key(node_id)
    }

    /// Is node with id `node_id` burning by time `time`?
    pub fn is_burning_by(&self, node_id: &usize, time: &TimeUnit) -> bool {
        match self.burning.get(node_id) {
            Some(nd) => nd.time <= *time,
            None => false
        }
    }

    /// Count all nodes burning by time `time`
    pub fn count_burning_by(&self, time: &TimeUnit) -> usize {
        self.burning.values()
            .filter(|nd| nd.time <= *time)
            .count()
    }

    /// Is node with id `node_id` defended?
    fn is_defended(&self, node_id: &usize) -> bool {
        self.defended.contains_key(node_id)
    }

    /// Is node with id `node_id` defended by time `time`?
    pub fn is_defended_by(&self, node_id: &usize, time: &TimeUnit) -> bool {
        match self.defended.get(node_id) {
            Some(nd) => nd.time <= *time,
            None => false
        }
    }

    /// Count all nodes defended by time `time`
    pub fn count_defended_by(&self, time: &TimeUnit) -> usize {
        self.defended.values()
            .filter(|nd| nd.time <= *time)
            .count()
    }

    /// Is node with id `node_id` undefended?
    pub fn is_undefended(&self, node_id: &usize) -> bool {
        !(self.is_burning(node_id) || self.is_defended(node_id))
    }

    /// Mark all nodes in `nodes` as burning at time `time`
    pub fn mark_burning(&mut self, nodes: &Vec<usize>, time: TimeUnit) {
        for node_id in nodes {
            self.burning.insert(*node_id, NodeData {
                node_id: *node_id,
                time,
            });
        }
    }

    /// Mark all nodes in `nodes` as defended at time `time`
    pub fn mark_defended(&mut self, nodes: &Vec<usize>, time: TimeUnit) {
        for node_id in nodes {
            self.defended.insert(*node_id, NodeData {
                node_id: *node_id,
                time,
            });
        }
    }

    /// Mark all nodes in `nodes` as defended at time `time`
    pub fn mark_defended2(&mut self, nodes: &[usize], time: TimeUnit) {
        for node_id in nodes {
            self.defended.insert(*node_id, NodeData {
                node_id: *node_id,
                time,
            });
        }
    }

    /// Get the node data of all burning vertices
    pub fn get_burning(&self) -> Vec<&NodeData> {
        self.burning.values().collect()
    }

    /// Get the id's of all burning vertices at time `time`
    pub fn get_burning_at(&self, time: &TimeUnit) -> Vec<usize> {
        self.burning.values()
            .filter(|&nd| nd.time == *time)
            .map(|nd| nd.node_id)
            .collect::<Vec<_>>()
    }

    /// Get the id's of all defended vertices at time `time`
    pub fn get_defended_at(&self, time: &TimeUnit) -> Vec<usize> {
        self.defended.values()
            .filter(|&nd| nd.time == *time)
            .map(|nd| nd.node_id)
            .collect::<Vec<_>>()
    }
}

/// Container for data about the simulation of a firefighter problem instance
#[derive(Serialize)]
pub struct OSMFSimulationResponse<'a> {
    nodes_burned: usize,
    nodes_defended: usize,
    nodes_total: usize,
    end_time: TimeUnit,
    view_bounds: &'a GridBounds,
    view_center: Coords,
}

/// Container for data about a specific step of a firefighter simulation
#[derive(Serialize)]
pub struct OSMFSimulationStepMetadata {
    nodes_burned_by: usize,
    nodes_defended_by: usize,
    nodes_burned_at: Vec<usize>,
    nodes_defended_at: Vec<usize>,
}

/// A firefighter problem instance
#[derive(Debug)]
pub struct OSMFProblem {
    graph: Arc<RwLock<Graph>>,
    settings: OSMFSettings,
    strategy: OSMFStrategy,
    node_data: NodeDataStorage,
    global_time: TimeUnit,
    is_active: bool,
    view: View,
}

impl OSMFProblem {
    /// Create a new firefighter problem instance
    pub fn new(graph: Arc<RwLock<Graph>>, settings: OSMFSettings, strategy: OSMFStrategy) -> Self {
        let num_nodes = graph.read().unwrap().num_nodes;
        if settings.num_roots > num_nodes {
            panic!("Number of fire roots must not be greater than {}", num_nodes);
        }

        let mut problem = Self {
            graph: graph.clone(),
            settings,
            strategy,
            node_data: NodeDataStorage::new(),
            global_time: 0,
            is_active: true,
            view: View::new(graph, 1920, 1080),
        };

        let roots = problem.gen_fire_roots();

        if let OSMFStrategy::MinDistanceGroup(ref mut mindistgroup_strategy) = problem.strategy {
            mindistgroup_strategy.compute_nodes_to_defend(&roots, &problem.settings);
        } else if let OSMFStrategy::Priority(ref mut priority_strategy) = problem.strategy {
            priority_strategy.compute_nodes_to_defend(&roots, &problem.settings);
        }

        problem
    }

    /// Generate `num_roots` fire roots
    fn gen_fire_roots(&mut self) -> Vec<usize> {
        let mut rng = thread_rng();
        let mut roots = Vec::with_capacity(self.settings.num_roots);
        let num_nodes = self.graph.read().unwrap().num_nodes;
        while roots.len() < self.settings.num_roots {
            let root = rng.gen_range(0..num_nodes);
            if self.node_data.is_undefended(&root) {
                roots.push(root);
            }
        }
        log::debug!("Setting nodes {:?} as fire roots", &roots);
        self.node_data.mark_burning(&roots, self.global_time);

        roots
    }

    /// Spread the fire to all nodes that are adjacent to burning nodes.
    /// Defended nodes will remain defended.
    fn spread_fire(&mut self) {
        let mut to_burn = Vec::new();
        {
            let burning = self.node_data.get_burning();

            let graph = self.graph.read().unwrap();
            let offsets = &graph.offsets;
            let edges = &graph.edges;

            // For all undefended neighbours that are not already burning, check whether they have
            // to be added to `to_burn`
            self.is_active = false;
            for node_data in burning {
                let node_id = node_data.node_id;
                for i in offsets[node_id]..offsets[node_id + 1] {
                    let edge = &edges[i];
                    if self.node_data.is_undefended(&edge.tgt) {
                        // There is at least one node to be burned at some point in the future
                        if !self.is_active {
                            self.is_active = true;
                        }
                        // Burn the node if the global time exceeds the time at which the edge source
                        // started burning plus the edge weight
                        if self.global_time >= node_data.time + edge.dist as u64 {
                            to_burn.push(edge.tgt);
                        }
                    }
                }
            }
        }

        // Burn all nodes in `to_burn`
        log::debug!("Burning nodes {:?}", &to_burn);
        self.node_data.mark_burning(&to_burn, self.global_time);
    }

    /// Execute the containment strategy to prevent as much nodes as
    /// possible from catching fire
    fn contain_fire(&mut self) {
        if self.global_time % self.settings.strategy_every == 0 {
            match self.strategy {
                OSMFStrategy::Greedy(ref mut greedy_strategy) =>
                    greedy_strategy.execute(&self.settings, &mut self.node_data, self.global_time),
                OSMFStrategy::MinDistanceGroup(ref mut mindistgroup_strategy) =>
                    mindistgroup_strategy.execute(&self.settings, &mut self.node_data, self.global_time),
                OSMFStrategy::Priority(ref mut priority_strategy) =>
                    priority_strategy.execute(&self.settings, &mut self.node_data, self.global_time)
            }
        }
    }

    /// Execute one time step in the firefighter problem.
    /// That is, execute the containment strategy, spread the fire and
    /// check whether the game is finished.
    fn exec_step(&mut self) {
        self.global_time += 1;

        self.contain_fire();
        self.spread_fire();
    }

    /// Simulate the firefighter problem until the `is_active` flag is set to `false`
    pub fn simulate(&mut self) {
        while self.is_active {
            self.exec_step();
        }
    }

    /// Generate the simulation response for this firefighter problem instance
    pub fn simulation_response(&self) -> OSMFSimulationResponse {
        OSMFSimulationResponse {
            nodes_burned: self.node_data.burning.len(),
            nodes_defended: self.node_data.defended.len(),
            nodes_total: self.graph.read().unwrap().num_nodes,
            end_time: self.global_time,
            view_bounds: &self.view.grid_bounds,
            view_center: self.view.initial_center,
        }
    }

    /// Generate the view response for this firefighter problem instance
    pub fn view_response(&mut self, center: Coords, zoom: f64, time: &TimeUnit) -> Vec<u8> {
        self.view.compute(center, zoom, time, &self.node_data);
        self.view.png_bytes()
    }

    /// Generate the alternative view response for this firefighter problem instance
    pub fn view_response_alt(&mut self, zoom: f64, time: &TimeUnit) -> Vec<u8> {
        self.view.compute_alt(zoom, time, &self.node_data);
        self.view.png_bytes()
    }

    pub fn sim_step_metadata_response(&self, time: &TimeUnit) -> OSMFSimulationStepMetadata {
        OSMFSimulationStepMetadata {
            nodes_burned_by: self.node_data.count_burning_by(time),
            nodes_defended_by: self.node_data.count_defended_by(time),
            nodes_burned_at: self.node_data.get_burning_at(time),
            nodes_defended_at: self.node_data.get_defended_at(time),
        }
    }
}

// #[derive(Debug)]
// pub enum OSMFProblemError {
//     NodeDataAlreadyAttached,
// }
//
// impl std::fmt::Display for OSMFProblemError {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         match self {
//             Self::NodeDataAlreadyAttached => write!(f, "Node data is already attached to this node")
//         }
//     }
// }
//
// impl std::error::Error for OSMFProblemError {}

#[cfg(test)]
mod test {
    use std::{collections::BTreeMap,
              sync::{Arc, RwLock}};

    use crate::firefighter::{problem::{OSMFProblem, OSMFSettings},
                             strategy::{OSMFStrategy, GreedyStrategy, Strategy}};
    use crate::graph::Graph;

    #[test]
    fn test() {
        let graph = Arc::new(RwLock::new(
            Graph::from_files("data/bbgrund")));
        let num_roots = 10;
        let strategy = OSMFStrategy::Greedy(GreedyStrategy::new(graph.clone()));
        let mut problem = OSMFProblem::new(
            graph.clone(),
            OSMFSettings {
                graph_name: "bbgrund".to_string(),
                strategy_name:"greedy".to_string(),
                num_roots,
                num_ffs: 2,
                strategy_every: 10,
            },
            strategy);

        assert_eq!(problem.node_data.burning.len(), num_roots);

        let graph_ = graph.read().unwrap();
        let num_nodes = graph_.num_nodes;

        let roots: Vec<_>;
        {
            roots = problem.node_data.burning.keys()
                .into_iter()
                .map(|k| *k)
                .collect();

            for root in &roots {
                assert!(*root < num_nodes);
            }
        }

        for _ in 0..10 {
            problem.exec_step();
            if !problem.is_active {
                break;
            }
        }

        let mut targets = Vec::new();
        let mut distances = BTreeMap::new();
        for root in &roots {
            let out_deg = graph_.get_out_degree(*root);
            targets.reserve(out_deg);
            for i in graph_.offsets[*root]..graph_.offsets[*root + 1] {
                let edge = &graph_.edges[i];
                targets.push(edge.tgt);
                distances.insert(edge.tgt, edge.dist as u64);
            }
        }

        for root in &roots {
            let root_nd = problem.node_data.burning.get(root).unwrap();
            for tgt in &targets {
                if problem.node_data.is_undefended(tgt) {
                    assert!(problem.global_time < root_nd.time + *distances.get(tgt).unwrap())
                }
            }
        }
    }
}