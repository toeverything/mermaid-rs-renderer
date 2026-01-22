use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    TopDown,
    LeftRight,
}

impl Direction {
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "TD" | "TB" => Some(Self::TopDown),
            "LR" => Some(Self::LeftRight),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub shape: NodeShape,
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub label: Option<String>,
    pub directed: bool,
    pub arrow_start: bool,
    pub arrow_end: bool,
    pub style: EdgeStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeStyle {
    Solid,
    Dotted,
    Thick,
}

#[derive(Debug, Clone)]
pub struct Subgraph {
    pub id: Option<String>,
    pub label: String,
    pub nodes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Graph {
    pub direction: Direction,
    pub nodes: BTreeMap<String, Node>,
    pub edges: Vec<Edge>,
    pub subgraphs: Vec<Subgraph>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeShape {
    Rectangle,
    RoundRect,
    Stadium,
    Subroutine,
    Cylinder,
    Circle,
    DoubleCircle,
    Diamond,
    Hexagon,
    Parallelogram,
    ParallelogramAlt,
    Trapezoid,
    TrapezoidAlt,
    Asymmetric,
    Text,
}

impl Graph {
    pub fn new() -> Self {
        Self {
            direction: Direction::TopDown,
            nodes: BTreeMap::new(),
            edges: Vec::new(),
            subgraphs: Vec::new(),
        }
    }

    pub fn ensure_node(&mut self, id: &str, label: Option<String>, shape: Option<NodeShape>) {
        let entry = self.nodes.entry(id.to_string()).or_insert(Node {
            id: id.to_string(),
            label: id.to_string(),
            shape: NodeShape::Rectangle,
        });
        if let Some(label) = label {
            entry.label = label;
        }
        if let Some(shape) = shape {
            entry.shape = shape;
        }
    }
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}
