//! AI bot policy.
//!
//! Pure and synchronous: it only reads a `GameState` and returns one legal
//! `Action`. The driver in `state::tick_bots` applies the action. Three
//! difficulty tiers share one scoring core; the tier decides how much the bot
//! considers (spot quality, ports, dev cards, trading, lookahead).
//!
//! Deliberately free of randomness so bot games are reproducible: ties break
//! deterministically because the legal-move helpers return index-ordered lists.

use crate::game::actions::Action;
use crate::game::board::{Building, PortKind};
use crate::game::resources::{
    ALL_RESOURCES, Bundle, COST_CITY, COST_DEV, COST_ROAD, COST_SETTLEMENT, DevCard, Resource,
};
use crate::game::state::{BotLevel, GameState, Phase, PlayerId, TradeResponse};

/// Choose one legal action for `pid`, appropriate to the difficulty tier.
pub fn choose_action(game: &GameState, pid: PlayerId, level: BotLevel) -> Action {
    match &game.phase {
        Phase::Setup => {
            if game.setup_road_from.is_some() {
                choose_setup_road(game, pid, level)
            } else {
                choose_setup_settlement(game, pid, level)
            }
        }
        Phase::Play => choose_play(game, pid, level),
        Phase::Discard => choose_discard(game, pid, level),
        Phase::MoveRobber { .. } => Action::MoveRobber {
            hex: choose_robber_hex(game, pid, level),
        },
        Phase::Steal { hex } => Action::StealFrom {
            player: choose_steal_victim(game, pid, *hex, level),
        },
        Phase::Lobby | Phase::GameOver => Action::EndTurn,
    }
}

/// A guaranteed-legal action, used if the policy ever returns something the
/// engine rejects (defence against a bot wedging a room).
pub fn fallback_action(game: &GameState, pid: PlayerId) -> Option<Action> {
    match &game.phase {
        Phase::Setup => {
            if game.setup_road_from.is_some() {
                game.legal_road_edges(pid)
                    .first()
                    .map(|&edge| Action::PlaceRoad { edge })
            } else {
                game.legal_settlement_vertices(pid)
                    .first()
                    .map(|&vertex| Action::PlaceSettlement { vertex })
            }
        }
        Phase::Play => {
            if game.dice.is_none() {
                Some(Action::RollDice)
            } else {
                Some(Action::EndTurn)
            }
        }
        Phase::Discard => Some(choose_discard(game, pid, BotLevel::Easy)),
        Phase::MoveRobber { .. } => game
            .legal_robber_hexes()
            .first()
            .map(|&hex| Action::MoveRobber { hex }),
        Phase::Steal { .. } => Some(Action::StealFrom { player: None }),
        Phase::Lobby | Phase::GameOver => None,
    }
}

// ===========================================================================
// Setup placement
// ===========================================================================

fn choose_setup_settlement(game: &GameState, pid: PlayerId, level: BotLevel) -> Action {
    let legal = game.legal_settlement_vertices(pid);
    match best_vertex(game, &legal, level) {
        Some(vertex) => Action::PlaceSettlement { vertex },
        None => Action::PlaceSettlement { vertex: 0 },
    }
}

fn choose_setup_road(game: &GameState, pid: PlayerId, level: BotLevel) -> Action {
    let legal = game.legal_road_edges(pid);
    let from = game.setup_road_from;
    if legal.is_empty() {
        return Action::PlaceRoad { edge: 0 };
    }
    let edge = match level {
        BotLevel::Easy => legal[0],
        _ => legal
            .iter()
            .copied()
            .max_by_key(|&e| {
                let new_vertex = from
                    .and_then(|f| game.board.edges[e].other(f))
                    .or_else(|| {
                        game.board.edges[e]
                            .verts()
                            .iter()
                            .copied()
                            .find(|&v| game.board.vertices[v].building == Building::None)
                    });
                new_vertex
                    .map(|v| vertex_value(game, v, level))
                    .unwrap_or(0)
            })
            .unwrap_or(legal[0]),
    };
    Action::PlaceRoad { edge }
}

// ===========================================================================
// Normal turn
// ===========================================================================

fn choose_play(game: &GameState, pid: PlayerId, level: BotLevel) -> Action {
    // Answer a shared offer on the table before doing anything else.
    if let Some(t) = &game.trade {
        if t.from != pid {
            let pending = t
                .responses
                .iter()
                .any(|(p, r)| *p == pid && *r == TradeResponse::Pending);
            if pending {
                return Action::RespondTrade {
                    accept: evaluate_trade(game, pid, level),
                };
            }
        }
    }

    if game.dice.is_none() {
        return Action::RollDice;
    }

    if let Some(action) = try_dev_card(game, pid, level) {
        return action;
    }
    if let Some(action) = try_build(game, pid, level) {
        return action;
    }
    if let Some(action) = try_bank_trade(game, pid, level) {
        return action;
    }
    Action::EndTurn
}

/// Build the best affordable piece for the tier.
fn try_build(game: &GameState, pid: PlayerId, level: BotLevel) -> Option<Action> {
    let p = &game.players[pid];

    let settlement: Option<(i32, Action)> =
        if p.settlements_left > 0 && COST_SETTLEMENT.can_pay(&p.resources) {
            let legal = game.legal_settlement_vertices(pid);
            best_vertex(game, &legal, level).map(|vertex| {
                (
                    10 + vertex_value(game, vertex, level) + road_potential(game, vertex),
                    Action::BuildSettlement { vertex },
                )
            })
        } else {
            None
        };

    let city: Option<(i32, Action)> =
        if p.cities_left > 0 && COST_CITY.can_pay(&p.resources) {
            let legal = game.legal_city_vertices(pid);
            best_vertex(game, &legal, level).map(|vertex| {
                (
                    13 + vertex_value(game, vertex, level),
                    Action::BuildCity { vertex },
                )
            })
        } else {
            None
        };

    // Defer development cards until expansion is under way, so bots do not
    // burn their opening resources on the deck.
    let dev: Option<(i32, Action)> = if !game.dev_deck.is_empty()
        && COST_DEV.can_pay(&p.resources)
        && (p.settlements_left <= 3 || game.legal_settlement_vertices(pid).is_empty())
    {
        let base = if game.vp_cards_remaining() > 0 { 9 } else { 7 };
        Some((base, Action::BuyDevCard))
    } else {
        None
    };

    let road: Option<(i32, Action)> = if p.roads_left > 0 {
        let free = game.free_roads_left > 0;
        if free || COST_ROAD.can_pay(&p.resources) {
            let legal = game.legal_road_edges(pid);
            best_road(game, pid, &legal, level)
                .map(|edge| (road_value(game, pid, edge, level), Action::BuildRoad { edge }))
        } else {
            None
        }
    } else {
        None
    };

    match level {
        // Naive greed: expand first, cities last.
        BotLevel::Easy => first_action(&[settlement, road, city, dev]),
        // Sound priorities: settlement, then city, then dev, then road.
        BotLevel::Medium => first_action(&[settlement, city, dev, road]),
        // Evaluate every option by expected value.
        BotLevel::Hard => best_action(&[settlement, city, dev, road]),
    }
}

/// Play a development card when it clearly helps.
fn try_dev_card(game: &GameState, pid: PlayerId, level: BotLevel) -> Option<Action> {
    if game.played_dev_this_turn {
        return None;
    }
    let easy = level == BotLevel::Easy;
    let p = &game.players[pid];
    let has = |c: DevCard| p.dev_cards.contains(&c);

    // Resource-locking Monopoly is the great un-sticker, so even Easy uses it.
    if has(DevCard::Monopoly) {
        if let Some(resource) = best_monopoly_target(game, pid) {
            return Some(Action::PlayMonopoly { resource });
        }
    }
    if !easy {
        if has(DevCard::YearOfPlenty) {
            if let Some((first, second)) = year_of_plenty_choice(game, pid, level) {
                return Some(Action::PlayYearOfPlenty { first, second });
            }
        }
        if has(DevCard::RoadBuilding)
            && game.free_roads_left == 0
            && !game.legal_road_edges(pid).is_empty()
        {
            return Some(Action::PlayRoadBuilding);
        }
    }
    if has(DevCard::Knight) && should_play_knight(game, pid) {
        return Some(Action::PlayKnight);
    }
    None
}

fn best_monopoly_target(game: &GameState, pid: PlayerId) -> Option<Resource> {
    let need = next_build_need(game, pid, BotLevel::Medium);
    let mut best: Option<(i32, Resource)> = None;
    for r in ALL_RESOURCES {
        let mut held: i32 = 0;
        for (i, p) in game.players.iter().enumerate() {
            if i != pid {
                held += p.resources.get(r) as i32;
            }
        }
        if held < 3 {
            continue;
        }
        let score = held + if need.get(r) > 0 { 3 } else { 0 };
        if best.map(|(s, _)| score > s).unwrap_or(true) {
            best = Some((score, r));
        }
    }
    best.map(|(_, r)| r)
}

fn year_of_plenty_choice(
    game: &GameState,
    pid: PlayerId,
    level: BotLevel,
) -> Option<(Resource, Resource)> {
    let need = next_build_need(game, pid, level);
    let hand = game.players[pid].resources;
    let mut deficits: Vec<(i32, Resource)> = ALL_RESOURCES
        .iter()
        .copied()
        .filter_map(|r| {
            let deficit = (need.get(r) as i32 - hand.get(r) as i32).max(0);
            (deficit > 0 && game.bank.get(r) > 0).then_some((deficit, r))
        })
        .collect();
    deficits.sort_by_key(|&(d, _)| -d);
    match deficits.as_slice() {
        [] => None,
        [(d, r)] => (*d >= 2 && game.bank.get(*r) >= 2).then_some((*r, *r)),
        [(_, a), (_, b), ..] => Some((*a, *b)),
    }
}

fn should_play_knight(game: &GameState, pid: PlayerId) -> bool {
    let me = &game.players[pid];
    let knights = me.played_knights + me.dev_cards.iter().filter(|c| **c == DevCard::Knight).count() as u8;
    if game.largest_army != Some(pid) && knights >= 3 {
        return true;
    }
    game.players.iter().any(|p| {
        p.id != pid
            && (p.resources.total() >= 4 || game.public_victory_points(p.id) >= 8)
    })
}

/// Trade one surplus card for a needed one at the bank/port.
fn try_bank_trade(game: &GameState, pid: PlayerId, level: BotLevel) -> Option<Action> {
    let p = &game.players[pid];
    let need = next_build_need(game, pid, level);

    let want = ALL_RESOURCES
        .iter()
        .copied()
        .filter(|&r| need.get(r) > p.resources.get(r) && game.bank.get(r) > 0)
        .min_by_key(|&r| game.best_maritime_ratio(pid, r))?;

    let max_ratio = if level == BotLevel::Hard { 4 } else { 3 };
    let mut givers: Vec<(u8, i32, Resource)> = ALL_RESOURCES
        .iter()
        .copied()
        .filter(|&r| r != want && p.resources.get(r) > need.get(r))
        .filter_map(|r| {
            let ratio = game.best_maritime_ratio(pid, r);
            (ratio <= max_ratio).then_some((ratio, (p.resources.get(r) - need.get(r)) as i32, r))
        })
        .collect();
    givers.sort_by_key(|&(ratio, surplus, _)| (ratio, -surplus));
    let give = givers
        .into_iter()
        .find(|&(ratio, _, r)| p.resources.has(r, ratio))
        .map(|(_, _, r)| r)?;

    Some(Action::BankTrade { give, want })
}

fn evaluate_trade(game: &GameState, pid: PlayerId, level: BotLevel) -> bool {
    if level == BotLevel::Easy {
        return false;
    }
    let Some(t) = game.trade.as_ref() else {
        return false;
    };
    if !t.want.can_pay(&game.players[pid].resources) {
        return false;
    }
    let mut received = 0;
    let mut paid = 0;
    for r in ALL_RESOURCES {
        received += t.give.get(r) as i32 * resource_value(game, pid, r);
        paid += t.want.get(r) as i32 * resource_value(game, pid, r);
    }
    received > paid + 1
}

fn choose_discard(game: &GameState, pid: PlayerId, level: BotLevel) -> Action {
    let hand = game.players[pid].resources;
    let required = hand.total() / 2;
    let mut counts = [0u8; 5];
    let mut remaining = required;

    // Keep cards earmarked for the next build; dump the surplus first.
    if level != BotLevel::Easy && remaining > 0 {
        let need = next_build_need(game, pid, level);
        for r in ALL_RESOURCES {
            let surplus = hand.get(r).saturating_sub(need.get(r));
            let take = surplus.min(remaining);
            counts[r.as_usize()] = take;
            remaining -= take;
        }
    }
    // Still short: shed the largest remaining stacks.
    if remaining > 0 {
        let mut order: Vec<Resource> = ALL_RESOURCES.to_vec();
        order.sort_by_key(|r| std::cmp::Reverse(hand.get(*r) - counts[r.as_usize()]));
        for r in order {
            if remaining == 0 {
                break;
            }
            let free = hand.get(r).saturating_sub(counts[r.as_usize()]);
            let take = free.min(remaining);
            counts[r.as_usize()] += take;
            remaining -= take;
        }
    }
    Action::Discard {
        resources: Bundle(counts),
    }
}

// ===========================================================================
// Robber
// ===========================================================================

fn choose_robber_hex(game: &GameState, pid: PlayerId, level: BotLevel) -> usize {
    let legal = game.legal_robber_hexes();
    if legal.is_empty() {
        return game.robber_hex;
    }
    match level {
        BotLevel::Easy => legal[0],
        _ => *legal
            .iter()
            .max_by_key(|&&h| robber_score(game, pid, h))
            .unwrap_or(&legal[0]),
    }
}

fn robber_score(game: &GameState, pid: PlayerId, hex: usize) -> i32 {
    let h = &game.board.hexes[hex];
    let block_pips = h.number.map(pip).unwrap_or(0);
    let mut score = 0;
    for &v in &h.vertices {
        let vt = &game.board.vertices[v];
        let Some(owner) = vt.owner else {
            continue;
        };
        if vt.building == Building::None {
            continue;
        }
        if owner == pid {
            score -= block_pips * 2 + 3;
        } else {
            let steal = game.players[owner].resources.total() as i32;
            let leader = if game.public_victory_points(owner) >= 7 { 3 } else { 0 };
            score += block_pips + steal.min(6) + leader;
        }
    }
    score
}

fn choose_steal_victim(
    game: &GameState,
    pid: PlayerId,
    hex: usize,
    level: BotLevel,
) -> Option<PlayerId> {
    let candidates = game.steal_candidates(hex, pid);
    match level {
        BotLevel::Easy => candidates.first().copied(),
        _ => candidates
            .iter()
            .copied()
            .max_by_key(|&o| game.players[o].resources.total()),
    }
}

// ===========================================================================
// Scoring helpers
// ===========================================================================

/// Classic dice pip weight (probability of rolling a number).
fn pip(n: u8) -> i32 {
    match n {
        2 | 12 => 1,
        3 | 11 => 2,
        4 | 10 => 3,
        5 | 9 => 4,
        6 | 8 => 5,
        _ => 0,
    }
}

/// Expected production of an intersection, with tier-dependent extras.
fn vertex_value(game: &GameState, v: usize, level: BotLevel) -> i32 {
    let vt = &game.board.vertices[v];
    let mut pips = 0;
    let mut seen = [false; 5];
    for &h in &vt.hexes {
        let hex = &game.board.hexes[h];
        if let (Some(n), Some(r)) = (hex.number, hex.terrain.resource()) {
            pips += pip(n);
            seen[r.as_usize()] = true;
        }
    }
    if level == BotLevel::Easy {
        return pips;
    }
    let diversity = seen.iter().filter(|s| **s).count() as i32;
    let mut score = pips * 2 + diversity * 3;
    if let Some(port) = vt.port {
        score += match port {
            PortKind::Generic => 4,
            _ => 6,
        };
    }
    score
}

/// Density of empty neighbouring intersections (room to expand from a spot).
fn road_potential(game: &GameState, v: usize) -> i32 {
    game.board.vertices[v]
        .edges
        .iter()
        .filter_map(|&e| game.board.edges[e].other(v))
        .filter(|&nv| game.board.vertices[nv].building == Building::None)
        .count() as i32
}

fn road_value(game: &GameState, pid: PlayerId, edge: usize, level: BotLevel) -> i32 {
    let ed = &game.board.edges[edge];
    let mut best = 0;
    for &v in &[ed.a, ed.b] {
        if game.board.vertices[v].building == Building::None {
            best = best.max(vertex_value(game, v, level));
        }
    }
    let mut value = best / 2;

    if level == BotLevel::Hard {
        // Reward roads that advance (or contest) Longest Road.
        let mut probe = game.clone();
        probe.board.edges[edge].owner = Some(pid);
        let len = probe.longest_road_of(pid) as i32;
        value += if len >= 5 { 6 + len } else { len };
    }
    value
}

/// Value of holding one more of a resource for this player.
fn resource_value(game: &GameState, pid: PlayerId, r: Resource) -> i32 {
    let need = next_build_need(game, pid, BotLevel::Medium).get(r) as i32;
    let held = game.players[pid].resources.get(r) as i32;
    let need_bonus = if need > 0 { 3 } else { 0 };
    let scarcity = (4 - held).max(0);
    3 + need_bonus + scarcity
}

/// Cost basket of the bot's most likely next purchase (used for trading and
/// discarding decisions).
fn next_build_need(game: &GameState, pid: PlayerId, _level: BotLevel) -> Bundle {
    let p = &game.players[pid];
    if p.settlements_left > 0 && !game.legal_settlement_vertices(pid).is_empty() {
        return COST_SETTLEMENT;
    }
    if p.cities_left > 0 && !game.legal_city_vertices(pid).is_empty() {
        return COST_CITY;
    }
    if !game.dev_deck.is_empty() {
        return COST_DEV;
    }
    if p.roads_left > 0 {
        return COST_ROAD;
    }
    COST_DEV
}

fn best_vertex(game: &GameState, legal: &[usize], level: BotLevel) -> Option<usize> {
    legal
        .iter()
        .copied()
        .max_by_key(|&v| vertex_value(game, v, level))
}

fn best_road(game: &GameState, pid: PlayerId, legal: &[usize], level: BotLevel) -> Option<usize> {
    match level {
        BotLevel::Easy => legal.first().copied(),
        _ => legal
            .iter()
            .copied()
            .max_by_key(|&e| road_value(game, pid, e, level)),
    }
}

fn first_action(options: &[Option<(i32, Action)>]) -> Option<Action> {
    options.iter().flatten().next().map(|(_, a)| a.clone())
}

fn best_action(options: &[Option<(i32, Action)>]) -> Option<Action> {
    options
        .iter()
        .flatten()
        .max_by_key(|pair| pair.0)
        .map(|(_, a)| a.clone())
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::state::{BotLevel, Color, GameState, Phase, PlayerConfig};

    fn bot_configs(n: usize) -> Vec<PlayerConfig> {
        (0..n)
            .map(|i| PlayerConfig {
                name: format!("B{i}"),
                color: Color::ALL[i],
                is_bot: true,
            })
            .collect()
    }

    /// The next player that must act in a game where every seat is a bot.
    fn pending(game: &GameState) -> Option<PlayerId> {
        match &game.phase {
            Phase::Lobby | Phase::GameOver => None,
            Phase::Setup => game.setup_queue.get(game.setup_pos).copied(),
            Phase::Play => {
                if let Some(t) = &game.trade {
                    if let Some((p, _)) = t
                        .responses
                        .iter()
                        .find(|(_, r)| *r == TradeResponse::Pending)
                    {
                        return Some(*p);
                    }
                }
                Some(game.current)
            }
            Phase::Discard => game.pending_discards.first().copied(),
            Phase::MoveRobber { .. } | Phase::Steal { .. } => Some(game.current),
        }
    }

    /// Drive a full all-bot game to completion with every seat on `level`.
    fn play_out(level: BotLevel, seed: u64) -> GameState {
        let mut game = GameState::new_lobby(bot_configs(4), seed);
        game.start();
        let mut steps = 0;
        while !game.is_over() {
            steps += 1;
            assert!(
                steps < 40_000,
                "game {seed} ({level:?}) failed to finish: turn={} phase={:?} current={} \
                 vp={:?} hands={:?} bank={:?} pieces={:?} legal_settle={:?} legal_city={:?} \
                 robber={}",
                game.turn,
                game.phase,
                game.current,
                (0..game.players.len())
                    .map(|p| game.total_victory_points(p))
                    .collect::<Vec<_>>(),
                (0..game.players.len())
                    .map(|p| game.players[p].resources.0)
                    .collect::<Vec<_>>(),
                game.bank.0,
                (0..game.players.len())
                    .map(|p| (
                        game.players[p].settlements_left,
                        game.players[p].cities_left,
                        game.players[p].roads_left
                    ))
                    .collect::<Vec<_>>(),
                (0..game.players.len())
                    .map(|p| game.legal_settlement_vertices(p).len())
                    .collect::<Vec<_>>(),
                (0..game.players.len())
                    .map(|p| game.legal_city_vertices(p).len())
                    .collect::<Vec<_>>(),
                game.robber_hex,
            );
            let Some(pid) = pending(&game) else {
                break;
            };
            let action = choose_action(&game, pid, level);
            game.apply(pid, &action)
                .unwrap_or_else(|e| panic!("illegal {action:?} by {pid} ({level:?}): {e}"));
        }
        game
    }

    #[test]
    fn every_level_completes_full_games() {
        for level in BotLevel::ALL {
            for seed in [1u64, 7, 42, 99, 1234] {
                let game = play_out(level, seed);
                assert!(game.is_over(), "seed {seed} {level:?} did not end");
                assert!(game.winner.is_some(), "seed {seed} {level:?} has no winner");
            }
        }
    }

    #[test]
    fn setup_and_play_produce_moves() {
        let mut game = GameState::new_lobby(bot_configs(2), 5);
        game.start();
        // Setup: each bot places two settlements and two roads.
        let mut guard = 0;
        while matches!(game.phase, Phase::Setup) {
            guard += 1;
            assert!(guard < 20, "setup did not converge");
            let pid = pending(&game).unwrap();
            let action = choose_action(&game, pid, BotLevel::Hard);
            game.apply(pid, &action).unwrap();
        }
        assert!(matches!(game.phase, Phase::Play));
        // First play action is a roll.
        let pid = game.current;
        assert!(matches!(
            choose_action(&game, pid, BotLevel::Medium),
            Action::RollDice
        ));
    }

    #[test]
    fn discard_returns_exactly_half() {
        let mut game = GameState::new_lobby(bot_configs(2), 8);
        game.start();
        game.players[0].resources = crate::game::resources::ResourceHand([4, 4, 4, 4, 4]);
        game.pending_discards = vec![0];
        game.phase = Phase::Discard;
        for level in BotLevel::ALL {
            let Action::Discard { resources } =
                choose_action(&game, 0, level)
            else {
                panic!("expected a discard");
            };
            assert_eq!(resources.total(), 10, "{level:?} must discard half of 20");
            assert!(resources.can_pay(&game.players[0].resources));
        }
    }
}
