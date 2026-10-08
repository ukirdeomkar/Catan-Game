use crate::game::actions::Action;
use crate::game::board::{Board, Building, Terrain};
use crate::game::resources::{Bundle, DevCard, Resource, ResourceHand};
use crate::game::rng::Rng64;
use crate::game::scoring::longest_road_length;
use crate::game::state::{Color, GameState, Phase, PlayerConfig, TradeResponse};

fn configs(n: usize) -> Vec<PlayerConfig> {
    (0..n)
        .map(|i| PlayerConfig {
            name: format!("P{i}"),
            color: Color::ALL[i],
            is_bot: false,
        })
        .collect()
}

fn fresh(n: usize, seed: u64) -> GameState {
    let mut s = GameState::new_lobby(configs(n), seed);
    s.start();
    s
}

/// Auto-plays the entire setup phase by trying every placement in order.
fn auto_setup(s: &mut GameState) {
    let mut guard = 0;
    while matches!(s.phase, Phase::Setup) {
        guard += 1;
        assert!(guard < 200, "setup failed to converge");
        let pid = s.current;
        if let Some(v) = s.setup_road_from {
            let edge = s.board.vertices[v]
                .edges
                .iter()
                .copied()
                .find(|&e| s.board.edges[e].owner.is_none())
                .expect("settlement must have a free edge");
            s.apply(pid, &Action::PlaceRoad { edge }).unwrap();
        } else {
            let placed = (0..s.board.vertices.len()).any(|v| {
                s.apply(pid, &Action::PlaceSettlement { vertex: v }).is_ok()
            });
            assert!(placed, "no legal settlement placement found");
        }
    }
}

// ---------------------------------------------------------------------------
// Board generation
// ---------------------------------------------------------------------------

#[test]
fn board_has_standard_geometry() {
    let b = Board::generate(&mut Rng64::new(7));
    assert_eq!(b.hexes.len(), 19);
    assert_eq!(b.vertices.len(), 54);
    assert_eq!(b.edges.len(), 72);

    let coastal = b.edges.iter().filter(|e| e.hexes.len() == 1).count();
    assert_eq!(coastal, 30);

    let port_vertices = b.vertices.iter().filter(|v| v.port.is_some()).count();
    assert_eq!(port_vertices, 18, "9 ports occupy 18 distinct vertices");
}

#[test]
fn board_has_correct_terrain_distribution() {
    let b = Board::generate(&mut Rng64::new(1234));
    let count = |t: Terrain| b.hexes.iter().filter(|h| h.terrain == t).count();
    assert_eq!(count(Terrain::Wood), 4);
    assert_eq!(count(Terrain::Wheat), 4);
    assert_eq!(count(Terrain::Sheep), 4);
    assert_eq!(count(Terrain::Brick), 3);
    assert_eq!(count(Terrain::Ore), 3);
    assert_eq!(count(Terrain::Desert), 1);

    // 18 number tokens on everything except the desert, exactly one 2 and one 12.
    let numbers: Vec<u8> = b.hexes.iter().filter_map(|h| h.number).collect();
    assert_eq!(numbers.len(), 18);
    assert_eq!(numbers.iter().filter(|&&n| n == 2).count(), 1);
    assert_eq!(numbers.iter().filter(|&&n| n == 12).count(), 1);
    assert!(numbers.iter().all(|&n| n != 7));
    assert!(numbers.iter().all(|&n| (2..=12).contains(&n)));
}

#[test]
fn red_numbers_never_adjacent() {
    // 6 and 8 are the red tokens and may not share a hex edge, for any seed.
    for seed in 0..200u64 {
        let b = Board::generate(&mut Rng64::new(seed));
        let is_red = |n: u8| n == 6 || n == 8;
        for e in &b.edges {
            if e.hexes.len() == 2 {
                let (a, c) = (e.hexes[0], e.hexes[1]);
                let ra = b.hexes[a].number.is_some_and(is_red);
                let rc = b.hexes[c].number.is_some_and(is_red);
                assert!(
                    !(ra && rc),
                    "seed {seed}: red tokens adjacent at hexes {a} and {c}"
                );
            }
        }
    }
}

#[test]
fn board_is_random_per_seed() {
    let a = Board::generate(&mut Rng64::new(1));
    let b = Board::generate(&mut Rng64::new(2));
    let terrains_a: Vec<_> = a.hexes.iter().map(|h| h.terrain).collect();
    let terrains_b: Vec<_> = b.hexes.iter().map(|h| h.terrain).collect();
    assert_ne!(terrains_a, terrains_b, "different seeds should vary the board");
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

#[test]
fn setup_snake_order_2_and_4_players() {
    let s2 = fresh(2, 5);
    assert_eq!(s2.setup_queue, vec![0, 1, 1, 0]);
    let s4 = fresh(4, 5);
    assert_eq!(s4.setup_queue, vec![0, 1, 2, 3, 3, 2, 1, 0]);
    let s3 = fresh(3, 5);
    assert_eq!(s3.setup_queue, vec![0, 1, 2, 2, 1, 0]);
}

#[test]
fn setup_completes_and_play_begins() {
    let mut s = fresh(4, 99);
    auto_setup(&mut s);
    assert!(matches!(s.phase, Phase::Play));
    assert_eq!(s.current, 0);
    for p in &s.players {
        assert_eq!(p.settlements_left, 3, "each placed 2 settlements");
        assert_eq!(p.roads_left, 13, "each placed 2 roads");
    }
}

// ---------------------------------------------------------------------------
// Production
// ---------------------------------------------------------------------------

#[test]
fn dice_production_pays_settlements_and_cities() {
    let mut s = fresh(2, 42);
    auto_setup(&mut s);

    let (hex, number, resource) = s
        .board
        .hexes
        .iter()
        .enumerate()
        .find_map(|(i, h)| {
            let n = h.number?;
            let r = h.terrain.resource()?;
            Some((i, n, r))
        })
        .unwrap();

    // Clear the board and set a controlled scenario.
    for v in s.board.vertices.iter_mut() {
        v.owner = None;
        v.building = Building::None;
    }
    for p in s.players.iter_mut() {
        p.resources = ResourceHand::default();
    }
    let v0 = s.board.hexes[hex].vertices[0];
    s.place_building(0, v0, Building::Settlement);
    let v1 = s.board.hexes[hex].vertices[1];
    s.place_building(1, v1, Building::City);
    s.robber_hex = if hex == 0 { 1 } else { 0 };

    s.produce(number);

    assert_eq!(s.players[0].resources.get(resource), 1, "settlement pays 1");
    assert_eq!(s.players[1].resources.get(resource), 2, "city pays 2");
}

#[test]
fn robber_blocks_production() {
    let mut s = fresh(2, 77);
    let (hex, number, resource) = s
        .board
        .hexes
        .iter()
        .enumerate()
        .find_map(|(i, h)| Some((i, h.number?, h.terrain.resource()?)))
        .unwrap();
    for v in s.board.vertices.iter_mut() {
        v.owner = None;
        v.building = Building::None;
    }
    for p in s.players.iter_mut() {
        p.resources = ResourceHand::default();
    }
    let v0 = s.board.hexes[hex].vertices[0];
    s.place_building(0, v0, Building::Settlement);
    s.robber_hex = hex;
    s.produce(number);
    assert_eq!(s.players[0].resources.get(resource), 0);
}

#[test]
fn moving_robber_onto_single_victim_auto_steals() {
    let mut s = fresh(2, 60);
    for v in s.board.vertices.iter_mut() {
        v.owner = None;
        v.building = Building::None;
    }
    for p in s.players.iter_mut() {
        p.resources = ResourceHand::default();
    }
    // One opponent building adjacent to hex 0: exactly one steal candidate.
    let hex = 0usize;
    let v = s.board.hexes[hex].vertices[0];
    s.place_building(1, v, Building::Settlement);
    s.players[1].resources = ResourceHand([1, 0, 0, 0, 0]);
    s.current = 0;
    s.robber_hex = 1;
    s.phase = Phase::MoveRobber { after_knight: false };

    s.apply(0, &Action::MoveRobber { hex }).unwrap();

    assert!(matches!(s.phase, Phase::Play), "auto-steal must resolve to Play");
    assert_eq!(s.players[0].resources.total(), 1, "actor steals one card");
    assert_eq!(s.players[1].resources.total(), 0);
}

// ---------------------------------------------------------------------------
// Building rules
// ---------------------------------------------------------------------------

#[test]
fn settlement_requires_connection_and_resources() {
    let mut s = fresh(2, 10);
    auto_setup(&mut s);
    s.current = 0;
    s.dice = Some((1, 1));

    // Pick an empty, disconnected vertex far from player 0's network.
    let far = (0..s.board.vertices.len())
        .find(|&v| {
            s.board.vertices[v].building == Building::None
                && !s.vertex_has_own_road(0, v)
                && s.board.vertices[v].edges.iter().all(|&e| s.board.edges[e].owner.is_none()
                    && s.board.edges[e]
                        .verts()
                        .iter()
                        .all(|&x| s.board.vertices[x].building == Building::None))
        });
    if let Some(v) = far {
        assert!(s.apply(0, &Action::BuildSettlement { vertex: v }).is_err());
    }

    // Give resources but still no connection -> still an error.
    s.players[0].resources = ResourceHand([5; 5]);
    if let Some(v) = far {
        assert!(s.apply(0, &Action::BuildSettlement { vertex: v }).is_err());
    }
}

#[test]
fn building_deducts_resources_and_bank_receives() {
    let mut s = fresh(2, 11);
    auto_setup(&mut s);
    s.current = 0;
    s.dice = Some((1, 1));
    s.players[0].resources = ResourceHand([9, 9, 9, 9, 9]);

    // Find a legal road edge attached to player 0's network.
    let edge = (0..s.board.edges.len())
        .find(|&e| {
            s.board.edges[e].owner.is_none() && s.edge_connects_to_network(0, e)
        })
        .unwrap();
    let bank_before = s.bank;
    s.apply(0, &Action::BuildRoad { edge }).unwrap();

    assert_eq!(s.players[0].resources.get(Resource::Wood), 8);
    assert_eq!(s.players[0].resources.get(Resource::Brick), 8);
    assert_eq!(s.bank.get(Resource::Wood), bank_before.get(Resource::Wood) + 1);
}

#[test]
fn distance_rule_blocks_adjacent_settlements() {
    let mut s = fresh(2, 12);
    auto_setup(&mut s);
    // Player 0's settlement vertices are known; adjacent vertices must be illegal.
    let owned: Vec<usize> = (0..s.board.vertices.len())
        .filter(|&v| s.board.vertices[v].owner == Some(0))
        .collect();
    let first = owned[0];
    let neighbor = s.board.edges[s.board.vertices[first].edges[0]]
        .other(first)
        .unwrap();
    s.current = 0;
    s.players[0].resources = ResourceHand([9; 5]);
    assert!(s.apply(0, &Action::BuildSettlement { vertex: neighbor }).is_err());
}

// ---------------------------------------------------------------------------
// Development cards
// ---------------------------------------------------------------------------

#[test]
fn monopoly_collects_all_of_a_resource() {
    let mut s = fresh(3, 21);
    auto_setup(&mut s);
    s.current = 0;
    s.players[0].dev_cards = vec![DevCard::Monopoly];
    s.played_dev_this_turn = false;
    s.players[1].resources = ResourceHand([0, 0, 0, 3, 0]);
    s.players[2].resources = ResourceHand([0, 0, 0, 2, 0]);
    s.players[0].resources = ResourceHand::default();

    s.apply(0, &Action::PlayMonopoly { resource: Resource::Ore })
        .unwrap();

    assert_eq!(s.players[0].resources.get(Resource::Ore), 5);
    assert_eq!(s.players[1].resources.get(Resource::Ore), 0);
    assert_eq!(s.players[2].resources.get(Resource::Ore), 0);
}

#[test]
fn one_dev_card_per_turn() {
    let mut s = fresh(2, 22);
    auto_setup(&mut s);
    s.current = 0;
    s.players[0].dev_cards = vec![DevCard::Monopoly, DevCard::YearOfPlenty];
    s.played_dev_this_turn = false;
    s.apply(0, &Action::PlayMonopoly { resource: Resource::Ore })
        .unwrap();
    assert!(
        s.apply(0, &Action::PlayYearOfPlenty { first: Resource::Wood, second: Resource::Wood })
            .is_err()
    );
}

#[test]
fn newly_bought_cards_cannot_be_played_same_turn() {
    let mut s = fresh(2, 23);
    auto_setup(&mut s);
    s.current = 0;
    s.players[0].resources = ResourceHand([9; 5]);
    s.apply(0, &Action::BuyDevCard).unwrap();
    assert_eq!(s.players[0].new_dev_cards.len(), 1);
    // Force a known card into the fresh pile and try to play it.
    let card = s.players[0].new_dev_cards[0];
    if card != DevCard::VictoryPoint {
        let action = match card {
            DevCard::Knight => Action::PlayKnight,
            DevCard::RoadBuilding => Action::PlayRoadBuilding,
            DevCard::Monopoly => Action::PlayMonopoly { resource: Resource::Ore },
            DevCard::YearOfPlenty => {
                Action::PlayYearOfPlenty { first: Resource::Ore, second: Resource::Ore }
            }
            DevCard::VictoryPoint => unreachable!(),
        };
        assert!(s.apply(0, &action).is_err(), "fresh card must not be playable");
    }
}

#[test]
fn road_building_places_two_free_roads() {
    let mut s = fresh(2, 24);
    auto_setup(&mut s);
    s.current = 0;
    s.dice = Some((1, 1));
    s.played_dev_this_turn = false;
    s.players[0].resources = ResourceHand::default();
    s.players[0].dev_cards = vec![DevCard::RoadBuilding];

    s.apply(0, &Action::PlayRoadBuilding).unwrap();
    assert_eq!(s.free_roads_left, 2, "card grants two free roads");
    assert!(
        s.players[0].dev_cards.is_empty(),
        "the card is consumed when played"
    );

    let mut built = 0;
    while s.free_roads_left > 0 {
        let edge = s.legal_road_edges(0).into_iter().next().expect("a legal edge");
        s.apply(0, &Action::BuildRoad { edge }).unwrap();
        built += 1;
        assert!(built <= 2, "never more than two free roads");
    }
    assert_eq!(built, 2);
    assert_eq!(s.players[0].resources.total(), 0, "free roads cost nothing");
    assert_eq!(s.players[0].roads_left, 11, "two roads came out of stock");
}

#[test]
fn road_building_refuses_when_it_cannot_be_used() {
    let mut s = fresh(2, 25);
    auto_setup(&mut s);
    s.current = 0;
    s.played_dev_this_turn = false;
    // No road pieces left: playing must fail without spending the card.
    s.players[0].roads_left = 0;
    s.players[0].dev_cards = vec![DevCard::RoadBuilding];
    let err = s.apply(0, &Action::PlayRoadBuilding).unwrap_err();
    assert!(err.0.contains("road"), "clear message: {}", err.0);
    assert_eq!(
        s.players[0].dev_cards,
        vec![DevCard::RoadBuilding],
        "an unusable card is not consumed"
    );
    assert_eq!(s.free_roads_left, 0);
    assert!(!s.played_dev_this_turn);
}

#[test]
fn road_building_grants_only_remaining_road_pieces() {
    let mut s = fresh(2, 26);
    auto_setup(&mut s);
    s.current = 0;
    s.played_dev_this_turn = false;
    s.players[0].roads_left = 1;
    s.players[0].dev_cards = vec![DevCard::RoadBuilding];

    s.apply(0, &Action::PlayRoadBuilding).unwrap();
    assert_eq!(s.free_roads_left, 1, "capped by pieces in stock");
    let edge = s.legal_road_edges(0).into_iter().next().expect("a legal edge");
    s.apply(0, &Action::BuildRoad { edge }).unwrap();
    assert_eq!(s.free_roads_left, 0);
    assert_eq!(s.players[0].roads_left, 0);
}

// ---------------------------------------------------------------------------
// Longest road
// ---------------------------------------------------------------------------

#[test]
fn longest_road_counts_a_ring_and_breaks_on_block() {
    let mut s = fresh(2, 30);
    for e in s.board.edges.iter_mut() {
        e.owner = None;
    }
    // A single hex's six edges form a ring owned by player 0.
    let hex_edges = s.board.hexes[0].vertices;
    for i in 0..6 {
        let a = hex_edges[i];
        let b = hex_edges[(i + 1) % 6];
        let e = s
            .board
            .edges
            .iter()
            .position(|ed| (ed.a == a && ed.b == b) || (ed.a == b && ed.b == a))
            .unwrap();
        s.board.edges[e].owner = Some(0);
    }
    assert_eq!(longest_road_length(&s.board, 0), 6);

    // An opponent settlement on a ring vertex splits the road.
    let block = s.board.hexes[0].vertices[0];
    s.board.vertices[block].owner = Some(1);
    s.board.vertices[block].building = Building::Settlement;
    assert_eq!(longest_road_length(&s.board, 0), 5);
}

#[test]
fn longest_road_still_counts_a_run_between_two_opponent_buildings() {
    let mut s = fresh(2, 33);
    for e in s.board.edges.iter_mut() {
        e.owner = None;
    }
    // A single hex's six edges form a ring owned by player 0.
    let verts = s.board.hexes[0].vertices;
    for i in 0..6 {
        let (a, b) = (verts[i], verts[(i + 1) % 6]);
        let e = s
            .board
            .edges
            .iter()
            .position(|ed| (ed.a == a && ed.b == b) || (ed.a == b && ed.b == a))
            .unwrap();
        s.board.edges[e].owner = Some(0);
    }
    // Block two adjacent ring vertices: the remaining five roads form one run
    // whose *both* ends are opponent buildings, and the sixth road sits
    // directly between two opponent buildings.
    for &v in &[verts[0], verts[1]] {
        s.board.vertices[v].owner = Some(1);
        s.board.vertices[v].building = Building::Settlement;
    }
    assert_eq!(longest_road_length(&s.board, 0), 5);
}

#[test]
fn longest_road_card_requires_five_and_handles_ties() {
    let mut s = fresh(2, 31);
    for e in s.board.edges.iter_mut() {
        e.owner = None;
    }
    s.recompute_special_cards();
    assert_eq!(s.longest_road, None, "short roads earn nothing");

    // Give player 0 a ring of 6.
    let verts = s.board.hexes[0].vertices;
    for i in 0..6 {
        let (a, b) = (verts[i], verts[(i + 1) % 6]);
        let e = s
            .board
            .edges
            .iter()
            .position(|ed| (ed.a == a && ed.b == b) || (ed.a == b && ed.b == a))
            .unwrap();
        s.board.edges[e].owner = Some(0);
    }
    s.recompute_special_cards();
    assert_eq!(s.longest_road, Some(0));
}

#[test]
fn taking_longest_road_is_logged() {
    let mut s = fresh(2, 32);
    for e in s.board.edges.iter_mut() {
        e.owner = None;
    }
    let verts = s.board.hexes[0].vertices;
    for i in 0..6 {
        let (a, b) = (verts[i], verts[(i + 1) % 6]);
        let e = s
            .board
            .edges
            .iter()
            .position(|ed| (ed.a == a && ed.b == b) || (ed.a == b && ed.b == a))
            .unwrap();
        s.board.edges[e].owner = Some(0);
    }
    let before = s.log.len();
    s.recompute_special_cards();
    assert_eq!(s.longest_road, Some(0));
    assert!(s.log.len() > before, "award should be written to the log");
    assert!(
        s.log.iter().any(|l| l.text.contains("Longest Road")),
        "log should announce the Longest Road award"
    );
}

// ---------------------------------------------------------------------------
// Trading
// ---------------------------------------------------------------------------

#[test]
fn bank_trade_uses_best_maritime_ratio() {
    let mut s = fresh(2, 40);
    auto_setup(&mut s);
    s.current = 0;
    s.dice = Some((1, 1));
    let ratio = s.best_maritime_ratio(0, Resource::Wood);
    assert!((2..=4).contains(&ratio));
    s.players[0].resources = ResourceHand::default();
    s.players[0].resources.set(Resource::Wood, ratio);
    s.apply(
        0,
        &Action::BankTrade { give: Resource::Wood, want: Resource::Ore },
    )
    .unwrap();
    assert_eq!(s.players[0].resources.get(Resource::Wood), 0);
    assert_eq!(s.players[0].resources.get(Resource::Ore), 1);
}

#[test]
fn player_trade_executes_on_accept() {
    let mut s = fresh(2, 41);
    auto_setup(&mut s);
    s.current = 0;
    s.dice = Some((1, 1));
    s.players[0].resources = ResourceHand([2, 0, 0, 0, 0]);
    s.players[1].resources = ResourceHand([0, 0, 3, 0, 0]);

    let give = Bundle::of(Resource::Wood, 1);
    let want = Bundle::of(Resource::Wheat, 1);
    s.apply(0, &Action::ProposeTrade { give, want }).unwrap();
    s.apply(1, &Action::RespondTrade { accept: true }).unwrap();

    assert_eq!(s.players[0].resources.get(Resource::Wood), 1);
    assert_eq!(s.players[0].resources.get(Resource::Wheat), 1);
    assert_eq!(s.players[1].resources.get(Resource::Wood), 1);
    assert_eq!(s.players[1].resources.get(Resource::Wheat), 2);
    assert!(s.trade.is_none());
}

#[test]
fn trade_with_no_possible_acceptor_is_rejected() {
    let mut s = fresh(2, 42);
    auto_setup(&mut s);
    s.current = 0;
    s.dice = Some((1, 1));
    s.players[0].resources = ResourceHand([2, 0, 0, 0, 0]);
    s.players[1].resources = ResourceHand([0, 0, 0, 0, 0]); // holds no wheat
    let give = Bundle::of(Resource::Wood, 1);
    let want = Bundle::of(Resource::Wheat, 1);
    assert!(
        s.apply(0, &Action::ProposeTrade { give, want }).is_err(),
        "an offer nobody can fulfil must be rejected outright"
    );
    assert!(s.trade.is_none());
}

#[test]
fn trade_auto_declines_players_without_the_wanted_resource() {
    let mut s = fresh(3, 43);
    auto_setup(&mut s);
    s.current = 0;
    s.dice = Some((1, 1));
    s.players[0].resources = ResourceHand([2, 0, 0, 0, 0]);
    s.players[1].resources = ResourceHand([0, 0, 0, 0, 0]); // cannot pay
    s.players[2].resources = ResourceHand([0, 0, 1, 0, 0]); // can pay
    let give = Bundle::of(Resource::Wood, 1);
    let want = Bundle::of(Resource::Wheat, 1);
    s.apply(0, &Action::ProposeTrade { give, want }).unwrap();
    let o = s.trade.as_ref().unwrap();
    let resp = |p| o.responses.iter().find(|(q, _)| *q == p).unwrap().1;
    assert_eq!(resp(1), TradeResponse::Declined, "pauper auto-declined");
    assert_eq!(resp(2), TradeResponse::Pending, "payer stays pending");
}

#[test]
fn eight_cards_triggers_discard_on_a_seven() {
    // Regression: a hand of exactly 8 (>7) must be asked to discard, whether
    // the 7 is rolled by the human or by another player.
    let mut seen = 0;
    for seed in 0..200_000u64 {
        let mut s = fresh(3, seed);
        auto_setup(&mut s);
        for p in s.players.iter_mut() {
            p.resources = ResourceHand::default();
        }
        s.players[0].resources = ResourceHand([0, 0, 0, 0, 8]);
        s.players[1].resources = ResourceHand([0, 0, 0, 0, 8]);
        s.players[2].resources = ResourceHand([0, 0, 0, 0, 8]);
        s.current = 0;
        s.dice = None;
        s.phase = Phase::Play;
        s.apply(0, &Action::RollDice).unwrap();
        if s.dice.map(|(a, b)| a + b) == Some(7) {
            seen += 1;
            assert_eq!(
                s.pending_discards,
                vec![0, 1, 2],
                "seed {seed}: all three 8-card hands must discard"
            );
            // The other players (bots) discarding first must not drop the
            // human from the pending list or end the discard phase.
            s.apply(
                1,
                &Action::Discard {
                    resources: Bundle([0, 0, 0, 0, 4]),
                },
            )
            .unwrap();
            s.apply(
                2,
                &Action::Discard {
                    resources: Bundle([0, 0, 0, 0, 4]),
                },
            )
            .unwrap();
            assert_eq!(s.pending_discards, vec![0]);
            assert!(matches!(s.phase, Phase::Discard));
        }
        if seen >= 5 {
            break;
        }
    }
    assert!(seen >= 5, "test never rolled a 7");
}

// ---------------------------------------------------------------------------
// Win condition
// ---------------------------------------------------------------------------

#[test]
fn ten_victory_points_wins_the_game() {
    let mut s = fresh(2, 50);
    auto_setup(&mut s);
    // Place five cities for player 0 => 10 VP.
    s.players[0].cities_left = 5;
    let mut placed = 0;
    for v in 0..s.board.vertices.len() {
        if s.board.vertices[v].building == Building::None {
            s.place_building(0, v, Building::City);
            placed += 1;
            if placed == 5 {
                break;
            }
        }
    }
    s.recompute_special_cards();
    assert!(s.check_winner());
    assert_eq!(s.winner, Some(0));
    assert!(s.is_over());
}

#[test]
fn hidden_victory_point_cards_count_toward_the_win_but_stay_private() {
    let mut s = fresh(2, 51);
    auto_setup(&mut s);
    let base = s.public_victory_points(0);
    s.players[0].dev_cards = vec![DevCard::VictoryPoint];
    s.players[0].new_dev_cards = vec![DevCard::VictoryPoint, DevCard::VictoryPoint];
    assert_eq!(s.public_victory_points(0), base, "hidden cards stay private");
    assert_eq!(s.total_victory_points(0), base + 3, "but they still count");
}

#[test]
fn nine_public_points_plus_a_hidden_card_is_a_win_at_ten() {
    let mut s = fresh(2, 52);
    auto_setup(&mut s);
    // Rebuild player 0's board as 4 cities + 1 settlement = 9 public VP.
    for v in s.board.vertices.iter_mut() {
        v.owner = None;
        v.building = Building::None;
    }
    s.players[0].cities_left = 4;
    s.players[0].settlements_left = 1;
    let mut built = 0;
    for v in 0..s.board.vertices.len() {
        if built < 4 && s.board.vertices[v].building == Building::None {
            s.place_building(0, v, Building::City);
            built += 1;
        }
    }
    let sv = (0..s.board.vertices.len())
        .find(|&v| s.board.vertices[v].building == Building::None)
        .unwrap();
    s.place_building(0, sv, Building::Settlement);
    assert_eq!(s.public_victory_points(0), 9);

    s.players[0].dev_cards = vec![DevCard::VictoryPoint];
    assert_eq!(s.total_victory_points(0), 10);
    assert!(s.check_winner());
    assert_eq!(s.winner, Some(0));
    assert!(
        s.log.iter().any(|l| l.text.contains("wins with 10 victory points")),
        "the win log must report the true total"
    );
}

#[test]
fn revealing_a_victory_point_card_makes_it_public() {
    let mut s = fresh(2, 70);
    auto_setup(&mut s);
    s.current = 0;
    s.dice = Some((1, 1));
    s.played_dev_this_turn = false;
    s.players[0].dev_cards = vec![DevCard::VictoryPoint];
    let base = s.public_victory_points(0);

    // A hidden VP card stays out of the public total but still counts toward
    // the total used for the win check.
    assert_eq!(s.public_victory_points(0), base);
    assert_eq!(s.total_victory_points(0), base + 1);

    s.apply(0, &Action::RevealVictoryPoint).unwrap();

    assert_eq!(s.players[0].revealed_vp, 1, "the card is now revealed");
    assert!(s.players[0].dev_cards.is_empty(), "the card left the hand");
    assert_eq!(
        s.public_victory_points(0),
        base + 1,
        "a revealed VP card is public"
    );
    assert_eq!(s.total_victory_points(0), base + 1);

    // Revealing is that turn's development-card action.
    s.players[0].dev_cards = vec![DevCard::VictoryPoint];
    assert!(s.apply(0, &Action::RevealVictoryPoint).is_err());
    assert_eq!(s.players[0].revealed_vp, 1);
}

#[test]
fn cannot_reveal_a_victory_point_card_bought_this_turn() {
    let mut s = fresh(2, 71);
    auto_setup(&mut s);
    s.current = 0;
    s.dice = Some((1, 1));
    s.played_dev_this_turn = false;
    s.players[0].new_dev_cards = vec![DevCard::VictoryPoint];
    assert!(
        s.apply(0, &Action::RevealVictoryPoint).is_err(),
        "a card bought this turn is not playable until the next turn"
    );
    assert_eq!(s.players[0].revealed_vp, 0);
}

#[test]
fn robbing_a_bot_builds_a_grudge() {
    let mut s = fresh(2, 60);
    auto_setup(&mut s);
    s.players[1].is_bot = true;
    let hex = 0;
    let v = s.board.hexes[hex].vertices[0];
    s.board.vertices[v].owner = Some(1);
    s.board.vertices[v].building = Building::Settlement;
    s.current = 0;
    s.phase = Phase::Steal { hex };
    s.apply(0, &Action::StealFrom { player: Some(1) }).unwrap();
    assert_eq!(s.anger(1, 0), 1, "a robbed bot is cross with the robber");
    // The grudge fades as the bot plays its own turns.
    s.decay_anger(1);
    assert_eq!(s.anger(1, 0), 0, "grudges cool off over turns");
}
