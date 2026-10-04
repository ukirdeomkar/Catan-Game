use serde::{Deserialize, Serialize};

/// The five tradeable resources.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Resource {
    Wood = 0,
    Brick = 1,
    Wheat = 2,
    Ore = 3,
    Sheep = 4,
}

pub const ALL_RESOURCES: [Resource; 5] = [
    Resource::Wood,
    Resource::Brick,
    Resource::Wheat,
    Resource::Ore,
    Resource::Sheep,
];

impl Resource {
    pub fn as_usize(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Resource::Wood => "Wood",
            Resource::Brick => "Brick",
            Resource::Wheat => "Wheat",
            Resource::Ore => "Ore",
            Resource::Sheep => "Sheep",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Resource::Wood => "wood",
            Resource::Brick => "brick",
            Resource::Wheat => "wheat",
            Resource::Ore => "ore",
            Resource::Sheep => "sheep",
        }
    }

    pub fn from_slug(s: &str) -> Option<Resource> {
        Some(match s {
            "wood" => Resource::Wood,
            "brick" => Resource::Brick,
            "wheat" => Resource::Wheat,
            "ore" => Resource::Ore,
            "sheep" => Resource::Sheep,
            _ => return None,
        })
    }
}

/// A player's resource cards, indexed by `Resource as usize`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct ResourceHand(pub [u8; 5]);

impl ResourceHand {
    pub fn get(&self, r: Resource) -> u8 {
        self.0[r.as_usize()]
    }

    pub fn set(&mut self, r: Resource, n: u8) {
        self.0[r.as_usize()] = n;
    }

    pub fn add(&mut self, r: Resource, n: u8) {
        self.0[r.as_usize()] = self.0[r.as_usize()].saturating_add(n);
    }

    /// Returns false (and changes nothing) if the hand lacks the cards.
    pub fn take(&mut self, r: Resource, n: u8) -> bool {
        let i = r.as_usize();
        if self.0[i] < n {
            return false;
        }
        self.0[i] -= n;
        true
    }

    pub fn has(&self, r: Resource, n: u8) -> bool {
        self.0[r.as_usize()] >= n
    }

    pub fn total(&self) -> u8 {
        self.0.iter().copied().fold(0u8, |a, b| a.saturating_add(b))
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
}

/// A bundle of resources (used for costs and trade offers).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Bundle(pub [u8; 5]);

impl Bundle {
    pub fn of(r: Resource, n: u8) -> Bundle {
        let mut b = Bundle::default();
        b.0[r.as_usize()] = n;
        b
    }

    pub fn get(&self, r: Resource) -> u8 {
        self.0[r.as_usize()]
    }

    pub fn with(mut self, r: Resource, n: u8) -> Bundle {
        self.0[r.as_usize()] = n;
        self
    }

    pub fn total(&self) -> u8 {
        self.0.iter().copied().sum()
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }

    pub fn can_pay(&self, hand: &ResourceHand) -> bool {
        ALL_RESOURCES
            .iter()
            .all(|&r| hand.get(r) >= self.get(r))
    }

    pub fn pay(&self, hand: &mut ResourceHand) -> bool {
        if !self.can_pay(hand) {
            return false;
        }
        for &r in ALL_RESOURCES.iter() {
            hand.take(r, self.get(r));
        }
        true
    }

    pub fn collect(&self, hand: &mut ResourceHand) {
        for &r in ALL_RESOURCES.iter() {
            hand.add(r, self.get(r));
        }
    }

    pub fn iter_nonzero(&self) -> impl Iterator<Item = (Resource, u8)> + '_ {
        ALL_RESOURCES
            .iter()
            .copied()
            .filter(move |&r| self.get(r) > 0)
            .map(move |r| (r, self.get(r)))
    }
}

/// Build costs.
pub const COST_ROAD: Bundle = Bundle([1, 1, 0, 0, 0]); // wood, brick
pub const COST_SETTLEMENT: Bundle = Bundle([1, 1, 1, 0, 1]); // wood, brick, wheat, (ore), sheep
pub const COST_CITY: Bundle = Bundle([0, 0, 2, 3, 0]); // wheat x2, ore x3
pub const COST_DEV: Bundle = Bundle([0, 0, 1, 1, 1]); // wheat, ore, sheep

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum DevCard {
    Knight,
    VictoryPoint,
    RoadBuilding,
    YearOfPlenty,
    Monopoly,
}

impl DevCard {
    pub fn name(self) -> &'static str {
        match self {
            DevCard::Knight => "Knight",
            DevCard::VictoryPoint => "Victory Point",
            DevCard::RoadBuilding => "Road Building",
            DevCard::YearOfPlenty => "Year of Plenty",
            DevCard::Monopoly => "Monopoly",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            DevCard::Knight => "Move the robber and steal one resource from an adjacent player. Counts toward Largest Army.",
            DevCard::VictoryPoint => "Worth 1 victory point. Kept hidden until the end.",
            DevCard::RoadBuilding => "Place two roads for free.",
            DevCard::YearOfPlenty => "Take any two resources from the bank.",
            DevCard::Monopoly => "Name a resource; every other player gives you all of their cards of that type.",
        }
    }
}

/// The standard 25-card development deck.
pub fn standard_dev_deck() -> Vec<DevCard> {
    let mut deck = Vec::with_capacity(25);
    deck.extend(std::iter::repeat(DevCard::Knight).take(14));
    deck.extend(std::iter::repeat(DevCard::VictoryPoint).take(5));
    deck.extend(std::iter::repeat(DevCard::RoadBuilding).take(2));
    deck.extend(std::iter::repeat(DevCard::YearOfPlenty).take(2));
    deck.extend(std::iter::repeat(DevCard::Monopoly).take(2));
    deck
}
