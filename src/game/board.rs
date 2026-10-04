use crate::game::resources::Resource;
use rand::Rng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const HEX_RADIUS: i32 = 2;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Terrain {
    Wood,
    Brick,
    Wheat,
    Ore,
    Sheep,
    Desert,
}

impl Terrain {
    pub fn resource(self) -> Option<Resource> {
        Some(match self {
            Terrain::Wood => Resource::Wood,
            Terrain::Brick => Resource::Brick,
            Terrain::Wheat => Resource::Wheat,
            Terrain::Ore => Resource::Ore,
            Terrain::Sheep => Resource::Sheep,
            Terrain::Desert => return None,
        })
    }

    pub fn slug(self) -> &'static str {
        match self {
            Terrain::Wood => "wood",
            Terrain::Brick => "brick",
            Terrain::Wheat => "wheat",
            Terrain::Ore => "ore",
            Terrain::Sheep => "sheep",
            Terrain::Desert => "desert",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Terrain::Wood => "Forest",
            Terrain::Brick => "Hills",
            Terrain::Wheat => "Fields",
            Terrain::Ore => "Mountains",
            Terrain::Sheep => "Pasture",
            Terrain::Desert => "Desert",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Building {
    None,
    Settlement,
    City,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum PortKind {
    Generic,
    Wood,
    Brick,
    Wheat,
    Ore,
    Sheep,
}

impl PortKind {
    pub fn resource(self) -> Option<Resource> {
        Some(match self {
            PortKind::Generic => return None,
            PortKind::Wood => Resource::Wood,
            PortKind::Brick => Resource::Brick,
            PortKind::Wheat => Resource::Wheat,
            PortKind::Ore => Resource::Ore,
            PortKind::Sheep => Resource::Sheep,
        })
    }

    pub fn ratio(self) -> u8 {
        match self {
            PortKind::Generic => 3,
            _ => 2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PortKind::Generic => "3:1",
            PortKind::Wood => "2:1 Wood",
            PortKind::Brick => "2:1 Brick",
            PortKind::Wheat => "2:1 Wheat",
            PortKind::Ore => "2:1 Ore",
            PortKind::Sheep => "2:1 Sheep",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hex {
    pub q: i32,
    pub r: i32,
    pub terrain: Terrain,
    pub number: Option<u8>,
    pub vertices: [usize; 6],
    pub cx: f64,
    pub cy: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vertex {
    pub hexes: Vec<usize>,
    pub edges: Vec<usize>,
    pub owner: Option<usize>,
    pub building: Building,
    pub port: Option<PortKind>,
    pub x: f64,
    pub y: f64,
}

impl Vertex {
    pub fn is_coastal(&self) -> bool {
        self.port.is_some()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Edge {
    pub a: usize,
    pub b: usize,
    pub hexes: Vec<usize>,
    pub owner: Option<usize>,
}

impl Edge {
    pub fn other(&self, v: usize) -> Option<usize> {
        if v == self.a {
            Some(self.b)
        } else if v == self.b {
            Some(self.a)
        } else {
            None
        }
    }

    pub fn verts(&self) -> [usize; 2] {
        [self.a, self.b]
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Board {
    pub hexes: Vec<Hex>,
    pub vertices: Vec<Vertex>,
    pub edges: Vec<Edge>,
}

impl Board {
    /// Generate a fresh, fully random board (terrain, number tokens, and ports).
    pub fn generate<R: Rng>(rng: &mut R) -> Board {
        let coords = hex_coordinates();
        let centers: Vec<(f64, f64)> = coords
            .iter()
            .map(|&(q, r)| axial_to_pixel(q, r))
            .collect();

        // --- Vertices and edges via rounded-coordinate dedup ---
        let mut vertex_index: HashMap<(i64, i64), usize> = HashMap::new();
        let mut vertices: Vec<Vertex> = Vec::with_capacity(54);
        let mut edge_index: HashMap<(usize, usize), usize> = HashMap::new();
        let mut edges: Vec<Edge> = Vec::with_capacity(72);
        let mut hex_vertex_ids: Vec<[usize; 6]> = Vec::with_capacity(coords.len());

        for (h, &(cx, cy)) in centers.iter().enumerate() {
            let mut ids = [0usize; 6];
            for (i, id) in ids.iter_mut().enumerate() {
                let ang = (60.0 * i as f64 - 30.0).to_radians();
                let (x, y) = (cx + ang.cos(), cy + ang.sin());
                let key = (round4(x), round4(y));
                let idx = *vertex_index.entry(key).or_insert_with(|| {
                    vertices.push(Vertex {
                        hexes: Vec::new(),
                        edges: Vec::new(),
                        owner: None,
                        building: Building::None,
                        port: None,
                        x,
                        y,
                    });
                    vertices.len() - 1
                });
                vertices[idx].hexes.push(h);
                *id = idx;
            }
            hex_vertex_ids.push(ids);
        }

        for (h, ids) in hex_vertex_ids.iter().enumerate() {
            for i in 0..6 {
                let a = ids[i];
                let b = ids[(i + 1) % 6];
                let key = if a < b { (a, b) } else { (b, a) };
                let ei = *edge_index.entry(key).or_insert_with(|| {
                    edges.push(Edge {
                        a: key.0,
                        b: key.1,
                        hexes: Vec::new(),
                        owner: None,
                    });
                    edges.len() - 1
                });
                edges[ei].hexes.push(h);
                if !vertices[a].edges.contains(&ei) {
                    vertices[a].edges.push(ei);
                }
                if !vertices[b].edges.contains(&ei) {
                    vertices[b].edges.push(ei);
                }
            }
        }

        // --- Terrain (4 wood, 4 wheat, 4 sheep, 3 brick, 3 ore, 1 desert) ---
        let mut terrains = vec![
            Terrain::Wood,
            Terrain::Wood,
            Terrain::Wood,
            Terrain::Wood,
            Terrain::Wheat,
            Terrain::Wheat,
            Terrain::Wheat,
            Terrain::Wheat,
            Terrain::Sheep,
            Terrain::Sheep,
            Terrain::Sheep,
            Terrain::Sheep,
            Terrain::Brick,
            Terrain::Brick,
            Terrain::Brick,
            Terrain::Ore,
            Terrain::Ore,
            Terrain::Ore,
            Terrain::Desert,
        ];
        terrains.shuffle(rng);

        // --- Number tokens: two each of 3..11 (no 7), one each of 2 and 12 ---
        let mut numbers: Vec<u8> = vec![
            2, 3, 3, 4, 4, 5, 5, 6, 6, 8, 8, 9, 9, 10, 10, 11, 11, 12,
        ];
        numbers.shuffle(rng);

        let mut hexes = Vec::with_capacity(coords.len());
        let mut nidx = 0;
        for (i, &(q, r)) in coords.iter().enumerate() {
            let terrain = terrains[i];
            let number = if terrain == Terrain::Desert {
                None
            } else {
                let n = numbers[nidx];
                nidx += 1;
                Some(n)
            };
            let (cx, cy) = centers[i];
            hexes.push(Hex {
                q,
                r,
                terrain,
                number,
                vertices: hex_vertex_ids[i],
                cx,
                cy,
            });
        }

        // --- Ports: 9 coastal edges, evenly spread by angle ---
        let coastal: Vec<usize> = (0..edges.len())
            .filter(|&e| edges[e].hexes.len() == 1)
            .collect();
        let mut by_angle: Vec<(f64, usize)> = coastal
            .iter()
            .map(|&e| {
                let a = &vertices[edges[e].a];
                let b = &vertices[edges[e].b];
                (
                    (0.5 * (a.y + b.y)).atan2(0.5 * (a.x + b.x)),
                    e,
                )
            })
            .collect();
        by_angle.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        let mut port_kinds = vec![
            PortKind::Generic,
            PortKind::Generic,
            PortKind::Generic,
            PortKind::Generic,
            PortKind::Wood,
            PortKind::Brick,
            PortKind::Wheat,
            PortKind::Ore,
            PortKind::Sheep,
        ];
        port_kinds.shuffle(rng);

        let n = by_angle.len();
        for (k, kind) in port_kinds.into_iter().enumerate() {
            // Evenly spaced selection around the perimeter.
            let idx = (k * n) / 9;
            let e = by_angle[idx].1;
            let (va, vb) = (edges[e].a, edges[e].b);
            vertices[va].port = Some(kind);
            vertices[vb].port = Some(kind);
        }

        Board {
            hexes,
            vertices,
            edges,
        }
    }

    pub fn vertex_hexes(&self, v: usize) -> &[usize] {
        &self.vertices[v].hexes
    }

    /// Best (lowest) maritime trade ratio for `resource` for a player owning `v`.
    pub fn maritime_ratio(&self, v: usize, resource: Resource) -> Option<u8> {
        let port = self.vertices[v].port?;
        match port.resource() {
            Some(r) if r == resource => Some(2),
            Some(_) => Some(4), // specific port for a different resource: no benefit
            None => Some(3),
        }
    }

    pub fn robber_start(&self) -> usize {
        self.hexes
            .iter()
            .position(|h| h.terrain == Terrain::Desert)
            .unwrap_or(0)
    }
}

fn hex_coordinates() -> Vec<(i32, i32)> {
    let mut v = Vec::with_capacity(19);
    for r in -HEX_RADIUS..=HEX_RADIUS {
        let q_min = (-HEX_RADIUS).max(-r - HEX_RADIUS);
        let q_max = HEX_RADIUS.min(-r + HEX_RADIUS);
        for q in q_min..=q_max {
            v.push((q, r));
        }
    }
    v
}

fn axial_to_pixel(q: i32, r: i32) -> (f64, f64) {
    let q = q as f64;
    let r = r as f64;
    let x = 3f64.sqrt() * (q + r / 2.0);
    let y = 1.5 * r;
    (x, y)
}

fn round4(x: f64) -> i64 {
    (x * 1000.0).round() as i64
}
