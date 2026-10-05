use crate::game::board::Building;
use crate::game::resources::{
    ALL_RESOURCES, COST_CITY, COST_DEV, COST_ROAD, COST_SETTLEMENT, Bundle, DevCard, Resource,
};
use crate::game::scoring::longest_road_length;
use crate::game::state::{GameState, Phase, PlayerId, TradeOffer, TradeResponse};

#[derive(Debug, Clone)]
pub struct RuleError(pub String);

impl RuleError {
    pub fn new(msg: impl Into<String>) -> Self {
        RuleError(msg.into())
    }
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for RuleError {}

pub type RuleResult = Result<(), RuleError>;

#[derive(Debug, Clone)]
pub enum Action {
    // --- Setup phase ---
    PlaceSettlement { vertex: usize },
    PlaceRoad { edge: usize },

    // --- Turn actions ---
    RollDice,
    BuildRoad { edge: usize },
    BuildSettlement { vertex: usize },
    BuildCity { vertex: usize },
    BuyDevCard,
    PlayKnight,
    PlayRoadBuilding,
    PlayYearOfPlenty { first: Resource, second: Resource },
    PlayMonopoly { resource: Resource },

    // --- Robber flow ---
    MoveRobber { hex: usize },
    StealFrom { player: Option<PlayerId> },
    Discard { resources: Bundle },

    // --- Trading ---
    BankTrade { give: Resource, want: Resource },
    ProposeTrade { give: Bundle, want: Bundle },
    RespondTrade { accept: bool },
    CancelTrade,

    EndTurn,
}

impl GameState {
    pub fn apply(&mut self, actor: PlayerId, action: &Action) -> RuleResult {
        if actor >= self.players.len() {
            return Err(RuleError::new("Unknown player"));
        }
        if self.is_over() {
            return Err(RuleError::new("The game is over"));
        }
        match action {
            Action::PlaceSettlement { vertex } => self.setup_place_settlement(actor, *vertex),
            Action::PlaceRoad { edge } => self.setup_place_road(actor, *edge),
            Action::RollDice => self.roll_dice(actor),
            Action::BuildRoad { edge } => self.build_road(actor, *edge),
            Action::BuildSettlement { vertex } => self.build_settlement(actor, *vertex),
            Action::BuildCity { vertex } => self.build_city(actor, *vertex),
            Action::BuyDevCard => self.buy_dev_card(actor),
            Action::PlayKnight => self.play_knight(actor),
            Action::PlayRoadBuilding => self.play_road_building(actor),
            Action::PlayYearOfPlenty { first, second } => {
                self.play_year_of_plenty(actor, *first, *second)
            }
            Action::PlayMonopoly { resource } => self.play_monopoly(actor, *resource),
            Action::MoveRobber { hex } => self.move_robber(actor, *hex),
            Action::StealFrom { player } => self.steal_from(actor, *player),
            Action::Discard { resources } => self.discard(actor, *resources),
            Action::BankTrade { give, want } => self.bank_trade(actor, *give, *want),
            Action::ProposeTrade { give, want } => self.propose_trade(actor, *give, *want),
            Action::RespondTrade { accept } => self.respond_trade(actor, *accept),
            Action::CancelTrade => self.cancel_trade(actor),
            Action::EndTurn => self.end_turn(actor),
        }
    }

    // =====================================================================
    // Setup phase
    // =====================================================================

    fn setup_place_settlement(&mut self, actor: PlayerId, vertex: usize) -> RuleResult {
        if !matches!(self.phase, Phase::Setup) {
            return Err(RuleError::new("Not in the setup phase"));
        }
        if self.setup_road_from.is_some() {
            return Err(RuleError::new("Place your road first"));
        }
        if actor != self.setup_queue[self.setup_pos] {
            return Err(RuleError::new("Not your placement turn"));
        }
        self.validate_settlement_spot(vertex)?;
        self.place_building(actor, vertex, Building::Settlement);
        self.setup_road_from = Some(vertex);
        let name = self.players[actor].name.clone();
        self.push_log(
            Some(actor),
            format!("{name} places a settlement during setup."),
        );
        Ok(())
    }

    fn setup_place_road(&mut self, actor: PlayerId, edge: usize) -> RuleResult {
        if !matches!(self.phase, Phase::Setup) {
            return Err(RuleError::new("Not in the setup phase"));
        }
        let Some(from) = self.setup_road_from else {
            return Err(RuleError::new("Place your settlement first"));
        };
        if actor != self.setup_queue[self.setup_pos] {
            return Err(RuleError::new("Not your placement turn"));
        }
        if self.board.edges[edge].owner.is_some() {
            return Err(RuleError::new("That road is already taken"));
        }
        if self.board.edges[edge].other(from).is_none() {
            return Err(RuleError::new("The road must touch your new settlement"));
        }
        if self.players[actor].roads_left == 0 {
            return Err(RuleError::new("No roads left"));
        }
        self.place_road(actor, edge);
        self.setup_road_from = None;
        self.setup_pos += 1;
        if self.setup_pos >= self.setup_queue.len() {
            self.begin_play_after_setup();
        } else {
            self.current = self.setup_queue[self.setup_pos];
        }
        Ok(())
    }

    fn begin_play_after_setup(&mut self) {
        self.phase = Phase::Play;
        self.current = 0;
        self.turn = 1;
        self.recompute_special_cards();
        let name = self.players[0].name.clone();
        self.push_log(None, format!("Setup complete. {name} begins."));
    }

    // =====================================================================
    // Rolling and production
    // =====================================================================

    fn roll_dice(&mut self, actor: PlayerId) -> RuleResult {
        self.require_play_turn(actor)?;
        if self.dice.is_some() {
            return Err(RuleError::new("You already rolled this turn"));
        }
        use rand::RngExt;
        let d1 = self.rng.random_range(1..=6u8);
        let d2 = self.rng.random_range(1..=6u8);
        let sum = d1 + d2;
        self.dice = Some((d1, d2));
        let name = self.players[actor].name.clone();
        self.push_log(Some(actor), format!("{name} rolls {d1} + {d2} = {sum}."));

        if sum == 7 {
            self.pending_discards = (0..self.players.len())
                .filter(|&p| self.players[p].resources.total() > 7)
                .collect();
            if self.pending_discards.is_empty() {
                self.phase = Phase::MoveRobber { after_knight: false };
            } else {
                self.phase = Phase::Discard;
            }
        } else {
            self.produce(sum);
        }
        Ok(())
    }

    pub(crate) fn produce(&mut self, roll: u8) {
        let mut gains: Vec<(PlayerId, Resource)> = Vec::new();
        for (hi, hex) in self.board.hexes.iter().enumerate() {
            if hex.number != Some(roll) || hi == self.robber_hex {
                continue;
            }
            let Some(res) = hex.terrain.resource() else {
                continue;
            };
            for &v in &hex.vertices {
                let vt = &self.board.vertices[v];
                if let Some(owner) = vt.owner {
                    let amount = match vt.building {
                        Building::Settlement => 1,
                        Building::City => 2,
                        Building::None => 0,
                    };
                    for _ in 0..amount {
                        gains.push((owner, res));
                    }
                }
            }
        }
        // Bank is limited: serve gains in order until each resource runs out.
        let mut served: Vec<(PlayerId, Resource)> = Vec::new();
        for (pid, res) in gains {
            if self.bank.get(res) > 0 {
                self.bank.set(res, self.bank.get(res) - 1);
                self.players[pid].resources.add(res, 1);
                served.push((pid, res));
            }
        }
        if served.is_empty() {
            self.push_log(None, format!("No production on {roll}."));
        } else {
            self.push_log(None, format!("Resources produced on {roll}."));
        }
    }

    // =====================================================================
    // Building
    // =====================================================================

    fn build_road(&mut self, actor: PlayerId, edge: usize) -> RuleResult {
        self.require_play_turn(actor)?;
        if self.board.edges[edge].owner.is_some() {
            return Err(RuleError::new("That road is already taken"));
        }
        if !self.edge_connects_to_network(actor, edge) {
            return Err(RuleError::new("The road must connect to your network"));
        }
        if self.players[actor].roads_left == 0 {
            return Err(RuleError::new("No roads left"));
        }
        let free = self.free_roads_left > 0;
        if !free {
            if !COST_ROAD.can_pay(&self.players[actor].resources) {
                return Err(RuleError::new("Not enough resources for a road"));
            }
            COST_ROAD.pay(&mut self.players[actor].resources);
            for &r in ALL_RESOURCES.iter() {
                self.bank_give(r, COST_ROAD.get(r));
            }
        } else {
            self.free_roads_left -= 1;
        }
        self.place_road(actor, edge);
        self.after_build(actor);
        let name = self.players[actor].name.clone();
        let suffix = if free { " (free)" } else { "" };
        self.push_log(Some(actor), format!("{name} builds a road{suffix}."));
        Ok(())
    }

    fn build_settlement(&mut self, actor: PlayerId, vertex: usize) -> RuleResult {
        self.require_play_turn(actor)?;
        if !self.has_piece_available(actor, Building::Settlement) {
            return Err(RuleError::new("No settlement pieces left"));
        }
        self.validate_settlement_spot(vertex)?;
        if !self.vertex_has_own_road(actor, vertex) {
            return Err(RuleError::new("The settlement must connect to your road"));
        }
        if !COST_SETTLEMENT.can_pay(&self.players[actor].resources) {
            return Err(RuleError::new("Not enough resources for a settlement"));
        }
        COST_SETTLEMENT.pay(&mut self.players[actor].resources);
        for &r in ALL_RESOURCES.iter() {
            self.bank_give(r, COST_SETTLEMENT.get(r));
        }
        self.place_building(actor, vertex, Building::Settlement);
        self.after_build(actor);
        let name = self.players[actor].name.clone();
        self.push_log(Some(actor), format!("{name} builds a settlement."));
        Ok(())
    }

    fn build_city(&mut self, actor: PlayerId, vertex: usize) -> RuleResult {
        self.require_play_turn(actor)?;
        let vt = &self.board.vertices[vertex];
        if vt.owner != Some(actor) || vt.building != Building::Settlement {
            return Err(RuleError::new("You can only upgrade your own settlement"));
        }
        if !self.has_piece_available(actor, Building::City) {
            return Err(RuleError::new("No city pieces left"));
        }
        if !COST_CITY.can_pay(&self.players[actor].resources) {
            return Err(RuleError::new("Not enough resources for a city"));
        }
        COST_CITY.pay(&mut self.players[actor].resources);
        for &r in ALL_RESOURCES.iter() {
            self.bank_give(r, COST_CITY.get(r));
        }
        self.upgrade_building(actor, vertex);
        self.after_build(actor);
        let name = self.players[actor].name.clone();
        self.push_log(Some(actor), format!("{name} upgrades to a city."));
        Ok(())
    }

    fn after_build(&mut self, _actor: PlayerId) {
        self.recompute_special_cards();
        self.check_winner();
    }

    // =====================================================================
    // Development cards
    // =====================================================================

    fn buy_dev_card(&mut self, actor: PlayerId) -> RuleResult {
        self.require_play_turn(actor)?;
        if self.dev_deck.is_empty() {
            return Err(RuleError::new("The development deck is empty"));
        }
        if !COST_DEV.can_pay(&self.players[actor].resources) {
            return Err(RuleError::new("Not enough resources for a development card"));
        }
        COST_DEV.pay(&mut self.players[actor].resources);
        for &r in ALL_RESOURCES.iter() {
            self.bank_give(r, COST_DEV.get(r));
        }
        let card = self.dev_deck.pop().unwrap();
        self.players[actor].new_dev_cards.push(card);
        let name = self.players[actor].name.clone();
        self.push_log(
            Some(actor),
            format!("{name} buys a development card."),
        );
        self.check_winner();
        Ok(())
    }

    fn require_playable_dev(&self, actor: PlayerId, card: DevCard) -> RuleResult {
        if self.played_dev_this_turn {
            return Err(RuleError::new("Only one development card per turn"));
        }
        let p = &self.players[actor];
        if !p.dev_cards.contains(&card) {
            return Err(RuleError::new("You do not have that card to play"));
        }
        Ok(())
    }

    fn remove_dev_card(&mut self, actor: PlayerId, card: DevCard) {
        if let Some(pos) = self.players[actor].dev_cards.iter().position(|c| *c == card) {
            self.players[actor].dev_cards.remove(pos);
        }
        self.played_dev_this_turn = true;
    }

    fn play_knight(&mut self, actor: PlayerId) -> RuleResult {
        self.require_play_turn(actor)?;
        self.require_playable_dev(actor, DevCard::Knight)?;
        self.remove_dev_card(actor, DevCard::Knight);
        self.players[actor].played_knights += 1;
        self.recompute_special_cards();
        let name = self.players[actor].name.clone();
        self.push_log(Some(actor), format!("{name} plays a Knight."));
        self.phase = Phase::MoveRobber { after_knight: true };
        Ok(())
    }

    fn play_road_building(&mut self, actor: PlayerId) -> RuleResult {
        self.require_play_turn(actor)?;
        self.require_playable_dev(actor, DevCard::RoadBuilding)?;
        self.remove_dev_card(actor, DevCard::RoadBuilding);
        self.free_roads_left = 2;
        let name = self.players[actor].name.clone();
        self.push_log(Some(actor), format!("{name} plays Road Building."));
        Ok(())
    }

    fn play_year_of_plenty(
        &mut self,
        actor: PlayerId,
        first: Resource,
        second: Resource,
    ) -> RuleResult {
        self.require_play_turn(actor)?;
        self.require_playable_dev(actor, DevCard::YearOfPlenty)?;
        if self.bank.get(first) < 1 {
            return Err(RuleError::new("The bank is out of that resource"));
        }
        let mut need_second = 1;
        if first == second {
            if self.bank.get(first) < 2 {
                return Err(RuleError::new("The bank does not have two of that resource"));
            }
            need_second = 0;
        } else if self.bank.get(second) < 1 {
            return Err(RuleError::new("The bank is out of that resource"));
        }
        self.remove_dev_card(actor, DevCard::YearOfPlenty);
        self.bank_take(first, 1);
        self.players[actor].resources.add(first, 1);
        if need_second == 1 {
            self.bank_take(second, 1);
        }
        self.players[actor].resources.add(second, 1);
        let name = self.players[actor].name.clone();
        self.push_log(
            Some(actor),
            format!(
                "{name} plays Year of Plenty: takes {} and {}.",
                first.name(),
                second.name()
            ),
        );
        Ok(())
    }

    fn play_monopoly(&mut self, actor: PlayerId, resource: Resource) -> RuleResult {
        self.require_play_turn(actor)?;
        self.require_playable_dev(actor, DevCard::Monopoly)?;
        self.remove_dev_card(actor, DevCard::Monopoly);
        let mut taken = 0u8;
        for pid in 0..self.players.len() {
            if pid == actor {
                continue;
            }
            let amount = self.players[pid].resources.get(resource);
            if amount > 0 {
                self.players[pid].resources.set(resource, 0);
                taken = taken.saturating_add(amount);
            }
        }
        self.players[actor].resources.add(resource, taken);
        let name = self.players[actor].name.clone();
        self.push_log(
            Some(actor),
            format!(
                "{name} plays Monopoly on {} and collects {taken}.",
                resource.name()
            ),
        );
        Ok(())
    }

    // =====================================================================
    // Robber flow
    // =====================================================================

    fn move_robber(&mut self, actor: PlayerId, hex: usize) -> RuleResult {
        if actor != self.current {
            return Err(RuleError::new("Not your turn"));
        }
        let after_knight = match self.phase {
            Phase::MoveRobber { after_knight } => after_knight,
            _ => return Err(RuleError::new("You are not moving the robber right now")),
        };
        if hex >= self.board.hexes.len() {
            return Err(RuleError::new("No such hex"));
        }
        if hex == self.robber_hex {
            return Err(RuleError::new("The robber is already there"));
        }
        self.robber_hex = hex;
        let terrain = match self.board.hexes[hex].terrain.resource() {
            Some(r) => r.name(),
            None => "Desert",
        };
        let number = self.board.hexes[hex].number;
        let name = self.players[actor].name.clone();
        let at = match number {
            Some(n) => format!("{terrain} ({n})"),
            None => terrain.to_string(),
        };
        self.push_log(Some(actor), format!("{name} moves the robber to the {at}."));

        let candidates = self.steal_candidates(hex, actor);
        match candidates.len() {
            0 => {
                self.phase = Phase::Play;
                let _ = after_knight;
            }
            1 => {
                // Exactly one victim: skip the chooser and steal automatically.
                // The phase must be `Steal` for `steal_from` to accept the call.
                self.phase = Phase::Steal { hex };
                self.steal_from(actor, Some(candidates[0]))?;
            }
            _ => {
                self.steal_hex = Some(hex);
                self.phase = Phase::Steal { hex };
            }
        }
        Ok(())
    }

    fn steal_from(&mut self, actor: PlayerId, victim: Option<PlayerId>) -> RuleResult {
        if actor != self.current {
            return Err(RuleError::new("Not your turn"));
        }
        let hex = match self.phase {
            Phase::Steal { hex } => hex,
            _ => return Err(RuleError::new("Nothing to steal right now")),
        };
        let Some(victim) = victim else {
            self.steal_hex = None;
            self.phase = Phase::Play;
            return Ok(());
        };
        let candidates = self.steal_candidates(hex, actor);
        if !candidates.contains(&victim) {
            return Err(RuleError::new("That player has no building there"));
        }
        // A robbed bot holds a grudge against the robber.
        if self.players[victim].is_bot {
            self.add_anger(victim, actor);
        }
        let stolen = self.take_random_resource(victim);
        let (vname, aname) = (
            self.players[victim].name.clone(),
            self.players[actor].name.clone(),
        );
        match stolen {
            Some(res) => {
                self.players[actor].resources.add(res, 1);
                self.push_log(
                    Some(actor),
                    format!("{aname} steals a card from {vname}."),
                );
            }
            None => {
                self.push_log(
                    Some(actor),
                    format!("{aname} finds nothing to steal from {vname}."),
                );
            }
        }
        self.steal_hex = None;
        self.phase = Phase::Play;
        Ok(())
    }

    fn take_random_resource(&mut self, victim: PlayerId) -> Option<Resource> {
        use rand::RngExt;
        let total = self.players[victim].resources.total();
        if total == 0 {
            return None;
        }
        let mut pick = self.rng.random_range(0..total);
        for &r in ALL_RESOURCES.iter() {
            let c = self.players[victim].resources.get(r);
            if pick < c {
                self.players[victim].resources.take(r, 1);
                return Some(r);
            }
            pick -= c;
        }
        None
    }

    /// Opponents with a building adjacent to `hex` (excluding `actor`).
    pub fn steal_candidates(&self, hex: usize, actor: PlayerId) -> Vec<PlayerId> {
        let mut out: Vec<PlayerId> = Vec::new();
        for &v in &self.board.hexes[hex].vertices {
            if let Some(owner) = self.board.vertices[v].owner {
                if owner != actor
                    && self.board.vertices[v].building != Building::None
                    && !out.contains(&owner)
                {
                    out.push(owner);
                }
            }
        }
        out
    }

    fn discard(&mut self, actor: PlayerId, resources: Bundle) -> RuleResult {
        if !matches!(self.phase, Phase::Discard) {
            return Err(RuleError::new("No discards are pending"));
        }
        if !self.pending_discards.contains(&actor) {
            return Err(RuleError::new("You do not need to discard"));
        }
        let hand = self.players[actor].resources;
        let total = hand.total();
        let required = total / 2;
        if resources.total() != required {
            return Err(RuleError::new(format!(
                "You must discard exactly {required} cards"
            )));
        }
        if !resources.can_pay(&hand) {
            return Err(RuleError::new("You do not have those cards"));
        }
        resources.pay(&mut self.players[actor].resources);
        for &r in ALL_RESOURCES.iter() {
            self.bank_give(r, resources.get(r));
        }
        self.pending_discards.retain(|&p| p != actor);
        let name = self.players[actor].name.clone();
        self.push_log(Some(actor), format!("{name} discards {required} cards."));
        if self.pending_discards.is_empty() {
            self.phase = Phase::MoveRobber { after_knight: false };
        }
        Ok(())
    }

    // =====================================================================
    // Trading
    // =====================================================================

    pub fn best_maritime_ratio(&self, pid: PlayerId, resource: Resource) -> u8 {
        let mut best = 4u8;
        for v in &self.board.vertices {
            if v.owner == Some(pid) && v.building != Building::None {
                if let Some(port) = v.port {
                    let ratio = match port.resource() {
                        Some(r) if r == resource => 2,
                        Some(_) => 4,
                        None => 3,
                    };
                    best = best.min(ratio);
                }
            }
        }
        best
    }

    fn bank_trade(&mut self, actor: PlayerId, give: Resource, want: Resource) -> RuleResult {
        self.require_play_turn(actor)?;
        if give == want {
            return Err(RuleError::new("Choose two different resources"));
        }
        let ratio = self.best_maritime_ratio(actor, give);
        if !self.players[actor].resources.has(give, ratio) {
            return Err(RuleError::new(format!(
                "You need {ratio} {} to trade",
                give.name()
            )));
        }
        if self.bank.get(want) < 1 {
            return Err(RuleError::new("The bank is out of that resource"));
        }
        self.players[actor].resources.take(give, ratio);
        self.bank.set(give, self.bank.get(give).saturating_add(ratio));
        self.bank_take(want, 1);
        self.players[actor].resources.add(want, 1);
        let name = self.players[actor].name.clone();
        self.push_log(
            Some(actor),
            format!("{name} trades {ratio} {} for 1 {}.", give.name(), want.name()),
        );
        Ok(())
    }

    fn propose_trade(&mut self, actor: PlayerId, give: Bundle, want: Bundle) -> RuleResult {
        self.require_play_turn(actor)?;
        if self.trade.is_some() {
            return Err(RuleError::new("A trade is already on the table"));
        }
        if give.is_empty() || want.is_empty() {
            return Err(RuleError::new("Offer must exchange resources on both sides"));
        }
        if !give.can_pay(&self.players[actor].resources) {
            return Err(RuleError::new("You do not have those cards to offer"));
        }
        // A player can only accept if they can actually pay what the proposer
        // wants, so anyone who cannot is auto-declined up front instead of
        // being shown an Accept button they can never use.
        let responses: Vec<(PlayerId, TradeResponse)> = (0..self.players.len())
            .filter(|&p| p != actor)
            .map(|p| {
                let can = want.can_pay(&self.players[p].resources);
                (p, if can { TradeResponse::Pending } else { TradeResponse::Declined })
            })
            .collect();
        if responses.iter().all(|(_, r)| *r == TradeResponse::Declined) {
            return Err(RuleError::new("No other player has those resources"));
        }
        self.trade_seq += 1;
        let id = self.trade_seq;
        self.trade = Some(TradeOffer {
            from: actor,
            give,
            want,
            responses,
            deadline_ms: 0,
            id,
        });
        let name = self.players[actor].name.clone();
        self.push_log(Some(actor), format!("{name} proposes a trade."));
        Ok(())
    }

    fn respond_trade(&mut self, actor: PlayerId, accept: bool) -> RuleResult {
        if !matches!(self.phase, Phase::Play) {
            return Err(RuleError::new("Not in a trading phase"));
        }
        let Some(offer) = self.trade.as_mut() else {
            return Err(RuleError::new("There is no trade to respond to"));
        };
        if actor == offer.from {
            return Err(RuleError::new("You cannot respond to your own offer"));
        }
        let Some(entry) = offer.responses.iter_mut().find(|(p, _)| *p == actor) else {
            return Err(RuleError::new("You are not part of this trade"));
        };
        if entry.1 != TradeResponse::Pending {
            return Err(RuleError::new("You already responded"));
        }
        let from = offer.from;
        let give = offer.give;
        let want = offer.want;
        if accept {
            // Validate both sides can still fulfil.
            if !give.can_pay(&self.players[from].resources)
                || !want.can_pay(&self.players[actor].resources)
            {
                offer.responses.iter_mut().find(|(p, _)| *p == actor).unwrap().1 =
                    TradeResponse::Declined;
                return Err(RuleError::new("The trade can no longer be completed"));
            }
            give.pay(&mut self.players[from].resources);
            give.collect(&mut self.players[actor].resources);
            want.pay(&mut self.players[actor].resources);
            want.collect(&mut self.players[from].resources);
            let (fname, tname) = (
                self.players[from].name.clone(),
                self.players[actor].name.clone(),
            );
            self.trade = None;
            self.push_log(
                Some(from),
                format!("{fname} and {tname} complete a trade."),
            );
        } else {
            entry.1 = TradeResponse::Declined;
            let all_rejected = self
                .trade
                .as_ref()
                .map(|o| o.responses.iter().all(|(_, r)| *r == TradeResponse::Declined))
                .unwrap_or(false);
            if all_rejected {
                self.trade = None;
                self.push_log(None, "The trade offer was declined.");
            }
        }
        Ok(())
    }

    fn cancel_trade(&mut self, actor: PlayerId) -> RuleResult {
        match &self.trade {
            Some(o) if o.from == actor => {
                self.trade = None;
                self.push_log(Some(actor), "The trade offer was withdrawn.");
                Ok(())
            }
            _ => Err(RuleError::new("You have no offer to cancel")),
        }
    }

    // =====================================================================
    // End turn
    // =====================================================================

    fn end_turn(&mut self, actor: PlayerId) -> RuleResult {
        self.require_play_turn(actor)?;
        if self.dice.is_none() {
            return Err(RuleError::new("You must roll the dice before ending your turn"));
        }
        self.finish_turn(actor);
        Ok(())
    }

    /// End the current player's turn even if they never rolled. Used by the
    /// per-turn timer. No-op outside the play phase.
    pub fn force_end_turn(&mut self) {
        if !matches!(self.phase, Phase::Play) {
            return;
        }
        let actor = self.current;
        self.finish_turn(actor);
        self.push_log(None, "Turn ended by timer.");
    }

    fn finish_turn(&mut self, actor: PlayerId) {
        // Move newly bought development cards into the playable set.
        let new_cards = std::mem::take(&mut self.players[actor].new_dev_cards);
        self.players[actor].dev_cards.extend(new_cards);

        // A bot cools off a little at the end of each of its own turns.
        if self.players[actor].is_bot {
            self.decay_anger(actor);
        }

        self.trade = None;
        self.free_roads_left = 0;
        self.dev_bought_this_turn = false;
        self.played_dev_this_turn = false;
        self.dice = None;
        self.current = (self.current + 1) % self.players.len();
        self.turn += 1;
        self.phase = Phase::Play;
        let name = self.players[self.current].name.clone();
        self.push_log(None, format!("{name}'s turn."));
    }

    // =====================================================================
    // Validation helpers
    // =====================================================================

    fn require_play_turn(&self, actor: PlayerId) -> RuleResult {
        if !matches!(self.phase, Phase::Play) {
            return Err(RuleError::new("That action is not available right now"));
        }
        if actor != self.current {
            return Err(RuleError::new("It is not your turn"));
        }
        Ok(())
    }

    fn validate_settlement_spot(&self, vertex: usize) -> RuleResult {
        if vertex >= self.board.vertices.len() {
            return Err(RuleError::new("No such intersection"));
        }
        if self.board.vertices[vertex].building != Building::None {
            return Err(RuleError::new("That intersection is occupied"));
        }
        for e in &self.board.vertices[vertex].edges {
            let other = self.board.edges[*e].other(vertex).unwrap();
            if self.board.vertices[other].building != Building::None {
                return Err(RuleError::new("Too close to another settlement"));
            }
        }
        Ok(())
    }

    pub(crate) fn vertex_has_own_road(&self, pid: PlayerId, vertex: usize) -> bool {
        self.board.vertices[vertex]
            .edges
            .iter()
            .any(|&e| self.board.edges[e].owner == Some(pid))
    }

    pub(crate) fn edge_connects_to_network(&self, pid: PlayerId, edge: usize) -> bool {
        let e = &self.board.edges[edge];
        [e.a, e.b].iter().any(|&v| {
            let vt = &self.board.vertices[v];
            vt.owner == Some(pid) || vt.edges.iter().any(|&ei| self.board.edges[ei].owner == Some(pid))
        })
    }

    /// Longest road length currently visible for a player (helper for UI/bot).
    pub fn longest_road_of(&self, pid: PlayerId) -> u8 {
        longest_road_length(&self.board, pid)
    }

    // --- Legal-move enumeration (used by the web UI overrides) -----------

    pub fn legal_settlement_vertices(&self, pid: PlayerId) -> Vec<usize> {
        if self.players[pid].settlements_left == 0 {
            return Vec::new();
        }
        let setup = matches!(self.phase, Phase::Setup);
        (0..self.board.vertices.len())
            .filter(|&v| {
                self.validate_settlement_spot(v).is_ok()
                    && (setup || self.vertex_has_own_road(pid, v))
            })
            .collect()
    }

    pub fn legal_city_vertices(&self, pid: PlayerId) -> Vec<usize> {
        if !self.has_piece_available(pid, Building::City) {
            return Vec::new();
        }
        (0..self.board.vertices.len())
            .filter(|&v| {
                let vt = &self.board.vertices[v];
                vt.owner == Some(pid) && vt.building == Building::Settlement
            })
            .collect()
    }

    pub fn legal_road_edges(&self, pid: PlayerId) -> Vec<usize> {
        if self.players[pid].roads_left == 0 {
            return Vec::new();
        }
        match self.phase {
            Phase::Setup => match self.setup_road_from {
                Some(from) => (0..self.board.edges.len())
                    .filter(|&e| {
                        self.board.edges[e].owner.is_none()
                            && self.board.edges[e].other(from).is_some()
                    })
                    .collect(),
                None => Vec::new(),
            },
            _ => (0..self.board.edges.len())
                .filter(|&e| {
                    self.board.edges[e].owner.is_none()
                        && self.edge_connects_to_network(pid, e)
                })
                .collect(),
        }
    }

    pub fn legal_robber_hexes(&self) -> Vec<usize> {
        (0..self.board.hexes.len())
            .filter(|&h| h != self.robber_hex)
            .collect()
    }
}
