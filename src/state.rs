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

/// Hard cap on a single pause; the room auto-resumes after this long so one
/// player cannot stall a table indefinitely.
pub const PAUSE_CAP_MS: u64 = 30 * 60 * 1000;

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
    /// The seat was emptied while the game was running (leave/remove). The
    /// member stays so every other player keeps their seat number, but its
    /// session token is dead, so that person can no longer act.
    #[serde(default)]
    pub left: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RoomData {
    pub code: String,
    pub host_token: String,
    pub members: Vec<Member>,
    pub game: Option<GameState>,
    /// The post-`start()` board state, kept as the replay baseline. The live
    /// `game` only holds the current state plus the move log; re-applying
    /// `game.moves` onto this snapshot rebuilds any earlier position.
    #[serde(default)]
    pub initial_game: Option<GameState>,
    pub started: bool,
    pub created_ms: u64,
    pub last_activity_ms: u64,
    /// Per-turn time limit in seconds (0 = untimed), chosen at creation.
    #[serde(default)]
    pub turn_seconds: u64,
    /// Whether this room allows pausing (chosen at creation; default off).
    #[serde(default)]
    pub allow_pause: bool,
    /// Whole-room pause in effect: freezes the turn timer and bots, and blinds
    /// every viewer server-side (no board, hand, bank, log until resumed).
    #[serde(default)]
    pub paused: bool,
    /// Seat that began the current pause, for the paused screen + audit log.
    #[serde(default)]
    pub paused_by: Option<usize>,
    /// When the current pause began (unix-ms).
    #[serde(default)]
    pub paused_at_ms: u64,
    /// Auto-resume deadline for the current pause (0 = none).
    #[serde(default)]
    pub pause_deadline_ms: u64,
    /// Frozen remaining turn time to restore on resume. Persisted because
    /// `turn_deadline_ms` is `#[serde(skip)]`, so a restart mid-pause keeps the
    /// clock correctly stopped.
    #[serde(default)]
    pub pause_remaining_ms: u64,
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

    /// Pausing is only meaningful during a live game (there is no clock to
    /// freeze in the lobby or once the game is over).
    pub fn can_pause_now(&self) -> bool {
        self.allow_pause
            && self.started
            && self.game.as_ref().is_some_and(|g| {
                matches!(
                    g.phase,
                    Phase::Setup
                        | Phase::Play
                        | Phase::Discard
                        | Phase::MoveRobber { .. }
                        | Phase::Steal { .. }
                )
            })
    }

    /// Whether `v`'s view must be blinded because the room is paused. Applies to
    /// seated humans only; bots never render a view.
    pub fn viewer_blinded(&self, v: usize) -> bool {
        self.paused && self.members.get(v).is_some_and(|m| !m.is_bot)
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
            left: false,
        });
        true
    }
}

/// What happened to a seat that was emptied.
#[derive(Debug, PartialEq, Eq)]
pub enum SeatOutcome {
    /// The slot was freed; the room is still waiting to start.
    Lobby { name: String },
    /// The game was running, so it ended for everyone in the room.
    Ended { name: String },
    /// The seat was handed to a bot and the game carries on.
    Continued { name: String },
    /// The last human left the lobby, so there is nobody left to play with.
    /// The caller must drop the room from the registry.
    Closed { name: String },
}

/// How a human chose to leave a live game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LeaveChoice {
    /// End the game for everyone (the existing abandoned path).
    #[default]
    End,
    /// Leave the seat playable: a Medium bot keeps everything and continues.
    Bot,
}

impl RoomData {
    /// Empty the seat at `idx`.
    ///
    /// In the lobby the member is dropped, which frees the slot. Once the game
    /// is running the member list must stay exactly as it is — member order is
    /// the player order, so removing an entry would re-index every other
    /// player's pieces — so instead the game ends for everybody and the seat is
    /// flagged `left` with a rotated token, killing the old session cookie.
    fn empty_seat(&mut self, idx: usize, why: impl Into<String>) -> SeatOutcome {
        let name = self.members[idx].name.clone();
        if let Some(game) = self.game.as_mut() {
            game.abandon(why.into());
            let seat = &mut self.members[idx];
            seat.left = true;
            seat.token = new_token();
            seat.connected = false;
            SeatOutcome::Ended { name }
        } else {
            self.members.remove(idx);
            SeatOutcome::Lobby { name }
        }
    }

    /// The host removes any other seat (`target` is the member index, which is
    /// the player id once the game starts). This keeps the original behaviour:
    /// mid-game the game ends for everyone.
    pub fn remove_player(&mut self, host_token: &str, target: usize) -> Result<SeatOutcome, String> {
        self.remove_player_with(host_token, target, LeaveChoice::End)
    }

    /// The host removes another seat, choosing what happens mid-game:
    /// `End` abandons the game, `Bot` hands the seat to a Medium bot and keeps
    /// the game running. `Bot` mirrors the leave rule: a takeover is refused
    /// when it would leave fewer than two humans (the host is staying, so the
    /// kicked seat's game must still have somebody to play against), and it is
    /// never offered for a seat that is already a bot.
    pub fn remove_player_with(
        &mut self,
        host_token: &str,
        target: usize,
        choice: LeaveChoice,
    ) -> Result<SeatOutcome, String> {
        if !self.is_host(host_token) {
            return Err("Only the host can remove players.".into());
        }
        let host_idx = self.member_index(host_token).unwrap_or(usize::MAX);
        if target == host_idx {
            return Err("You cannot remove yourself.".into());
        }
        let Some(member) = self.members.get(target) else {
            return Err("That player is not in this room.".into());
        };
        if self.game.is_some() && choice == LeaveChoice::Bot {
            if member.is_bot {
                return Err("That seat is already a bot.".into());
            }
            let humans_after = self
                .members
                .iter()
                .enumerate()
                .filter(|(idx, m)| *idx != target && !m.is_bot)
                .count();
            if humans_after < 2 {
                return Err(
                    "Passing this seat to a bot would leave only one human. End the game instead."
                        .into(),
                );
            }
            return Ok(self.bot_takeover(target, "was removed by the host"));
        }
        let name = member.name.clone();
        Ok(self.empty_seat(target, format!("{name} was removed by the host.")))
    }

    /// Token of the human who should take the host badge. An explicit pick wins
    /// as long as it is a real human other than the leaver; otherwise the
    /// longest-present remaining human (first in join order) is used.
    fn next_host_token(&self, leaver: usize, requested: Option<usize>) -> Option<String> {
        let humans = || {
            self.members
                .iter()
                .enumerate()
                .filter(|(idx, m)| *idx != leaver && !m.is_bot)
        };
        let picked = requested
            .filter(|idx| humans().any(|(i, _)| i == *idx))
            .or_else(|| humans().next().map(|(i, _)| i));
        picked.map(|idx| self.members[idx].token.clone())
    }

    /// Turn an occupied seat into a Medium bot without touching its game state.
    /// The seat's session token is rotated so the old cookie is dead; the game
    /// keeps running and the bot starts taking turns normally. `why` completes
    /// "<name> ___ — a bot takes over." (left / was removed by the host).
    fn bot_takeover(&mut self, idx: usize, why: &str) -> SeatOutcome {
        let name = self.members[idx].name.clone();
        if let Some(game) = self.game.as_mut() {
            game.players[idx].is_bot = true;
            game.push_log(None, format!("{name} {why} — a bot takes over."));
        }
        let seat = &mut self.members[idx];
        seat.is_bot = true;
        seat.level = BotLevel::Medium;
        seat.connected = false;
        seat.left = false;
        seat.token = new_token();
        SeatOutcome::Continued { name }
    }

    /// A player leaves the room with the default choice: mid-game the game ends
    /// for everyone, and in the lobby the next host is auto-picked.
    pub fn leave(&mut self, token: &str) -> Result<SeatOutcome, String> {
        self.leave_with(token, LeaveChoice::End, None)
    }

    /// A player leaves the room with an explicit choice.
    ///
    /// In the lobby the host seat is handed to `next_host` (default: the
    /// longest-present remaining human); if no human is left the room closes.
    /// Mid-game `LeaveChoice::End` keeps the abandoned path, while
    /// `LeaveChoice::Bot` hands the seat to a Medium bot and keeps the game
    /// running. A host taking the bot path transfers the badge first.
    pub fn leave_with(
        &mut self,
        token: &str,
        choice: LeaveChoice,
        next_host: Option<usize>,
    ) -> Result<SeatOutcome, String> {
        let Some(idx) = self.member_index(token) else {
            return Err("You are not a member of this room.".into());
        };
        let name = self.members[idx].name.clone();
        let was_host = self.is_host(token);
        let successor = self.next_host_token(idx, next_host);

        if self.game.is_some() {
            return match choice {
                LeaveChoice::End => {
                    let why = if was_host {
                        "Host left the game.".to_string()
                    } else {
                        format!("{name} left the game.")
                    };
                    Ok(self.empty_seat(idx, why))
                }
                LeaveChoice::Bot => {
                    let Some(successor) = successor else {
                        return Err(
                            "Only End the game is available when no other humans remain.".into(),
                        );
                    };
                    if was_host {
                        self.host_token = successor;
                    }
                    Ok(self.bot_takeover(idx, "left"))
                }
            };
        }

        let outcome = self.empty_seat(idx, format!("{name} left the game."));
        if was_host {
            match successor {
                Some(successor) => self.host_token = successor,
                None => return Ok(SeatOutcome::Closed { name }),
            }
        }
        Ok(outcome)
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
        allow_pause: bool,
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
            left: false,
            }],
            game: None,
            initial_game: None,
            started: false,
            created_ms: now_ms(),
            last_activity_ms: now_ms(),
            turn_seconds,
            allow_pause,
            paused: false,
            paused_by: None,
            paused_at_ms: 0,
            pause_deadline_ms: 0,
            pause_remaining_ms: 0,
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
            // resume after closing the tab. A seat emptied mid-game (left or
            // removed) is dead and must not be resumed.
            if let Some(seat) = data
                .members
                .iter()
                .position(|m| {
                    !m.is_bot && !m.left && m.name.to_lowercase() == name.to_lowercase()
                })
            {
                let token = new_token();
                // Rejoining hands back a fresh token for the same seat. If that
                // seat was the host, rotate `host_token` in lockstep — `is_host`
                // compares the two, so otherwise the rejoined host is locked out
                // of host-only actions such as starting the game.
                if data.host_token == data.members[seat].token {
                    data.host_token = token.clone();
                }
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
            left: false,
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
            // Keep the post-start board as the immutable replay baseline.
            data.initial_game = Some(game.clone());
            data.game = Some(game);
            data.started = true;
            data.last_activity_ms = now_ms();
        }
        room.bump();
        self.persist(room);
        Ok(())
    }

    /// Pause the whole room. Any seated human may do it; idempotent.
    pub fn pause_room(&self, room: &Arc<Room>, token: &str) -> Result<(), String> {
        let changed = {
            let mut data = room.data.lock().unwrap();
            let Some(v) = data.member_index(token) else {
                return Err("You are not a member of this game.".into());
            };
            if data.members[v].is_bot {
                return Err("Bots cannot pause.".into());
            }
            if !data.allow_pause {
                return Err("Pausing is not enabled for this room.".into());
            }
            if data.paused {
                return Ok(());
            }
            if !data.can_pause_now() {
                return Err("You can only pause during a live game.".into());
            }
            begin_pause(&mut data, v)
        };
        if changed {
            room.bump();
            self.persist(room);
        }
        Ok(())
    }

    /// Resume (or cancel) the room pause. Any seated human may do it; idempotent.
    pub fn resume_room(&self, room: &Arc<Room>, token: &str) -> Result<(), String> {
        let changed = {
            let mut data = room.data.lock().unwrap();
            let Some(v) = data.member_index(token) else {
                return Err("You are not a member of this game.".into());
            };
            if data.members[v].is_bot {
                return Err("Bots cannot resume.".into());
            }
            if !data.paused {
                return Ok(());
            }
            resume_data(&mut data, false)
        };
        if changed {
            room.bump();
            self.persist(room);
        }
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

    /// Drop a lobby room whose last human has left. Re-checks under the room
    /// lock so a join that raced the leave keeps the room alive. If such a join
    /// stranded the room without a host (the leaver held the badge), the badge
    /// is handed to the longest-present human instead. Returns whether the room
    /// was actually removed.
    pub fn close_room_if_empty(&self, code: &str) -> bool {
        let mut rooms = self.rooms.write().unwrap();
        let Some(room) = rooms.get(code).cloned() else {
            return false;
        };
        let mut repaired = false;
        {
            let mut data = room.data.lock().unwrap();
            let has_human = data.members.iter().any(|m| !m.is_bot);
            if data.game.is_some() || has_human {
                if has_human
                    && !data.members.iter().any(|m| m.token == data.host_token)
                    && let Some(next) = data.members.iter().position(|m| !m.is_bot)
                {
                    data.host_token = data.members[next].token.clone();
                    repaired = true;
                }
            } else {
                drop(data);
                rooms.remove(code);
                drop(rooms);
                let _ = std::fs::remove_file(self.data_dir.join(format!("{code}.json")));
                return true;
            }
        }
        drop(rooms);
        if repaired {
            room.bump();
            self.persist(&room);
        }
        false
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

/// Enter the paused state for `data`, freezing the remaining turn time.
fn begin_pause(data: &mut RoomData, v: usize) -> bool {
    let now = now_ms();
    data.paused = true;
    data.paused_by = Some(v);
    data.paused_at_ms = now;
    data.pause_deadline_ms = now + PAUSE_CAP_MS;
    data.pause_remaining_ms = data.turn_deadline_ms.saturating_sub(now);
    data.last_activity_ms = now;
    let name = data
        .members
        .get(v)
        .map(|m| m.name.clone())
        .unwrap_or_else(|| "A player".into());
    if let Some(g) = data.game.as_mut() {
        g.push_log(Some(v), format!("{name} paused the game."));
    }
    true
}

/// Leave the paused state, restoring the frozen turn time. `auto` marks the
/// 30-minute cap firing rather than a player pressing Resume. Returns whether
/// anything changed.
fn resume_data(data: &mut RoomData, auto: bool) -> bool {
    if !data.paused {
        return false;
    }
    let now = now_ms();
    let by = data.paused_by;
    data.paused = false;
    data.paused_by = None;
    if data.pause_remaining_ms > 0 {
        // Re-arm the timer so tick_turn_timers uses the frozen remainder rather
        // than a fresh turn. turn_armed_for is unchanged, so the tick sees the
        // deadline already armed for this turn.
        data.turn_deadline_ms = now + data.pause_remaining_ms;
    }
    data.pause_remaining_ms = 0;
    data.pause_deadline_ms = 0;
    data.paused_at_ms = 0;
    data.last_activity_ms = now;
    let text = if auto {
        "The game auto-resumed after the pause time limit.".to_string()
    } else {
        let name = by
            .and_then(|p| data.members.get(p))
            .map(|m| m.name.clone())
            .unwrap_or_else(|| "A player".into());
        format!("{name} resumed the game.")
    };
    if let Some(g) = data.game.as_mut() {
        g.push_log(by, text);
    }
    true
}

/// Arm/expire the per-turn timer for every room (called ~1x/second).
pub fn tick_turn_timers(app: &AppState) {
    use crate::game::state::Phase;
    let now = now_ms();
    let rooms: Vec<Arc<Room>> = app.rooms.read().unwrap().values().cloned().collect();
    for room in rooms {
        let mut changed = false;
        let mut ended = false;
        let mut resumed = false;
        {
            let mut data = room.data.lock().unwrap();
            if data.paused {
                // Freeze the turn timer entirely; only the auto-resume cap can
                // move a paused room forward.
                if data.pause_deadline_ms > 0 && now >= data.pause_deadline_ms {
                    resumed = resume_data(&mut data, true);
                    changed = resumed;
                }
            } else {
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
        }
        if changed {
            room.bump();
        }
        if ended || resumed {
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
            // A paused room does not advance: bots wait with everyone else.
            if data.paused {
                continue;
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn member(is_bot: bool) -> Member {
        Member {
            token: "t".into(),
            name: "Al".into(),
            color: Color::Red,
            is_bot,
            level: BotLevel::default(),
            connected: true,
            last_seen_ms: 0,
            mode: ViewMode::Normal,
            layout: Layout::default(),
            left: false,
        }
    }

    fn room(paused: bool, is_bot: bool) -> RoomData {
        RoomData {
            code: "ABCD".into(),
            host_token: "t".into(),
            members: vec![member(is_bot)],
            game: None,
            initial_game: None,
            started: true,
            created_ms: 0,
            last_activity_ms: 0,
            turn_seconds: 180,
            allow_pause: true,
            paused,
            paused_by: if paused { Some(0) } else { None },
            paused_at_ms: 0,
            pause_deadline_ms: 0,
            pause_remaining_ms: 0,
            turn_deadline_ms: 0,
            turn_armed_for: u64::MAX,
            bot_next_ms: 0,
            bot_key: u64::MAX,
        }
    }

    fn test_app(tag: &str) -> Arc<AppState> {
        let dir = std::env::temp_dir().join(format!("catan-state-test-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        AppState::new(dir)
    }

    #[test]
    fn same_name_rejoin_keeps_host_privileges() {
        let app = test_app("rejoin-host");
        let (room, host_token) = app.create_room("Al".into(), 0, false).unwrap();
        let code = room.data.lock().unwrap().code.clone();
        // A second human so the lobby can actually start.
        app.join(&code, "Bo".into()).unwrap();

        // Rejoin the host by name; the seat token is rotated.
        let (_, fresh) = app.join(&code, "Al".into()).unwrap();
        assert_ne!(fresh, host_token, "rejoin must rotate the seat token");

        {
            let data = room.data.lock().unwrap();
            assert!(data.is_host(&fresh), "the rejoined host must remain host");
            assert!(!data.is_host(&host_token), "the stale token must lose host");
            assert!(data.can_start(), "the lobby is still startable");
        }
        assert!(
            app.start_game(&room, &fresh).is_ok(),
            "the rejoined host must be able to start the game"
        );

        let _ = std::fs::remove_dir_all(app.data_dir.clone());
    }

    #[test]
    fn blinded_only_when_paused_and_human() {
        assert!(!room(false, false).viewer_blinded(0));
        assert!(room(true, false).viewer_blinded(0));
        assert!(!room(true, true).viewer_blinded(0));
        // Unknown seat is never blinded.
        assert!(!room(true, false).viewer_blinded(9));
    }

    #[test]
    fn pause_freezes_and_resume_restores_turn_time() {
        let mut data = room(false, false);
        let deadline = now_ms() + 4_000;
        data.turn_deadline_ms = deadline;
        data.turn_armed_for = 0;

        assert!(begin_pause(&mut data, 0));
        assert!(data.paused);
        assert!(data.pause_remaining_ms >= 3_000 && data.pause_remaining_ms <= 4_000);
        assert!(data.pause_deadline_ms > now_ms());

        // Wait long enough that the original deadline would have passed.
        std::thread::sleep(std::time::Duration::from_millis(60));
        let restore_at = now_ms();
        assert!(resume_data(&mut data, false));
        assert!(!data.paused);
        assert!(data.paused_by.is_none());
        assert!(
            data.turn_deadline_ms >= restore_at + 3_000,
            "resume must restore the frozen remainder"
        );
    }

    #[test]
    fn pause_resume_are_idempotent() {
        let mut data = room(true, false);
        assert!(resume_data(&mut data, false));
        assert!(!resume_data(&mut data, false));
        assert!(!resume_data(&mut data, true));
    }

    #[test]
    fn can_pause_needs_option_flag() {
        let mut data = room(false, false);
        data.game = Some(GameState::new_lobby(
            vec![PlayerConfig {
                name: "Al".into(),
                color: Color::Red,
                is_bot: false,
            }],
            1,
        ));
        // Still in Lobby phase.
        assert!(!data.can_pause_now());
        data.game.as_mut().unwrap().start();
        assert!(data.can_pause_now());
        data.allow_pause = false;
        assert!(!data.can_pause_now());
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("catan-omk64-{tag}-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn app(tag: &str) -> (Arc<AppState>, PathBuf) {
        let dir = scratch(tag);
        (AppState::new(dir.clone()), dir)
    }

    /// Alice creates a room and Bob joins it.
    fn two_players(tag: &str) -> (Arc<AppState>, Arc<Room>, String, String, PathBuf) {
        let (app, dir) = app(tag);
        let (room, alice) = app.create_room("Alice".into(), 180, true).unwrap();
        let code = room_code(&room);
        let (_, bob) = app.join(&code, "Bob".into()).unwrap();
        (app, room, alice, bob, dir)
    }

    /// Alice creates a room and Bob and Carol join it.
    fn three_players(tag: &str) -> (Arc<AppState>, Arc<Room>, String, String, PathBuf) {
        let (app, dir) = app(tag);
        let (room, alice) = app.create_room("Alice".into(), 180, true).unwrap();
        let code = room_code(&room);
        let (_, bob) = app.join(&code, "Bob".into()).unwrap();
        let (_, carol) = app.join(&code, "Carol".into()).unwrap();
        // `carol` is only here to keep three humans; the callers pick seats by
        // token/name so the unused binding is deliberate.
        let _ = &carol;
        (app, room, alice, bob, dir)
    }

    #[test]
    fn host_remove_in_lobby_frees_the_slot() {
        let (app, room, alice, bob, dir) = two_players("remove-lobby");
        let outcome = room.data.lock().unwrap().remove_player(&alice, 1);
        assert_eq!(outcome, Ok(SeatOutcome::Lobby { name: "Bob".into() }));
        let data = room.data.lock().unwrap();
        assert_eq!(data.members.len(), 1, "the freed slot leaves only the host");
        assert!(
            data.member_index(&bob).is_none(),
            "the removed player is gone"
        );
        assert!(data.member_index(&alice).is_some(), "the host stays");
        assert!(!data.started);
        let _ = app;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn non_host_cannot_remove() {
        let (app, room, alice, bob, dir) = two_players("remove-denied");
        let err = room.data.lock().unwrap().remove_player(&bob, 0);
        assert_eq!(err, Err("Only the host can remove players.".into()));
        let data = room.data.lock().unwrap();
        assert_eq!(data.members.len(), 2, "nobody was removed");
        assert!(data.member_index(&alice).is_some());
        assert!(data.member_index(&bob).is_some());
        let _ = app;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn host_cannot_remove_themselves_or_a_missing_seat() {
        let (_app, room, alice, _bob, dir) = two_players("remove-self");
        let mut data = room.data.lock().unwrap();
        assert_eq!(
            data.remove_player(&alice, 0),
            Err("You cannot remove yourself.".into())
        );
        assert_eq!(
            data.remove_player(&alice, 9),
            Err("That player is not in this room.".into())
        );
        assert_eq!(data.members.len(), 2);
        drop(data);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn leave_in_lobby_frees_the_slot() {
        let (app, room, alice, bob, dir) = two_players("leave-lobby");
        let outcome = room.data.lock().unwrap().leave(&bob);
        assert_eq!(outcome, Ok(SeatOutcome::Lobby { name: "Bob".into() }));
        let data = room.data.lock().unwrap();
        assert_eq!(data.members.len(), 1);
        assert!(
            data.member_index(&bob).is_none(),
            "the leaver's seat is free"
        );
        assert!(data.member_index(&alice).is_some());
        let _ = app;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn host_leave_in_lobby_transfers_host_to_the_longest_present_human() {
        let (app, room, alice, bob, dir) = two_players("host-leave");
        let code = room_code(&room);
        // A bot joins before Carol, so the transfer must skip non-humans.
        room.data.lock().unwrap().add_bot(BotLevel::default());
        let (_, carol) = app.join(&code, "Carol".into()).unwrap();

        let outcome = room.data.lock().unwrap().leave(&alice);
        assert_eq!(outcome, Ok(SeatOutcome::Lobby { name: "Alice".into() }));

        let data = room.data.lock().unwrap();
        assert_eq!(data.members.len(), 3, "the host seat is freed, the rest stay");
        assert!(data.member_index(&alice).is_none());
        assert_eq!(data.host_token, bob, "the longest-present human becomes host");
        assert!(data.is_host(&bob));
        assert!(!data.is_host(&carol));
        assert!(
            data.member_index(&bob) < data.member_index(&carol),
            "join order still decides who is longest-present"
        );
        drop(data);
        let _ = app;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn host_leave_in_lobby_honours_an_explicit_next_host() {
        let (app, room, alice, bob, dir) = two_players("host-leave-picked");
        let code = room_code(&room);
        let (_, carol) = app.join(&code, "Carol".into()).unwrap();

        let (outcome, bob_idx, carol_idx) = {
            let mut data = room.data.lock().unwrap();
            let bob_idx = data.member_index(&bob).unwrap();
            let carol_idx = data.member_index(&carol).unwrap();
            let outcome = data.leave_with(&alice, LeaveChoice::End, Some(carol_idx));
            (outcome, bob_idx, carol_idx)
        };
        assert_eq!(outcome, Ok(SeatOutcome::Lobby { name: "Alice".into() }));

        let data = room.data.lock().unwrap();
        assert_eq!(
            data.host_token, carol,
            "the host's explicit pick becomes the next host"
        );
        assert!(data.is_host(&carol));
        assert!(
            !data.is_host(&bob),
            "the longest-present human is only the default, not the rule"
        );
        assert!(carol_idx > bob_idx, "the test picked the later human");
        drop(data);
        let _ = app;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn host_leave_in_lobby_with_only_bots_closes_the_room() {
        let (app, room, alice, bob, dir) = two_players("host-leave-bots");
        let code = room_code(&room);
        {
            let mut data = room.data.lock().unwrap();
            // Bob leaves and is replaced by a bot, so Alice is the last human.
            assert_eq!(
                data.leave(&bob),
                Ok(SeatOutcome::Lobby { name: "Bob".into() })
            );
            data.add_bot(BotLevel::default());
            assert_eq!(
                data.leave(&alice),
                Ok(SeatOutcome::Closed { name: "Alice".into() })
            );
            assert_eq!(data.human_count(), 0);
            assert!(data.members.iter().all(|m| m.is_bot));
        }
        assert!(app.close_room_if_empty(&code), "the bot-only room is dropped");
        assert!(app.room(&code).is_none(), "the room code is gone");
        assert!(
            !dir.join(format!("{code}.json")).exists(),
            "the saved room is deleted"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn human_join_while_the_last_host_leaves_keeps_the_room_with_a_host() {
        let (app, room, alice, bob, dir) = two_players("host-leave-race");
        let code = room_code(&room);
        {
            let mut data = room.data.lock().unwrap();
            data.leave(&bob).unwrap();
            data.add_bot(BotLevel::default());
            assert_eq!(
                data.leave(&alice),
                Ok(SeatOutcome::Closed { name: "Alice".into() })
            );
        }
        // A join lands between the leave and the registry teardown.
        let (_, dave) = app.join(&code, "Dave".into()).unwrap();
        assert!(
            !app.close_room_if_empty(&code),
            "a real human keeps the room open"
        );
        assert!(app.room(&code).is_some(), "the room code still works");
        let data = room.data.lock().unwrap();
        assert!(data.is_host(&dave), "the new human gets the host badge");
        assert_eq!(data.members.iter().filter(|m| !m.is_bot).count(), 1);
        drop(data);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn host_leave_midgame_ends_the_game_for_everyone() {
        let (app, room, alice, bob, dir) = two_players("host-leave-mid");
        app.start_game(&room, &alice).unwrap();
        let host_seat = room.data.lock().unwrap().member_index(&alice).unwrap();

        let outcome = room.data.lock().unwrap().leave(&alice);
        assert_eq!(outcome, Ok(SeatOutcome::Ended { name: "Alice".into() }));

        let data = room.data.lock().unwrap();
        let game = data.game.as_ref().expect("the game object stays");
        assert!(game.is_over(), "the game is over for everyone");
        assert_eq!(game.winner, None, "nobody wins when the host walks out");
        assert!(
            game.log
                .iter()
                .any(|l| l.text.contains("Host left the game.")),
            "the shared log records that the host left"
        );
        // Seats keep their indices, but the host's old session is dead.
        assert_eq!(data.members.len(), 2, "seats are not re-indexed");
        assert!(data.members[host_seat].left, "the host seat is emptied");
        assert!(
            data.member_index(&alice).is_none(),
            "the old host token no longer works"
        );
        assert!(data.member_index(&bob).is_some(), "Bob keeps his seat");
        assert!(!data.is_host(&bob), "the host badge does not transfer midgame");
        drop(data);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn leave_midgame_ends_the_game_for_everyone() {
        let (app, room, alice, bob, dir) = two_players("leave-mid");
        app.start_game(&room, &alice).unwrap();
        let bob_seat = room.data.lock().unwrap().member_index(&bob).unwrap();

        let outcome = room.data.lock().unwrap().leave(&bob);
        assert_eq!(outcome, Ok(SeatOutcome::Ended { name: "Bob".into() }));

        let data = room.data.lock().unwrap();
        let game = data.game.as_ref().expect("the game object stays");
        assert!(game.is_over(), "the game is over for everyone");
        assert_eq!(game.winner, None, "nobody wins when a player walks out");
        assert!(
            game.log
                .iter()
                .any(|l| l.text.contains("Bob left the game.")),
            "the shared log records why it ended"
        );
        // Seats keep their indices, so the surviving players still map to
        // their own pieces...
        assert_eq!(data.members.len(), 2, "seats are not re-indexed");
        assert!(data.member_index(&alice).is_some());
        assert_eq!(data.member_index(&alice).unwrap(), 0);
        // ...but the leaver's old session is dead.
        assert!(
            data.member_index(&bob).is_none(),
            "the old token no longer works"
        );
        assert!(
            data.members[bob_seat].left,
            "the seat is flagged as emptied"
        );
        assert!(!data.members[bob_seat].is_bot);
        drop(data);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn host_remove_midgame_ends_the_game_for_everyone() {
        let (app, room, alice, bob, dir) = two_players("remove-mid");
        app.start_game(&room, &alice).unwrap();

        let outcome = room.data.lock().unwrap().remove_player(&alice, 1);
        assert_eq!(outcome, Ok(SeatOutcome::Ended { name: "Bob".into() }));

        let data = room.data.lock().unwrap();
        assert!(data.game.as_ref().unwrap().is_over());
        assert_eq!(data.game.as_ref().unwrap().winner, None);
        assert!(
            data.game
                .as_ref()
                .unwrap()
                .log
                .iter()
                .any(|l| l.text.contains("Bob was removed by the host.")),
            "the shared log records who removed whom"
        );
        assert!(data.member_index(&bob).is_none());
        assert!(data.member_index(&alice).is_some());
        drop(data);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn host_remove_with_bot_takeover_keeps_the_game_running() {
        let (app, room, alice, bob, dir) = three_players("remove-mid-bot");
        app.start_game(&room, &alice).unwrap();
        let bob_seat = room.data.lock().unwrap().member_index(&bob).unwrap();
        let (bob_token_before, vp_before) = {
            let mut data = room.data.lock().unwrap();
            let token = data.members[bob_seat].token.clone();
            let game = data.game.as_mut().unwrap();
            game.players[bob_seat]
                .resources
                .add(crate::game::resources::Resource::Wood, 2);
            (token, game.total_victory_points(bob_seat))
        };

        let outcome = room
            .data
            .lock()
            .unwrap()
            .remove_player_with(&alice, bob_seat, LeaveChoice::Bot);
        assert_eq!(outcome, Ok(SeatOutcome::Continued { name: "Bob".into() }));

        let data = room.data.lock().unwrap();
        let game = data.game.as_ref().unwrap();
        assert!(!game.is_over(), "the game keeps running for everyone else");
        let seat = &data.members[bob_seat];
        assert!(seat.is_bot, "the kicked seat becomes a bot");
        assert_eq!(seat.level, BotLevel::Medium, "takeover uses a Medium bot");
        assert!(!seat.left, "the seat is live, not emptied");
        assert!(
            game.players[bob_seat].is_bot,
            "the engine also treats the seat as a bot (AI turns)"
        );
        assert_eq!(
            game.total_victory_points(bob_seat),
            vp_before,
            "the bot keeps the kicked player's game state"
        );
        assert_ne!(seat.token, bob_token_before, "the seat token is rotated");
        assert!(
            data.member_index(&bob).is_none(),
            "the kicked player's old session is dead"
        );
        assert_eq!(
            data.host_token, alice,
            "the host badge stays with the host who kicked"
        );
        assert!(data.is_host(&alice));
        assert!(
            !data.members[2].is_bot && data.member_index(&data.members[2].token).is_some(),
            "the other human keeps their seat"
        );
        assert!(
            game.log
                .iter()
                .any(|l| l.text.contains("Bob was removed by the host — a bot takes over.")),
            "the log records the takeover and who caused it"
        );
        drop(data);
        let _ = app;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn host_remove_with_bot_takeover_rejected_when_only_one_human_would_remain() {
        let (app, room, alice, bob, dir) = two_players("remove-mid-bot-alone");
        app.start_game(&room, &alice).unwrap();
        let bob_seat = room.data.lock().unwrap().member_index(&bob).unwrap();

        let err = room
            .data
            .lock()
            .unwrap()
            .remove_player_with(&alice, bob_seat, LeaveChoice::Bot);
        assert_eq!(
            err,
            Err(
                "Passing this seat to a bot would leave only one human. End the game instead."
                    .into()
            )
        );
        {
            let data = room.data.lock().unwrap();
            let game = data.game.as_ref().unwrap();
            assert!(!game.is_over(), "a refused kick leaves the game intact");
            assert!(!data.members[bob_seat].is_bot, "Bob stays human");
            assert!(data.member_index(&bob).is_some());
            assert!(data.is_host(&alice));
        }

        // The same choice with End still works.
        let ended = room.data.lock().unwrap().remove_player(&alice, bob_seat);
        assert_eq!(ended, Ok(SeatOutcome::Ended { name: "Bob".into() }));
        let data = room.data.lock().unwrap();
        assert!(data.game.as_ref().unwrap().is_over());
        assert!(data.members[bob_seat].left);
        assert!(data.member_index(&bob).is_none(), "the kicked token is dead");
        drop(data);
        let _ = app;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn host_cannot_pass_a_bot_seat_to_a_bot() {
        let (app, room, alice, bob, dir) = two_players("remove-bot-seat");
        room.data.lock().unwrap().add_bot(BotLevel::Medium);
        app.start_game(&room, &alice).unwrap();
        let bot_seat = 2;
        assert!(room.data.lock().unwrap().members[bot_seat].is_bot);

        let err = room
            .data
            .lock()
            .unwrap()
            .remove_player_with(&alice, bot_seat, LeaveChoice::Bot);
        assert_eq!(err, Err("That seat is already a bot.".into()));
        assert!(!room.data.lock().unwrap().game.as_ref().unwrap().is_over());
        let _ = (bob, app);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn leave_midgame_with_bot_takeover_keeps_the_seat_and_the_game() {
        let (app, room, alice, bob, dir) = two_players("leave-mid-bot");
        app.start_game(&room, &alice).unwrap();
        let bob_seat = room.data.lock().unwrap().member_index(&bob).unwrap();
        let before = {
            let mut data = room.data.lock().unwrap();
            let game = data.game.as_mut().unwrap();
            game.players[bob_seat]
                .resources
                .add(crate::game::resources::Resource::Wood, 3);
            game.players[bob_seat]
                .dev_cards
                .push(crate::game::resources::DevCard::Knight);
            game.players[bob_seat].revealed_vp = 1;
            (
                game.total_victory_points(bob_seat),
                game.players[bob_seat].resources.total(),
                game.players[bob_seat].all_dev_cards().count(),
            )
        };

        let outcome = room
            .data
            .lock()
            .unwrap()
            .leave_with(&bob, LeaveChoice::Bot, None);
        assert_eq!(outcome, Ok(SeatOutcome::Continued { name: "Bob".into() }));

        let data = room.data.lock().unwrap();
        let game = data.game.as_ref().unwrap();
        assert!(!game.is_over(), "the game keeps running for everyone else");
        let seat = &data.members[bob_seat];
        assert!(seat.is_bot, "the seat becomes a bot");
        assert!(!seat.left, "the seat is live, not emptied");
        assert_eq!(seat.level, BotLevel::Medium, "takeover uses a Medium bot");
        assert!(
            game.players[bob_seat].is_bot,
            "the engine also treats the player as a bot (grudges, AI turns)"
        );
        assert_eq!(data.members.len(), 2, "seats are never re-indexed");
        assert_eq!(
            (
                game.total_victory_points(bob_seat),
                game.players[bob_seat].resources.total(),
                game.players[bob_seat].all_dev_cards().count(),
            ),
            before,
            "the bot keeps the leaver's resources, dev cards and VP"
        );
        assert!(
            game.log
                .iter()
                .any(|l| l.text.contains("Bob left — a bot takes over.")),
            "the shared log announces the takeover"
        );
        assert!(
            data.member_index(&bob).is_none(),
            "the old session token is dead"
        );
        assert!(data.member_index(&alice).is_some(), "the other human stays");
        drop(data);
        let _ = app;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn host_leave_midgame_with_bot_takeover_transfers_the_badge_first() {
        let (app, room, alice, bob, dir) = two_players("host-leave-mid-bot");
        let code = room_code(&room);
        let (_, carol) = app.join(&code, "Carol".into()).unwrap();
        app.start_game(&room, &alice).unwrap();
        let alice_seat = room.data.lock().unwrap().member_index(&alice).unwrap();

        let (outcome, carol_idx) = {
            let mut data = room.data.lock().unwrap();
            let carol_idx = data.member_index(&carol).unwrap();
            let outcome = data.leave_with(&alice, LeaveChoice::Bot, Some(carol_idx));
            (outcome, carol_idx)
        };
        assert_eq!(outcome, Ok(SeatOutcome::Continued { name: "Alice".into() }));

        let data = room.data.lock().unwrap();
        let game = data.game.as_ref().unwrap();
        assert!(!game.is_over(), "the host's leave does not end the game");
        assert_eq!(
            data.host_token, carol,
            "the badge moves to the chosen human before the seat becomes a bot"
        );
        assert!(data.is_host(&carol));
        assert!(data.members[alice_seat].is_bot, "the old host seat is a bot");
        assert!(game.players[alice_seat].is_bot);
        assert_eq!(data.members[carol_idx].is_bot, false, "Carol stays human");
        assert!(
            game.log
                .iter()
                .any(|l| l.text.contains("Alice left — a bot takes over.")),
            "the log names the takeover"
        );
        drop(data);

        assert_eq!(
            app.join(&code, "Alice".into()).map(|(_, token)| token),
            Err(JoinError::AlreadyStarted),
            "the leaver cannot rejoin a seat that a bot now owns"
        );
        let _ = bob;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn midgame_bot_takeover_needs_another_human() {
        let (app, dir) = app("leave-mid-alone");
        let (room, alice) = app.create_room("Alice".into(), 180, true).unwrap();
        room.data.lock().unwrap().add_bot(BotLevel::Medium);
        app.start_game(&room, &alice).unwrap();

        let err = room
            .data
            .lock()
            .unwrap()
            .leave_with(&alice, LeaveChoice::Bot, None);
        assert_eq!(
            err,
            Err("Only End the game is available when no other humans remain.".into())
        );
        {
            let data = room.data.lock().unwrap();
            assert!(
                !data.game.as_ref().unwrap().is_over(),
                "a refused takeover leaves the game intact"
            );
            assert!(data.member_index(&alice).is_some());
        }

        let ended = room
            .data
            .lock()
            .unwrap()
            .leave_with(&alice, LeaveChoice::End, None);
        assert_eq!(ended, Ok(SeatOutcome::Ended { name: "Alice".into() }));
        assert!(room.data.lock().unwrap().game.as_ref().unwrap().is_over());
        let _ = app;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_bot_takeover_takes_its_turn() {
        let (app, room, alice, bob, dir) = two_players("bot-takeover-turn");
        app.start_game(&room, &alice).unwrap();
        // Hand over whichever seat the engine is waiting on in setup.
        let (token, pid) = {
            let data = room.data.lock().unwrap();
            let game = data.game.as_ref().unwrap();
            let pid = game.setup_queue[game.setup_pos];
            (data.members[pid].token.clone(), pid)
        };
        room.data
            .lock()
            .unwrap()
            .leave_with(&token, LeaveChoice::Bot, None)
            .unwrap();

        // The first tick arms the first-action delay; the second acts.
        tick_bots(&app);
        room.data.lock().unwrap().bot_next_ms = 0;
        let before = room.data.lock().unwrap().game.as_ref().unwrap().moves.len();
        tick_bots(&app);
        let data = room.data.lock().unwrap();
        let game = data.game.as_ref().unwrap();
        assert!(
            game.moves.len() > before,
            "the bot plays the seat's turn without a human"
        );
        assert!(data.members[pid].is_bot);
        drop(data);
        let _ = bob;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn player_who_left_cannot_rejoin_the_finished_game() {
        let (app, room, alice, bob, dir) = two_players("rejoin-dead-seat");
        let code = room_code(&room);
        app.start_game(&room, &alice).unwrap();
        room.data.lock().unwrap().leave(&bob).unwrap();

        // Same name, but the seat is dead: joining must not hand the leaver a
        // fresh token for the finished game.
        let err = app.join(&code, "Bob".into()).map(|(_, token)| token);
        assert_eq!(err, Err(JoinError::AlreadyStarted));
        let data = room.data.lock().unwrap();
        assert!(data.member_index(&bob).is_none());
        drop(data);
        std::fs::remove_dir_all(&dir).ok();
    }
}
