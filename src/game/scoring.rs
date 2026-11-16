use crate::game::board::{Board, Building};
use crate::game::resources::DevCard;
use crate::game::state::{GameState, PlayerId, WIN_VP};

/// Longest continuous road length (in roads) for a player.
///
/// Opponents' settlements/cities break a road: a path may not continue through
/// a vertex occupied by another player's building (it may still end there).
pub fn longest_road_length(board: &Board, pid: PlayerId) -> u8 {
    RoadProbe::new(board, pid).longest()
}

/// One player's road network, precomputed once so the same board can be asked
/// about many hypothetical roads.
///
/// The bot scores every legal road candidate before building, and each score
/// needs "how long would my longest road be if I owned this edge?". Answering
/// that by cloning the `GameState` per candidate was the dominant hard-bot CPU
/// cost. A probe costs two small vectors and answers every candidate without
/// touching the state.
pub struct RoadProbe<'a> {
    board: &'a Board,
    /// Edges this player already owns, indexed by vertex.
    incident: Vec<Vec<usize>>,
    /// A vertex occupied by someone else — a road may end there, not pass through.
    blocked: Vec<bool>,
}

impl<'a> RoadProbe<'a> {
    pub fn new(board: &'a Board, pid: PlayerId) -> Self {
        let mut incident: Vec<Vec<usize>> = vec![Vec::new(); board.vertices.len()];
        for (ei, e) in board.edges.iter().enumerate() {
            if e.owner == Some(pid) {
                incident[e.a].push(ei);
                incident[e.b].push(ei);
            }
        }
        let blocked = board
            .vertices
            .iter()
            .map(|vt| vt.building != Building::None && vt.owner != Some(pid))
            .collect();
        Self {
            board,
            incident,
            blocked,
        }
    }

    /// The player's longest road as the board stands.
    pub fn longest(&self) -> u8 {
        self.length(None)
    }

    /// The player's longest road if they also owned `edge`.
    ///
    /// Identical to cloning the game and setting `edges[edge].owner = Some(pid)`,
    /// which is why the bot's road choices are unchanged.
    pub fn with_extra(&self, edge: usize) -> u8 {
        let e = &self.board.edges[edge];
        // An edge the player already owns is already wired into `incident`.
        let extra = if self.incident[e.a].contains(&edge) {
            None
        } else {
            Some(edge)
        };
        self.length(extra)
    }

    fn length(&self, extra: Option<usize>) -> u8 {
        let mut best = 0u8;
        let mut used = vec![false; self.board.edges.len()];
        for v in 0..self.board.vertices.len() {
            // Start from every vertex that touches a road of this player, even
            // an opponent's building: a road segment may be bracketed by
            // opponent buildings at both ends, and only starting there counts
            // the whole run. With a hypothetical road, its ends are starts too.
            if self.incident[v].is_empty() && !self.touches(v, extra) {
                continue;
            }
            self.dfs(v, v, &mut used, 0, &mut best, true, extra);
        }
        best
    }

    /// Does the hypothetical road end at `v`?
    fn touches(&self, v: usize, extra: Option<usize>) -> bool {
        extra.is_some_and(|e| self.board.edges[e].other(v).is_some())
    }

    #[allow(clippy::too_many_arguments)]
    fn dfs(
        &self,
        start: usize,
        vertex: usize,
        used: &mut [bool],
        len: u8,
        best: &mut u8,
        is_start: bool,
        extra: Option<usize>,
    ) {
        // A road that started at an opponent's building must not loop back onto
        // it: the building breaks the road, so both of its roads cannot be part
        // of one continuous run. (A loop among the player's own vertices still
        // counts in full, matching the ring behaviour elsewhere.)
        if !is_start && vertex == start && self.blocked[start] {
            return;
        }
        if len > *best {
            *best = len;
        }
        // A path may not continue *through* a vertex occupied by an opponent,
        // but such a vertex is still a legal end of the road.
        if !is_start && self.blocked[vertex] {
            return;
        }
        for &ei in &self.incident[vertex] {
            if used[ei] {
                continue;
            }
            let Some(other) = self.board.edges[ei].other(vertex) else {
                continue;
            };
            used[ei] = true;
            self.dfs(start, other, used, len + 1, best, false, extra);
            used[ei] = false;
        }
        // The hypothetical road is not in `incident`, so offer it here.
        if let Some(ei) = extra.filter(|&e| !used[e]) {
            if let Some(other) = self.board.edges[ei].other(vertex) {
                used[ei] = true;
                self.dfs(start, other, used, len + 1, best, false, extra);
                used[ei] = false;
            }
        }
    }
}

impl GameState {
    pub fn public_victory_points(&self, pid: PlayerId) -> u8 {
        let mut vp = 0u8;
        for v in &self.board.vertices {
            if v.owner == Some(pid) {
                vp += match v.building {
                    Building::Settlement => 1,
                    Building::City => 2,
                    Building::None => 0,
                };
            }
        }
        if self.longest_road == Some(pid) {
            vp += 2;
        }
        if self.largest_army == Some(pid) {
            vp += 2;
        }
        // Victory Point cards the owner has revealed are public.
        vp += self.players[pid].revealed_vp;
        vp
    }

    pub fn total_victory_points(&self, pid: PlayerId) -> u8 {
        self.public_victory_points(pid) + self.players[pid].secret_vp()
    }

    pub fn army_size(&self, pid: PlayerId) -> u8 {
        self.players[pid].played_knights
    }

    /// Re-evaluate Longest Road and Largest Army after any board/piece change.
    pub fn recompute_special_cards(&mut self) {
        self.recompute_longest_road();
        self.recompute_largest_army();
    }

    fn recompute_longest_road(&mut self) {
        let lens: Vec<u8> = (0..self.players.len())
            .map(|pid| longest_road_length(&self.board, pid))
            .collect();
        let best = lens.iter().copied().max().unwrap_or(0);
        let new = if best < 5 {
            None
        } else {
            let candidates: Vec<PlayerId> = (0..lens.len()).filter(|&p| lens[p] == best).collect();
            if candidates.len() == 1 {
                Some(candidates[0])
            } else if let Some(h) = self.longest_road {
                if candidates.contains(&h) { Some(h) } else { None }
            } else {
                None
            }
        };
        if new != self.longest_road {
            match new {
                Some(p) => {
                    let name = self.players[p].name.clone();
                    self.push_log(Some(p), format!("{name} takes Longest Road ({best})."));
                }
                None => {
                    if let Some(h) = self.longest_road {
                        let name = self.players[h].name.clone();
                        self.push_log(None, format!("{name} loses Longest Road."));
                    }
                }
            }
        }
        self.longest_road = new;
    }

    fn recompute_largest_army(&mut self) {
        let knights: Vec<u8> = (0..self.players.len())
            .map(|pid| self.players[pid].played_knights)
            .collect();
        let best = knights.iter().copied().max().unwrap_or(0);
        let new = if best < 3 {
            None
        } else {
            let candidates: Vec<PlayerId> =
                (0..knights.len()).filter(|&p| knights[p] == best).collect();
            if candidates.len() == 1 {
                Some(candidates[0])
            } else if let Some(h) = self.largest_army {
                if candidates.contains(&h) { Some(h) } else { None }
            } else {
                None
            }
        };
        if new != self.largest_army {
            match new {
                Some(p) => {
                    let name = self.players[p].name.clone();
                    self.push_log(Some(p), format!("{name} takes Largest Army ({best} knights)."));
                }
                None => {
                    if let Some(h) = self.largest_army {
                        let name = self.players[h].name.clone();
                        self.push_log(None, format!("{name} loses Largest Army."));
                    }
                }
            }
        }
        self.largest_army = new;
    }

    /// Count the number of Victory Point dev cards across the board (for UI hints).
    pub fn vp_cards_remaining(&self) -> usize {
        self.dev_deck.iter().filter(|c| **c == DevCard::VictoryPoint).count()
    }

    /// Check for a winner; returns true if the game just ended.
    pub fn check_winner(&mut self) -> bool {
        if self.is_over() {
            return false;
        }
        for pid in 0..self.players.len() {
            if self.total_victory_points(pid) >= WIN_VP {
                self.winner = Some(pid);
                self.phase = crate::game::state::Phase::GameOver;
                let name = self.players[pid].name.clone();
                let vp = self.total_victory_points(pid);
                self.push_log(Some(pid), format!("{name} wins with {vp} victory points!"));
                return true;
            }
        }
        false
    }
}
