use crate::game::board::{Board, Building};
use crate::game::resources::DevCard;
use crate::game::state::{GameState, PlayerId, WIN_VP};

/// Longest continuous road length (in roads) for a player.
///
/// Opponents' settlements/cities break a road: a path may not continue through
/// a vertex occupied by another player's building (it may still end there).
pub fn longest_road_length(board: &Board, pid: PlayerId) -> u8 {
    let mut incident: Vec<Vec<usize>> = vec![Vec::new(); board.vertices.len()];
    for (ei, e) in board.edges.iter().enumerate() {
        if e.owner == Some(pid) {
            incident[e.a].push(ei);
            incident[e.b].push(ei);
        }
    }

    let blocked = |v: usize| -> bool {
        let vt = &board.vertices[v];
        vt.building != Building::None && vt.owner != Some(pid)
    };

    let mut best = 0u8;

    fn dfs(
        board: &Board,
        incident: &[Vec<usize>],
        blocked: &impl Fn(usize) -> bool,
        start: usize,
        vertex: usize,
        used: &mut Vec<bool>,
        len: u8,
        best: &mut u8,
        is_start: bool,
    ) {
        // A road that started at an opponent's building must not loop back onto
        // it: the building breaks the road, so both of its roads cannot be part
        // of one continuous run. (A loop among the player's own vertices still
        // counts in full, matching the ring behaviour elsewhere.)
        if !is_start && vertex == start && blocked(start) {
            return;
        }
        if len > *best {
            *best = len;
        }
        // A path may not continue *through* a vertex occupied by an opponent,
        // but such a vertex is still a legal end of the road.
        if !is_start && blocked(vertex) {
            return;
        }
        for &ei in &incident[vertex] {
            if used[ei] {
                continue;
            }
            let e = &board.edges[ei];
            let Some(other) = e.other(vertex) else { continue };
            used[ei] = true;
            dfs(board, incident, blocked, start, other, used, len + 1, best, false);
            used[ei] = false;
        }
    }

    let mut used = vec![false; board.edges.len()];
    for v in 0..board.vertices.len() {
        // Start from every vertex that touches a road of this player, even an
        // opponent's building: a road segment may be bracketed by opponent
        // buildings at both ends, and only starting there counts the whole run.
        if incident[v].is_empty() {
            continue;
        }
        dfs(
            board,
            &incident,
            &blocked,
            v,
            v,
            &mut used,
            0,
            &mut best,
            true,
        );
    }
    best
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
