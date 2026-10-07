use crate::bot;
use crate::game::rng::Rng64;
use crate::game::state::{
    BotLevel, Color, GameState, MAX_PLAYERS, MIN_PLAYERS, Phase, PlayerConfig, PlayerId,
    TradeResponse,
};
use rand::Rng;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::watch;

pub const ROOM_CODE_LEN: usize = 4;
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ViewMode {
    #[default]
    Normal,
    PlaceRoad,
    PlaceSettlement,
    PlaceCity,
}

/// Per-viewer visual style. `Board` is the painted "offline board" revamp;
/// `Classic` is the original flat colonist-style look. Chosen independently by
/// each member and shared across every fragment they render.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Layout {
    Board,
    #[default]
    Classic,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Member {
    pub token: String,
    pub name: String,
    pub color: Color,
    pub is_bot: bool,
    #[serde(default)]
    pub level: BotLevel,
    #[serde(default)]
    pub connected: bool,
    #[serde(default)]
    pub last_seen_ms: u64,
    #[serde(default)]
    pub mode: ViewMode,
    #[serde(default)]
    pub layout: Layout,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RoomData {
    pub code: String,
    pub host_token: String,
    pub members: Vec<Member>,
    pub game: Option<GameState>,
    pub started: bool,
    pub created_ms: u64,
    pub last_activity_ms: u64,
    /// Per-turn time limit in seconds (0 = untimed), chosen at creation.
    #[serde(default)]
    pub turn_seconds: u64,
    /// Absolute unix-ms deadline for the current turn (0 = not armed).
    #[serde(skip)]
    pub turn_deadline_ms: u64,
    /// Turn key the deadline was armed for, so the timer re-arms per turn.
    #[serde(skip)]
    pub turn_armed_for: u64,
    /// Earliest unix-ms a bot may take its next action (pacing).
    #[serde(skip)]
    pub bot_next_ms: u64,
    /// Context key for the bot's current decision; a change re-arms the delay.
    #[serde(skip)]
    pub bot_key: u64,
}

impl RoomData {
    pub fn member_index(&self, token: &str) -> Option<usize> {
        self.members.iter().position(|m| m.token == token)
    }

    pub fn is_host(&self, token: &str) -> bool {
        self.host_token == token
    }

    pub fn human_count(&self) -> usize {
        self.members.iter().filter(|m| !m.is_bot).count()
    }

    pub fn can_start(&self) -> bool {
        !self.started && self.members.len() >= MIN_PLAYERS
    }

    pub fn free_color(&self) -> Color {
        for c in Color::ALL {
            if !self.members.iter().any(|m| m.color == c) {
                return c;
            }
        }
        Color::ALL[0]
    }

    /// Auto-place a bot at the host's chosen difficulty.
    pub fn add_bot(&mut self, level: BotLevel) -> bool {
        if self.started || self.members.len() >= MAX_PLAYERS {
            return false;
        }
        let n = self.members.iter().filter(|m| m.is_bot).count() + 1;
        let token = format!("bot-{}-{}", self.code, n);
        let color = self.free_color();
        self.members.push(Member {
            token,
            name: format!("Bot {n}"),
            color,
            is_bot: true,
            level,
            connected: true,
            last_seen_ms: now_ms(),
            mode: ViewMode::Normal,
            layout: Layout::default(),
        });
        true
    }
}

pub struct Room {
    pub data: Mutex<RoomData>,
    seq: AtomicU64,
    version: watch::Sender<u64>,
}

impl Room {
    fn new(data: RoomData) -> Arc<Room> {
        let (version, _) = watch::channel(0u64);
        Arc::new(Room {
            data: Mutex::new(data),
            seq: AtomicU64::new(0),
            version,
        })
    }

    /// Notify all SSE listeners that room state changed.
    pub fn bump(&self) {
        let v = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = self.version.send(v);
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.version.subscribe()
    }
}

pub struct AppState {
    pub rooms: RwLock<HashMap<String, Arc<Room>>>,
    pub data_dir: PathBuf,
    rng: Mutex<Rng64>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum JoinError {
    NotFound,
    Full,
    AlreadyStarted,
    NoName,
    NameTooLong,
    NameTaken,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Arc<AppState> {
        let app = Arc::new(AppState {
            rooms: RwLock::new(HashMap::new()),
            data_dir,
            rng: Mutex::new(Rng64::from_entropy()),
        });
        app.load_from_disk();
        app
    }

    fn load_from_disk(&self) {
        let Ok(entries) = std::fs::read_dir(&self.data_dir) else {
            return;
        };
        let mut map = self.rooms.write().unwrap();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(bytes) = std::fs::read(&path) {
                if let Ok(data) = serde_json::from_slice::<RoomData>(&bytes) {
                    let code = data.code.clone();
                    map.insert(code, Room::new(data));
                }
            }
        }
    }

    pub fn room(&self, code: &str) -> Option<Arc<Room>> {
        self.rooms.read().unwrap().get(code).cloned()
    }

    pub fn create_room(
        &self,
        name: String,
        turn_seconds: u64,
    ) -> Result<(Arc<Room>, String), JoinError> {
        let name = validate_name(&name)?;
        let code = self.unique_code();
        let token = new_token();
        let data = RoomData {
            code: code.clone(),
            host_token: token.clone(),
            members: vec![Member {
                token: token.clone(),
                name,
                color: Color::Red,
                is_bot: false,
                level: BotLevel::default(),
                connected: true,
                last_seen_ms: now_ms(),
                mode: ViewMode::Normal,
            layout: Layout::default(),
            }],
            game: None,
            started: false,
            created_ms: now_ms(),
            last_activity_ms: now_ms(),
            turn_seconds,
            turn_deadline_ms: 0,
            turn_armed_for: u64::MAX,
            bot_next_ms: 0,
            bot_key: u64::MAX,
        };
        let room = Room::new(data);
        self.rooms
            .write()
            .unwrap()
            .insert(code.clone(), room.clone());
        self.persist(&room);
        Ok((room, token))
    }

    pub fn join(&self, code: &str, name: String) -> Result<(Arc<Room>, String), JoinError> {
        let room = self.room(code).ok_or(JoinError::NotFound)?;
        let name = validate_name(&name)?;
        let token = {
            let mut data = room.data.lock().unwrap();
            // Rejoin: a human with the same name (case-insensitive) already
            // holds a seat — hand them a fresh token for that seat so they
            // resume after closing the tab.
            if let Some(seat) = data
                .members
                .iter()
                .position(|m| !m.is_bot && m.name.to_lowercase() == name.to_lowercase())
            {
                let token = new_token();
                data.members[seat].token = token.clone();
                data.members[seat].connected = true;
                data.members[seat].last_seen_ms = now_ms();
                data.last_activity_ms = now_ms();
                token
            } else {
                if data.started {
                    return Err(JoinError::AlreadyStarted);
                }
                if data.members.len() >= MAX_PLAYERS {
                    return Err(JoinError::Full);
                }
                let token = new_token();
                let color = data.free_color();
                data.members.push(Member {
                    token: token.clone(),
                    name,
                    color,
                    is_bot: false,
                    level: BotLevel::default(),
                    connected: true,
                    last_seen_ms: now_ms(),
                    mode: ViewMode::Normal,
            layout: Layout::default(),
                });
                data.last_activity_ms = now_ms();
                token
            }
        };
        room.bump();
        self.persist(&room);
        Ok((room, token))
    }

    pub fn start_game(&self, room: &Arc<Room>, token: &str) -> Result<(), String> {
        {
            let mut data = room.data.lock().unwrap();
            if !data.is_host(token) {
                return Err("Only the host can start the game".into());
            }
            if data.started {
                return Err("The game has already started".into());
            }
            if data.members.len() < MIN_PLAYERS {
                return Err(format!("Need at least {MIN_PLAYERS} players"));
            }
            let configs: Vec<PlayerConfig> = data
                .members
                .iter()
                .map(|m| PlayerConfig {
                    name: m.name.clone(),
                    color: m.color,
                    is_bot: m.is_bot,
                })
                .collect();
            let seed = self.rng.lock().unwrap().next_u64();
            let mut game = GameState::new_lobby(configs, seed);
            game.start();
            data.game = Some(game);
            data.started = true;
            data.last_activity_ms = now_ms();
        }
        room.bump();
        self.persist(room);
        Ok(())
    }

    pub fn persist(&self, room: &Arc<Room>) {
        let snapshot = {
            let data = room.data.lock().unwrap();
            serde_json::to_vec(&*data)
        };
        let Ok(bytes) = snapshot else { return };
        let _ = std::fs::create_dir_all(&self.data_dir);
        let path = self.data_dir.join(format!("{}.json", room_code(room)));
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, &bytes).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }

    fn unique_code(&self) -> String {
        let rooms = self.rooms.read().unwrap();
        for _ in 0..100 {
            let code: String = {
                let mut rng = self.rng.lock().unwrap();
                (0..ROOM_CODE_LEN)
                    .map(|_| {
                        let i = rng.random_range(0..CODE_ALPHABET.len());
                        CODE_ALPHABET[i] as char
                    })
                    .collect()
            };
            if !rooms.contains_key(&code) {
                return code;
            }
        }
        format!("X{}", now_ms() % 1000)
    }
}

impl RoomData {
    /// The player id that owns `token` in the current game, if any.
    pub fn player_id_for(&self, token: &str) -> Option<usize> {
        self.member_index(token)
    }
}

pub fn room_code(room: &Arc<Room>) -> String {
    room.data.lock().unwrap().code.clone()
}

fn new_token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Trim and validate a player name: 1–5 visible characters, no control chars.
fn validate_name(name: &str) -> Result<String, JoinError> {
    let trimmed: String = name.trim().chars().filter(|c| !c.is_control()).collect();
    if trimmed.is_empty() {
        return Err(JoinError::NoName);
    }
    if trimmed.chars().count() > 5 {
        return Err(JoinError::NameTooLong);
    }
    Ok(trimmed)
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Arm/expire the per-turn timer for every room (called ~1x/second).
pub fn tick_turn_timers(app: &AppState) {
    use crate::game::state::Phase;
    let now = now_ms();
    let rooms: Vec<Arc<Room>> = app.rooms.read().unwrap().values().cloned().collect();
    for room in rooms {
        let mut changed = false;
        let mut ended = false;
        {
            let mut data = room.data.lock().unwrap();
            let secs = data.turn_seconds;
            let info = data.game.as_ref().map(|g| {
                (
                    matches!(g.phase, Phase::Play),
                    g.turn as u64 * 64 + g.current as u64,
                )
            });
            match info {
                Some((true, key)) if secs > 0 => {
                    if data.turn_armed_for != key {
                        data.turn_deadline_ms = now + secs * 1000;
                        data.turn_armed_for = key;
                        changed = true;
                    } else if now >= data.turn_deadline_ms {
                        if let Some(g) = data.game.as_mut() {
                            g.force_end_turn();
                        }
                        data.turn_deadline_ms = 0;
                        data.turn_armed_for = u64::MAX;
                        changed = true;
                        ended = true;
                    }
                }
                _ => {
                    if data.turn_armed_for != u64::MAX {
                        data.turn_deadline_ms = 0;
                        data.turn_armed_for = u64::MAX;
                        changed = true;
                    }
                }
            }
        }
        if changed {
            room.bump();
        }
        if ended {
            app.persist(&room);
        }
    }
}

/// Pick the bot that should act next in this room, if any.
///
/// Returns the player id and its difficulty. Handles every phase that needs
/// input: setup placement, normal play (including responses to a shared trade
/// offer), discards, and the robber flow.
fn bot_actor(data: &RoomData, game: &GameState) -> Option<(PlayerId, BotLevel)> {
    let level_of = |pid: PlayerId| -> Option<(PlayerId, BotLevel)> {
        data.members
            .get(pid)
            .filter(|m| m.is_bot)
            .map(|m| (pid, m.level))
    };
    match &game.phase {
        Phase::Lobby | Phase::GameOver => None,
        Phase::Setup => {
            let pid = *game.setup_queue.get(game.setup_pos)?;
            level_of(pid)
        }
        Phase::Play => {
            // A shared offer on the table takes priority: bots answer before
            // the turn continues.
            if let Some(t) = &game.trade {
                if let Some((pid, _)) = t
                    .responses
                    .iter()
                    .find(|(p, r)| *r == TradeResponse::Pending && data.members.get(*p).is_some_and(|m| m.is_bot))
                {
                    return level_of(*pid);
                }
            }
            level_of(game.current)
        }
        Phase::Discard => game
            .pending_discards
            .iter()
            .copied()
            .find(|p| data.members.get(*p).is_some_and(|m| m.is_bot))
            .and_then(level_of),
        Phase::MoveRobber { .. } | Phase::Steal { .. } => level_of(game.current),
    }
}

/// Delay before a bot's first action in a new decision context (turn, setup
/// placement, robber step, …) and between its subsequent actions in the same
/// one. Keeps bot turns watchable (roughly 10–20s) instead of instant.
const BOT_FIRST_DELAY_MS: u64 = 4500;
const BOT_STEP_DELAY_MS: u64 = 2500;

/// A value that changes only when the bot faces a genuinely new decision, so a
/// fresh "first action" delay is applied per turn/phase but not per sub-step.
fn bot_context_key(game: &GameState, pid: PlayerId) -> u64 {
    let phase = match game.phase {
        Phase::Lobby => 0u64,
        Phase::Setup => 1,
        Phase::Play => 2,
        Phase::Discard => 3,
        Phase::MoveRobber { .. } => 4,
        Phase::Steal { .. } => 5,
        Phase::GameOver => 6,
    };
    (pid as u64) << 40
        | (game.turn as u64) << 16
        | (phase << 8)
        | (game.setup_pos as u64 & 0xff)
}

/// Advance every room by at most one paced bot action. Acting one step per
/// tick keeps the pace watchable and avoids holding a room lock across
/// multiple mutations.
pub fn tick_bots(app: &AppState) {
    let now = now_ms();
    let rooms: Vec<Arc<Room>> = app.rooms.read().unwrap().values().cloned().collect();
    for room in rooms {
        let planned = {
            let mut data = room.data.lock().unwrap();
            let actor = match data.game.as_ref() {
                Some(g) if !g.is_over() => bot_actor(&data, g),
                _ => None,
            };
            let Some((pid, level)) = actor else {
                continue;
            };
            let ctx = {
                let g = data.game.as_ref().unwrap();
                bot_context_key(g, pid)
            };
            if data.bot_key != ctx {
                data.bot_key = ctx;
                data.bot_next_ms = now + BOT_FIRST_DELAY_MS;
                continue;
            }
            if now < data.bot_next_ms {
                continue;
            }
            let action = {
                let g = data.game.as_ref().unwrap();
                bot::choose_action(g, pid, level)
            };
            data.bot_next_ms = now + BOT_STEP_DELAY_MS;
            Some((pid, action))
        };
        let Some((pid, action)) = planned else {
            continue;
        };
        let applied = {
            let mut data = room.data.lock().unwrap();
            let Some(game) = data.game.as_mut() else {
                continue;
            };
            match game.apply(pid, &action) {
                Ok(()) => true,
                // Defensive: never let a bot wedge a room. Fall back to a
                // guaranteed-legal action for the phase if the policy errored.
                Err(_) => bot::fallback_action(game, pid)
                    .map(|a| game.apply(pid, &a).is_ok())
                    .unwrap_or(false),
            }
        };
        if applied {
            let mut data = room.data.lock().unwrap();
            data.last_activity_ms = now_ms();
            if let Some(m) = data.members.get_mut(pid) {
                m.last_seen_ms = now_ms();
            }
            drop(data);
            room.bump();
            app.persist(&room);
        }
    }
}

/// Remove stale rooms from memory and disk (called periodically).
pub fn sweep(app: &AppState, max_idle_ms: u64) {
    let now = now_ms();
    let stale: Vec<String> = {
        let rooms = app.rooms.read().unwrap();
        rooms
            .iter()
            .filter(|(_, r)| {
                let data = r.data.lock().unwrap();
                now.saturating_sub(data.last_activity_ms) > max_idle_ms
            })
            .map(|(code, _)| code.clone())
            .collect()
    };
    if stale.is_empty() {
        return;
    }
    let mut rooms = app.rooms.write().unwrap();
    for code in stale {
        rooms.remove(&code);
        let _ = std::fs::remove_file(app.data_dir.join(format!("{code}.json")));
    }
}

pub fn data_dir_default() -> PathBuf {
    std::env::var("CATAN_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("data"))
}

pub fn ensure_dir(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
}
