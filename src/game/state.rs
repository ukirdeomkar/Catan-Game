use crate::game::board::{Board, Building};
use crate::game::resources::{
    ALL_RESOURCES, Bundle, DevCard, Resource, ResourceHand, standard_dev_deck,
};
use crate::game::rng::Rng64;
use rand::Rng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

pub type PlayerId = usize;

pub const MAX_PLAYERS: usize = 4;
pub const MIN_PLAYERS: usize = 2;
pub const WIN_VP: u8 = 10;
pub const BANK_PER_RESOURCE: u8 = 19;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Color {
    Red,
    Blue,
    Orange,
    White,
}

impl Color {
    pub fn slug(self) -> &'static str {
        match self {
            Color::Red => "red",
            Color::Blue => "blue",
            Color::Orange => "orange",
            Color::White => "white",
        }
    }

    pub const ALL: [Color; 4] = [Color::Red, Color::Blue, Color::Orange, Color::White];
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub color: Color,
    pub is_bot: bool,
    pub connected: bool,
    pub resources: ResourceHand,
    /// Playable development cards (bought on a previous turn).
    pub dev_cards: Vec<DevCard>,
    /// Cards bought this turn; not playable until the next turn.
    pub new_dev_cards: Vec<DevCard>,
    pub played_knights: u8,
    pub roads_left: u8,
    pub settlements_left: u8,
    pub cities_left: u8,
}

impl Player {
    pub fn new(id: PlayerId, name: String, color: Color, is_bot: bool) -> Player {
        Player {
            id,
            name,
            color,
            is_bot,
            connected: true,
            resources: ResourceHand::default(),
            dev_cards: Vec::new(),
            new_dev_cards: Vec::new(),
            played_knights: 0,
            roads_left: 15,
            settlements_left: 5,
            cities_left: 4,
        }
    }

    pub fn all_dev_cards(&self) -> impl Iterator<Item = DevCard> + '_ {
        self.dev_cards.iter().chain(self.new_dev_cards.iter()).copied()
    }

    pub fn secret_vp(&self) -> u8 {
        self.all_dev_cards()
            .filter(|c| *c == DevCard::VictoryPoint)
            .count() as u8
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    /// Waiting for players to join and the host to start.
    Lobby,
    /// Initial snake placement (settlement + road each).
    Setup,
    /// Normal turn play.
    Play,
    /// One or more players must discard half their hand (rolled a 7).
    Discard,
    /// Current player must move the robber. `after_knight` = no discard step.
    MoveRobber { after_knight: bool },
    /// Current player picks who to steal from (or nobody).
    Steal { hex: usize },
    GameOver,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEntry {
    pub turn: u32,
    pub actor: Option<PlayerId>,
    pub text: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum TradeResponse {
    Pending,
    Accepted,
    Declined,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TradeOffer {
    pub from: PlayerId,
    pub give: Bundle,
    pub want: Bundle,
    pub responses: Vec<(PlayerId, TradeResponse)>,
    /// Absolute unix-ms deadline; 0 means no timer. Set by the server layer.
    #[serde(default)]
    pub deadline_ms: u64,
    /// Unique id of this offer, used to expire exactly this offer.
    #[serde(default)]
    pub id: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameState {
    pub board: Board,
    pub players: Vec<Player>,
    pub bank: ResourceHand,
    pub current: PlayerId,
    pub phase: Phase,
    pub turn: u32,
    /// Snake order of player ids for initial placement.
    pub setup_queue: Vec<PlayerId>,
    pub setup_pos: usize,
    /// If set, the current setup player still owes a road on this settlement vertex.
    pub setup_road_from: Option<usize>,
    pub robber_hex: usize,
    pub dev_deck: Vec<DevCard>,
    pub dice: Option<(u8, u8)>,
    pub dev_bought_this_turn: bool,
    pub played_dev_this_turn: bool,
    pub longest_road: Option<PlayerId>,
    pub largest_army: Option<PlayerId>,
    pub winner: Option<PlayerId>,
    pub log: Vec<LogEntry>,
    pub pending_discards: Vec<PlayerId>,
    pub trade: Option<TradeOffer>,
    /// Monotonic id source for trade offers.
    #[serde(default)]
    pub trade_seq: u64,
    /// Active multi-step dev card (Road Building / Year of Plenty / Monopoly).
    pub free_roads_left: u8,
    pub year_of_plenty_left: u8,
    pub steal_hex: Option<usize>,
    pub last_seed: u64,
    pub rng: Rng64,
}

pub struct PlayerConfig {
    pub name: String,
    pub color: Color,
    pub is_bot: bool,
}

impl GameState {
    /// A fresh game in the lobby phase (no board yet beyond a placeholder).
    pub fn new_lobby(players: Vec<PlayerConfig>, seed: u64) -> GameState {
        let mut rng = Rng64::new(seed);
        let board = Board::generate(&mut rng);
        let players: Vec<Player> = players
            .into_iter()
            .enumerate()
            .map(|(i, c)| Player::new(i, c.name, c.color, c.is_bot))
            .collect();
        let robber_hex = board.robber_start();
        GameState {
            board,
            players,
            bank: ResourceHand([BANK_PER_RESOURCE; 5]),
            current: 0,
            phase: Phase::Lobby,
            turn: 0,
            setup_queue: Vec::new(),
            setup_pos: 0,
            setup_road_from: None,
            robber_hex,
            dev_deck: standard_dev_deck(),
            dice: None,
            dev_bought_this_turn: false,
            played_dev_this_turn: false,
            longest_road: None,
            largest_army: None,
            winner: None,
            log: Vec::new(),
            pending_discards: Vec::new(),
            trade: None,
            trade_seq: 0,
            free_roads_left: 0,
            year_of_plenty_left: 0,
            steal_hex: None,
            last_seed: seed,
            rng,
        }
    }

    /// Transition from lobby to setup, shuffling a fresh board and orders.
    pub fn start(&mut self) {
        let seed = self.rng.next_u64();
        self.last_seed = seed;
        self.board = Board::generate(&mut self.rng);
        self.robber_hex = self.board.robber_start();
        self.dev_deck = standard_dev_deck();
        self.dev_deck.shuffle(&mut self.rng);
        self.bank = ResourceHand([BANK_PER_RESOURCE; 5]);

        let n = self.players.len();
        let mut queue: Vec<PlayerId> = (0..n).collect();
        let mut tail: Vec<PlayerId> = (0..n).rev().collect();
        queue.append(&mut tail);
        self.setup_queue = queue;
        self.setup_pos = 0;
        self.setup_road_from = None;
        self.current = self.setup_queue[0];
        self.phase = Phase::Setup;
        self.log.clear();
        self.push_log(None, "The game begins. Place settlements and roads in snake order.");
    }

    // --- Basic accessors -------------------------------------------------

    pub fn player(&self, id: PlayerId) -> &Player {
        &self.players[id]
    }

    pub fn player_mut(&mut self, id: PlayerId) -> &mut Player {
        &mut self.players[id]
    }

    pub fn current_player(&self) -> &Player {
        &self.players[self.current]
    }

    pub fn num_players(&self) -> usize {
        self.players.len()
    }

    pub fn is_over(&self) -> bool {
        matches!(self.phase, Phase::GameOver)
    }

    pub fn robber_terrain(&self) -> crate::game::board::Terrain {
        self.board.hexes[self.robber_hex].terrain
    }

    // --- Logging ---------------------------------------------------------

    pub fn push_log(&mut self, actor: Option<PlayerId>, text: impl Into<String>) {
        self.log.push(LogEntry {
            turn: self.turn,
            actor,
            text: text.into(),
        });
        if self.log.len() > 300 {
            let overflow = self.log.len() - 300;
            self.log.drain(0..overflow);
        }
    }

    // --- Bank ------------------------------------------------------------

    pub fn bank_take(&mut self, r: Resource, n: u8) -> u8 {
        let have = self.bank.get(r);
        let taken = have.min(n);
        self.bank.set(r, have - taken);
        taken
    }

    pub fn bank_give(&mut self, r: Resource, n: u8) {
        self.bank.set(r, self.bank.get(r).saturating_add(n));
    }

    pub fn bank_pay(&mut self, b: &Bundle, to: PlayerId) -> bool {
        for &r in ALL_RESOURCES.iter() {
            if self.bank.get(r) < b.get(r) {
                return false;
            }
        }
        for &r in ALL_RESOURCES.iter() {
            self.bank.set(r, self.bank.get(r) - b.get(r));
            self.players[to].resources.add(r, b.get(r));
        }
        true
    }

    // --- Building helpers ------------------------------------------------

    pub fn has_piece_available(&self, pid: PlayerId, building: Building) -> bool {
        let p = &self.players[pid];
        match building {
            Building::Settlement => p.settlements_left > 0,
            Building::City => p.cities_left > 0,
            Building::None => true,
        }
    }

    pub fn place_building(&mut self, pid: PlayerId, vertex: usize, building: Building) {
        {
            let v = &mut self.board.vertices[vertex];
            v.owner = Some(pid);
            v.building = building;
        }
        match building {
            Building::Settlement => self.players[pid].settlements_left -= 1,
            Building::City => self.players[pid].cities_left -= 1,
            Building::None => {}
        }
    }

    pub fn upgrade_building(&mut self, pid: PlayerId, vertex: usize) {
        self.board.vertices[vertex].building = Building::City;
        self.players[pid].cities_left -= 1;
        self.players[pid].settlements_left += 1; // settlement piece returns to stock
    }

    pub fn place_road(&mut self, pid: PlayerId, edge: usize) {
        self.board.edges[edge].owner = Some(pid);
        self.players[pid].roads_left -= 1;
    }
}
