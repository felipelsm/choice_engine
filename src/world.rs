// The world: creatures, bushes, trees, water, and piles of dropped things.
//
// There are no "farm" or "build" actions. Instead the physics are:
// - food dropped on the ground forms a pile. Piles slowly spoil, and now and
//   then a food pile sprouts into a new bush (that's how farming can emerge)
// - wood dropped on the ground forms a pile. A big enough wood pile works as
//   a shelter: warm at night and better rest (that's how building can emerge)
// - every creature has a home (where it was born). Whether anyone ever learns
//   to bring things home and drop them there is up to evolution.

use crate::creature::{
    random_personality, Action, Creature, Item, Perception, Personality, CARRY_CAP, N_ACTIONS, N_STATES,
};
use crate::culture::{Culture, Discovery, Memory};
use crate::font::draw_text;
use crate::render::Renderer;

/// Seconds between decisions.
pub const DECISION_INTERVAL: f32 = 1.0;

// --- body ---
const METABOLISM_EVERY: u64 = 5; // every this many ticks...
const METABOLISM: f32 = 0.10; //    ...lose this much food and water
const EAT_RATE: f32 = 0.25; //      per second
const DRINK_RATE: f32 = 0.25;
const BOREDOM_RATE: f32 = 0.05;
const REST_RATE: f32 = 0.15;
const SHELTER_REST_BONUS: f32 = 2.0; // resting near a shelter is this much faster

// --- health ---
const STARVE_DAMAGE: f32 = 0.05; //     per second while food OR water is empty
const EXHAUSTION_DAMAGE: f32 = 0.03; // per second while boredom is maxed
const COLD_DAMAGE: f32 = 0.01; //       per second at night away from shelter
const HEAL_RATE: f32 = 0.02; //         per second while none of the above
const HEAL_ABOVE: f32 = 0.5; //         ...and food and water are above this

// --- resources ---
const WILD_BUSHES: usize = 6;
const WILD_MAX: f32 = 8.0; //      a full wild bush feeds 8 full stomachs
const WILD_REGROW: f32 = 0.2; //   per second
const PLANTED_MAX: f32 = 4.0; //   bushes that sprouted from piles are smaller
const PLANTED_REGROW: f32 = 0.1;
const MAX_BUSHES: usize = 40;
const TREES: usize = 4;
const TREE_MAX: f32 = 10.0;
const TREE_REGROW: f32 = 0.15;
const WATER_SOURCES: usize = 4; //  water never runs out
const GATHER_RATE: f32 = 0.4; //    picked up per second

// --- piles (the physics farming and building can emerge from) ---
const PILE_MERGE: f32 = 20.0; //     drops this close to a pile join it
const FOOD_SPOIL: f32 = 0.003; //    food lost per second from a pile
const WOOD_DECAY: f32 = 0.001; //    wood lost per second from a pile
const SPROUT_CHANCE: f32 = 0.03; //  per second, for a food pile of at least SPROUT_COST
const SPROUT_COST: f32 = 0.2; //     food used up by a sprout
const SHELTER_WOOD: f32 = 1.0; //    wood in one pile to count as a shelter
const PANTRY_AGE: f32 = 30.0; //     a food pile must be this old (seconds) to count as stored food
const SHELTER_RADIUS: f32 = 50.0;

// --- social ---
const NEAR: f32 = 40.0; //   "next to me" radius
const VIEW: f32 = 150.0; //  how far creatures can see family
const SHARE_AMOUNT: f32 = 0.15;
const STEAL_AMOUNT: f32 = 0.2;
const STEAL_SELF_DAMAGE: f32 = 0.03;
const STEAL_VICTIM_DAMAGE: f32 = 0.06;
const LONELY_RATE: f32 = 0.03;
const UNLONELY_RATE: f32 = 0.10;
const FEAR_DECAY: f32 = 0.05;

// --- time ---
pub const DAY_LENGTH: u64 = 60; // ticks per day
const NIGHT_START: u64 = 36; //    ticks into the day when night begins

// --- history sampling (for the evolution viewer) ---
pub const FAMILY_SAMPLE_EVERY: u64 = 5;
pub const PERSONALITY_SAMPLE_EVERY: u64 = 10;

/// Safety cap. Food is what should limit population.
pub const MAX_POPULATION: usize = 1000;

const SPEED: f32 = 60.0; // pixels per second
const REACH: f32 = 12.0; // close enough to use something
const HOME_REACH: f32 = 20.0;
const FAR: f32 = 400.0; //  distance that counts as "1.0 far" in perception
const HOME_SCALE: f32 = 100.0; // NEAR_HOME goes from 1 at home to 0 at this distance

pub struct Bush {
    pub x: f32,
    pub y: f32,
    pub amount: f32,
    pub max: f32,
    pub regrow: f32,
    pub planted: bool, // sprouted from a dropped food pile
}

pub struct Tree {
    pub x: f32,
    pub y: f32,
    pub amount: f32,
}

pub struct Water {
    pub x: f32,
    pub y: f32,
}

pub struct Pile {
    pub x: f32,
    pub y: f32,
    pub item: Item,
    pub amount: f32,
    // provenance: who dropped onto it last, and what they remembered doing
    pub family: u32,
    pub dropper: (u64, u32), // (id, generation)
    pub memory: Memory,
    pub credited_day: [u64; 2], // last day Warmth / Pantry paid off from this pile
    pub age: f32,               // seconds since it was started
}

impl Pile {
    fn new(x: f32, y: f32, item: Item, amount: f32, c: &Creature) -> Self {
        Self {
            x,
            y,
            item,
            amount,
            family: c.family,
            dropper: (c.id, c.generation),
            memory: c.memory.clone(),
            credited_day: [u64::MAX; 2],
            age: 0.0,
        }
    }
}

impl Pile {
    pub fn is_shelter(&self) -> bool {
        self.item == Item::Wood && self.amount >= SHELTER_WOOD
    }
}

/// A pile paid off for someone; resolved into culture after the step.
enum Payoff {
    Sprouted { pile: usize },
    KeptWarm { pile: usize, family: u32 },
    Fed { pile: usize, family: u32 },
}

/// Where a creature can get food from.
#[derive(Clone, Copy)]
enum FoodSpot {
    Bush(usize),
    Pile(usize),
}

/// Why creatures died, for stats.
#[derive(Clone, Copy)]
pub enum DeathCause {
    OldAge,
    Starved,
    Exhausted,
    Cold,
    Fight,
}

pub const DEATH_CAUSES: [(DeathCause, &str); 5] = [
    (DeathCause::OldAge, "AGE"),
    (DeathCause::Starved, "STARVE"),
    (DeathCause::Exhausted, "TIRED"),
    (DeathCause::Cold, "COLD"),
    (DeathCause::Fight, "FIGHT"),
];

/// Per-creature info gathered at the start of a tick.
#[derive(Default)]
struct Senses {
    share_target: Option<usize>,
    steal_target: Option<usize>,
    kin_center: Option<(f32, f32)>,
}

enum Interaction {
    Share { from: usize, to: usize },
    Steal { thief: usize, victim: usize },
}

pub struct World {
    pub width: f32,
    pub height: f32,
    pub creatures: Vec<Creature>,
    pub bushes: Vec<Bush>,
    pub trees: Vec<Tree>,
    pub water: Vec<Water>,
    pub piles: Vec<Pile>,
    pub ticks: u64,
    pub births: usize,
    pub deaths_by: [usize; 5], // indexed like DEATH_CAUSES
    pub sprouts: usize,        // bushes that grew from dropped food, ever
    pub mistakes: usize,       // rash picks of the runner-up, ever
    pub action_totals: [u64; N_ACTIONS], // how often each action was chosen, ever
    pub n_families: usize,
    pub culture: Culture,

    // history for graphs
    pub pop_history: Vec<usize>,                // every tick
    pub family_history: Vec<Vec<u32>>,          // every FAMILY_SAMPLE_EVERY ticks
    pub personality_history: Vec<Personality>,  // every PERSONALITY_SAMPLE_EVERY ticks
    pub caution_history: Vec<f32>,              // same timing as personality_history

    decision_timer: f32,
    next_id: u64,
}

impl World {
    pub fn new(width: f32, height: f32, n_creatures: usize) -> Self {
        let bushes = (0..WILD_BUSHES)
            .map(|_| Bush {
                x: rand_in(width),
                y: rand_in(height),
                amount: WILD_MAX,
                max: WILD_MAX,
                regrow: WILD_REGROW,
                planted: false,
            })
            .collect();
        let trees = (0..TREES)
            .map(|_| Tree { x: rand_in(width), y: rand_in(height), amount: TREE_MAX })
            .collect();
        let water = (0..WATER_SOURCES).map(|_| Water { x: rand_in(width), y: rand_in(height) }).collect();

        // every founder starts its own family, with a random caution gene
        let creatures: Vec<Creature> = (0..n_creatures)
            .map(|i| {
                Creature::new(i as u64, rand_in(width), rand_in(height), i as u32, random_personality(), fastrand::f32())
            })
            .collect();

        let mut w = Self {
            width,
            height,
            creatures,
            bushes,
            trees,
            water,
            piles: Vec::new(),
            ticks: 0,
            births: 0,
            deaths_by: [0; 5],
            sprouts: 0,
            mistakes: 0,
            action_totals: [0; N_ACTIONS],
            n_families: n_creatures,
            culture: Culture::new(n_creatures),
            pop_history: Vec::new(),
            family_history: Vec::new(),
            personality_history: Vec::new(),
            caution_history: Vec::new(),
            decision_timer: 0.0,
            next_id: n_creatures as u64,
        };
        w.record_history();
        w
    }

    pub fn deaths(&self) -> usize {
        self.deaths_by.iter().sum()
    }

    pub fn is_night(&self) -> bool {
        self.ticks % DAY_LENGTH >= NIGHT_START
    }

    pub fn day(&self) -> u64 {
        self.ticks / DAY_LENGTH + 1
    }

    /// Advance the world by `dt` seconds. Returns true if a decision tick happened.
    pub fn update(&mut self, dt: f32) -> bool {
        self.decision_timer += dt;
        let mut ticked = false;
        while self.decision_timer >= DECISION_INTERVAL {
            self.decision_timer -= DECISION_INTERVAL;
            self.tick();
            ticked = true;
        }
        self.step(dt);
        ticked
    }

    // -----------------------------------------------------------------------
    // Decision tick: perceive, decide, do instant actions
    // -----------------------------------------------------------------------

    fn tick(&mut self) {
        self.ticks += 1;

        for c in &mut self.creatures {
            c.age += 1.0;
            c.cooldown -= 1.0;
            if self.ticks % METABOLISM_EVERY == 0 {
                c.food = (c.food - METABOLISM).max(0.0);
                c.water = (c.water - METABOLISM).max(0.0);
            }
        }

        let senses = self.perceive();

        let population = self.creatures.len();
        let mut babies = Vec::new();
        let mut interactions = Vec::new();

        for i in 0..self.creatures.len() {
            let possible = self.possible_actions(i, &senses[i]);
            let learned = *self.culture.learned(self.creatures[i].family);
            self.creatures[i].decide(&possible, &learned);
            if self.creatures[i].mistake {
                self.mistakes += 1;
            }
            self.action_totals[self.creatures[i].action.index()] += 1;

            match self.creatures[i].action {
                Action::Reproduce => {
                    if population + babies.len() < MAX_POPULATION {
                        babies.push(self.creatures[i].reproduce(self.next_id));
                        self.next_id += 1;
                    }
                }
                Action::Share => {
                    if let Some(to) = senses[i].share_target {
                        interactions.push(Interaction::Share { from: i, to });
                    }
                }
                Action::Steal => {
                    if let Some(victim) = senses[i].steal_target {
                        interactions.push(Interaction::Steal { thief: i, victim });
                    }
                }
                Action::Explore => {
                    let c = &self.creatures[i];
                    let tx = (c.x + fastrand::f32() * 300.0 - 150.0).clamp(10.0, self.width - 10.0);
                    let ty = (c.y + fastrand::f32() * 300.0 - 150.0).clamp(10.0, self.height - 10.0);
                    self.creatures[i].target = Some((tx, ty));
                }
                Action::Follow => self.creatures[i].target = senses[i].kin_center,
                Action::Flee => {
                    let c = &self.creatures[i];
                    if let Some((tx, ty)) = c.threat {
                        let (dx, dy) = (c.x - tx, c.y - ty);
                        let d = (dx * dx + dy * dy).sqrt().max(1.0);
                        let fx = (c.x + dx / d * 120.0).clamp(10.0, self.width - 10.0);
                        let fy = (c.y + dy / d * 120.0).clamp(10.0, self.height - 10.0);
                        self.creatures[i].target = Some((fx, fy));
                    }
                }
                _ => {}
            }
        }

        for it in interactions {
            match it {
                Interaction::Share { from, to } => self.share(from, to),
                Interaction::Steal { thief, victim } => self.steal(thief, victim),
            }
        }

        self.births += babies.len();
        self.creatures.extend(babies);
        self.culture.forget();
        self.record_history();
    }

    fn record_history(&mut self) {
        self.pop_history.push(self.creatures.len());
        if self.ticks % FAMILY_SAMPLE_EVERY == 0 {
            let mut counts = vec![0u32; self.n_families];
            for c in &self.creatures {
                counts[c.family as usize] += 1;
            }
            self.family_history.push(counts);
        }
        if self.ticks % PERSONALITY_SAMPLE_EVERY == 0 {
            if let Some(avg) = self.average_personality() {
                self.personality_history.push(avg);
                self.caution_history.push(self.average_caution());
            }
        }
    }

    /// Fill in each creature's Perception and collect targets. O(n^2), which
    /// is fine for a few hundred creatures; use a grid if it gets slow.
    fn perceive(&mut self) -> Vec<Senses> {
        let n = self.creatures.len();
        let mut senses: Vec<Senses> = (0..n).map(|_| Senses::default()).collect();
        let night = self.is_night();

        for i in 0..n {
            let c = &self.creatures[i];
            let (mut near, mut kin_near) = (0usize, 0usize);
            let (mut kin_sum_x, mut kin_sum_y, mut kin_seen) = (0.0, 0.0, 0usize);
            let mut hungriest: Option<(usize, f32)> = None;
            let mut nearest_stranger: Option<(usize, f32)> = None;

            for j in 0..n {
                if i == j {
                    continue;
                }
                let o = &self.creatures[j];
                let d = dist(c.x, c.y, o.x, o.y);
                let kin = o.family == c.family;
                if d <= NEAR {
                    near += 1;
                    if kin {
                        kin_near += 1;
                    }
                    if hungriest.is_none_or(|(_, f)| o.food < f) {
                        hungriest = Some((j, o.food));
                    }
                    let has_food = o.carrying(Item::Food) > 0.05 || o.food > 0.2;
                    if !kin && has_food && nearest_stranger.is_none_or(|(_, nd)| d < nd) {
                        nearest_stranger = Some((j, d));
                    }
                }
                if kin && d <= VIEW {
                    kin_sum_x += o.x;
                    kin_sum_y += o.y;
                    kin_seen += 1;
                }
            }

            let kin_center = (kin_seen > 0)
                .then(|| (kin_sum_x / kin_seen as f32, kin_sum_y / kin_seen as f32))
                .filter(|&(kx, ky)| dist(c.x, c.y, kx, ky) > 15.0);

            let bush_d = self.bushes.iter().filter(|b| b.amount > 0.5).map(|b| dist(c.x, c.y, b.x, b.y));
            let pile_d = self
                .piles
                .iter()
                .filter(|p| p.item == Item::Food && p.amount > 0.2 && p.family == c.family)
                .map(|p| dist(c.x, c.y, p.x, p.y));
            let food_far = bush_d.chain(pile_d).fold(f32::INFINITY, f32::min);

            let home_d = dist(c.x, c.y, c.home.0, c.home.1);
            let cold = night && !self.sheltered(c.x, c.y);

            senses[i] = Senses {
                share_target: hungriest.map(|(j, _)| j),
                steal_target: nearest_stranger.map(|(j, _)| j),
                kin_center,
            };

            let c = &mut self.creatures[i];
            c.perception = Perception {
                food_far: (food_far / FAR).min(1.0),
                crowding: (near as f32 / 8.0).min(1.0),
                kin_near: (kin_near as f32 / 5.0).min(1.0),
                near_home: 1.0 - (home_d / HOME_SCALE).min(1.0),
                cold: if cold { 1.0 } else { 0.0 },
            };
        }
        senses
    }

    fn possible_actions(&self, i: usize, s: &Senses) -> [bool; N_ACTIONS] {
        let c = &self.creatures[i];
        let any_bush = self.bushes.iter().any(|b| b.amount > 0.05);
        let any_food_pile = self
            .piles
            .iter()
            .any(|p| p.item == Item::Food && p.amount > 0.05 && p.family == c.family);
        let any_tree = self.trees.iter().any(|t| t.amount > 0.05);
        let room = c.carried() < CARRY_CAP - 0.05;
        let hands_free_for = |item: Item| c.carry.is_none_or(|(i, _)| i == item);
        let food_in_hand = c.carrying(Item::Food);

        let mut p = [false; N_ACTIONS];
        p[Action::Eat.index()] = food_in_hand > 0.01 || any_bush || any_food_pile;
        p[Action::Drink.index()] = true;
        p[Action::Rest.index()] = true;
        p[Action::Reproduce.index()] = c.can_reproduce();
        p[Action::Explore.index()] = true;
        p[Action::Follow.index()] = s.kin_center.is_some();
        p[Action::Share.index()] = s.share_target.is_some() && (food_in_hand >= SHARE_AMOUNT || c.food > 0.6);
        p[Action::Steal.index()] = s.steal_target.is_some();
        p[Action::Flee.index()] = c.threat.is_some() && c.fear > 0.05;
        p[Action::PickFood.index()] = room && hands_free_for(Item::Food) && (any_bush || any_food_pile);
        p[Action::PickWood.index()] = room && hands_free_for(Item::Wood) && any_tree;
        p[Action::Drop.index()] = c.carried() > 0.05;
        p[Action::GoHome.index()] = dist(c.x, c.y, c.home.0, c.home.1) > HOME_REACH;
        p
    }

    fn share(&mut self, from: usize, to: usize) {
        let giver = &mut self.creatures[from];
        if giver.carrying(Item::Food) >= SHARE_AMOUNT {
            take_from_hands(giver, SHARE_AMOUNT);
        } else if giver.food > 0.6 {
            giver.food -= SHARE_AMOUNT;
        } else {
            return;
        }
        feed(&mut self.creatures[to], SHARE_AMOUNT);
        let (f_from, f_to) = (self.creatures[from].family, self.creatures[to].family);
        self.culture.teach(f_from, f_to, self.ticks);
    }

    fn steal(&mut self, thief: usize, victim: usize) {
        let (tx, ty) = (self.creatures[thief].x, self.creatures[thief].y);
        let v = &mut self.creatures[victim];
        let taken = if v.carrying(Item::Food) > 0.05 {
            take_from_hands(v, STEAL_AMOUNT)
        } else {
            let t = v.food.min(STEAL_AMOUNT);
            v.food -= t;
            t
        };
        v.health -= STEAL_VICTIM_DAMAGE;
        v.fear = 1.0;
        v.threat = Some((tx, ty));

        let t = &mut self.creatures[thief];
        t.health -= STEAL_SELF_DAMAGE;
        feed(t, taken);
    }

    fn sheltered(&self, x: f32, y: f32) -> bool {
        self.piles.iter().any(|p| p.is_shelter() && dist(x, y, p.x, p.y) <= SHELTER_RADIUS)
    }

    // -----------------------------------------------------------------------
    // Continuous step: physics of piles, then creatures move and act
    // -----------------------------------------------------------------------

    fn step(&mut self, dt: f32) {
        let night = self.is_night();

        for b in &mut self.bushes {
            b.amount = (b.amount + b.regrow * dt).min(b.max);
        }
        for t in &mut self.trees {
            t.amount = (t.amount + TREE_REGROW * dt).min(TREE_MAX);
        }

        let mut payoffs: Vec<Payoff> = Vec::new();

        // piles spoil / decay; food piles sometimes sprout into bushes
        for (k, p) in self.piles.iter_mut().enumerate() {
            p.age += dt;
            match p.item {
                Item::Food => {
                    p.amount -= FOOD_SPOIL * dt;
                    if p.amount >= SPROUT_COST
                        && self.bushes.len() < MAX_BUSHES
                        && fastrand::f32() < SPROUT_CHANCE * dt
                    {
                        p.amount -= SPROUT_COST;
                        self.bushes.push(Bush {
                            x: p.x + fastrand::f32() * 30.0 - 15.0,
                            y: p.y + fastrand::f32() * 30.0 - 15.0,
                            amount: SPROUT_COST,
                            max: PLANTED_MAX,
                            regrow: PLANTED_REGROW,
                            planted: true,
                        });
                        self.sprouts += 1;
                        payoffs.push(Payoff::Sprouted { pile: k });
                    }
                }
                Item::Wood => p.amount -= WOOD_DECAY * dt,
            }
        }

        let shelters: Vec<(usize, f32, f32)> = self
            .piles
            .iter()
            .enumerate()
            .filter(|(_, p)| p.is_shelter())
            .map(|(k, p)| (k, p.x, p.y))
            .collect();

        let World { creatures, bushes, trees, water, piles, .. } = self;

        for c in creatures.iter_mut() {
            let shelter = shelters.iter().find(|&&(_, sx, sy)| dist(c.x, c.y, sx, sy) <= SHELTER_RADIUS);
            let sheltered = shelter.is_some();
            if let (true, Some(&(k, _, _))) = (night, shelter) {
                payoffs.push(Payoff::KeptWarm { pile: k, family: c.family });
            }
            c.boredom += BOREDOM_RATE * dt;

            if c.hesitation > 0.0 {
                // standing still, thinking it over
                c.hesitation -= dt;
            } else if let Some(pile) = act(c, bushes, trees, water, piles, sheltered, dt) {
                payoffs.push(Payoff::Fed { pile, family: c.family });
            }

            // social feelings
            if c.perception.crowding > 0.0 {
                c.loneliness -= UNLONELY_RATE * dt;
            } else {
                c.loneliness += LONELY_RATE * dt;
            }
            c.fear -= FEAR_DECAY * dt;
            if c.fear <= 0.0 {
                c.threat = None;
            }

            c.food = c.food.clamp(0.0, 1.0);
            c.water = c.water.clamp(0.0, 1.0);
            c.boredom = c.boredom.clamp(0.0, 1.0);
            c.loneliness = c.loneliness.clamp(0.0, 1.0);
            c.fear = c.fear.clamp(0.0, 1.0);

            // health
            let starving = c.food <= 0.0 || c.water <= 0.0;
            let exhausted = c.boredom >= 1.0;
            let cold = night && !sheltered;
            if starving {
                c.health -= STARVE_DAMAGE * dt;
            }
            if exhausted {
                c.health -= EXHAUSTION_DAMAGE * dt;
            }
            if cold {
                c.health -= COLD_DAMAGE * dt;
            }
            if !starving && !exhausted && !cold && c.food > HEAL_ABOVE && c.water > HEAL_ABOVE {
                c.health += HEAL_RATE * dt;
            }
            c.health = c.health.min(1.0);
        }

        self.resolve_payoffs(payoffs);
        self.piles.retain(|p| p.amount > 0.01);

        let mut by = self.deaths_by;
        self.creatures.retain(|c| {
            if !c.is_dead() {
                return true;
            }
            let cause = if c.age >= c.lifespan {
                DeathCause::OldAge
            } else if c.food <= 0.0 || c.water <= 0.0 {
                DeathCause::Starved
            } else if c.boredom >= 1.0 {
                DeathCause::Exhausted
            } else if night && c.perception.cold > 0.0 {
                DeathCause::Cold
            } else {
                DeathCause::Fight
            };
            by[cause as usize] += 1;
            false
        });
        self.deaths_by = by;
    }

    /// Turn real outcomes into culture for the family that caused them.
    /// Warmth and pantry pay off at most once per pile per day, so a family
    /// isn't rewarded 60 times a second for sitting next to one pile.
    fn resolve_payoffs(&mut self, payoffs: Vec<Payoff>) {
        let day = self.day();
        for payoff in payoffs {
            let (k, discovery) = match payoff {
                Payoff::Sprouted { pile } => (pile, Discovery::Seeds),
                Payoff::KeptWarm { pile, family } | Payoff::Fed { pile, family } => {
                    let (d, slot) = match payoff {
                        Payoff::KeptWarm { .. } => (Discovery::Warmth, 0),
                        _ => (Discovery::Pantry, 1),
                    };
                    let p = &mut self.piles[pile];
                    // only the family that built / stored it learns from it,
                    // and food only counts as "stored" once it has sat a while
                    let too_fresh = d == Discovery::Pantry && p.age < PANTRY_AGE;
                    if p.family != family || p.credited_day[slot] == day || too_fresh {
                        continue;
                    }
                    p.credited_day[slot] = day;
                    (pile, d)
                }
            };
            let p = &self.piles[k];
            self.culture.reward(p.family, discovery, &p.memory, self.ticks, p.dropper);
        }
    }

    // -----------------------------------------------------------------------
    // Stats
    // -----------------------------------------------------------------------

    /// Mean of every living creature's personality matrix.
    pub fn average_personality(&self) -> Option<Personality> {
        if self.creatures.is_empty() {
            return None;
        }
        let mut avg = [[0.0; N_STATES]; N_ACTIONS];
        for c in &self.creatures {
            for row in 0..N_ACTIONS {
                for col in 0..N_STATES {
                    avg[row][col] += c.personality[row][col];
                }
            }
        }
        let n = self.creatures.len() as f32;
        for row in avg.iter_mut() {
            for v in row.iter_mut() {
                *v /= n;
            }
        }
        Some(avg)
    }

    pub fn average_caution(&self) -> f32 {
        let n = self.creatures.len().max(1) as f32;
        self.creatures.iter().map(|c| c.caution).sum::<f32>() / n
    }

    pub fn hesitating(&self) -> usize {
        self.creatures.iter().filter(|c| c.hesitation > 0.0).count()
    }

    pub fn max_generation(&self) -> u32 {
        self.creatures.iter().map(|c| c.generation).max().unwrap_or(0)
    }

    pub fn families_alive(&self) -> usize {
        let mut alive = vec![false; self.n_families];
        for c in &self.creatures {
            alive[c.family as usize] = true;
        }
        alive.iter().filter(|&&a| a).count()
    }

    /// How many creatures are doing each action right now.
    pub fn action_counts(&self) -> [usize; N_ACTIONS] {
        let mut counts = [0; N_ACTIONS];
        for c in &self.creatures {
            counts[c.action.index()] += 1;
        }
        counts
    }

    pub fn shelters(&self) -> usize {
        self.piles.iter().filter(|p| p.is_shelter()).count()
    }

    pub fn planted_bushes(&self) -> usize {
        self.bushes.iter().filter(|b| b.planted).count()
    }

    pub fn piled(&self, item: Item) -> f32 {
        self.piles.iter().filter(|p| p.item == item).map(|p| p.amount).sum()
    }

    /// Creature closest to (x, y) within `radius`.
    pub fn creature_at(&self, x: f32, y: f32, radius: f32) -> Option<u64> {
        self.creatures
            .iter()
            .map(|c| (c.id, dist(x, y, c.x, c.y)))
            .filter(|&(_, d)| d <= radius)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
    }

    pub fn creature(&self, id: u64) -> Option<&Creature> {
        self.creatures.iter().find(|c| c.id == id)
    }

    // -----------------------------------------------------------------------
    // Drawing
    // -----------------------------------------------------------------------

    pub fn draw(&self, r: &mut dyn Renderer, color_by_family: bool, selected: Option<u64>) {
        let bg = if self.is_night() { 0x15151f } else { 0x2b2b2b };
        r.fill_rect(0, 0, self.width as i32, self.height as i32, bg);

        for p in &self.piles {
            let (x, y) = (p.x as i32, p.y as i32);
            if p.is_shelter() {
                r.fill_rect(x - 14, y - 6, 28, 16, 0x8d6e63); // walls
                r.fill_rect(x - 17, y - 12, 34, 7, 0x5d4037); // roof
            } else {
                let half = 2 + (p.amount.min(2.0) * 3.0) as i32;
                let color = match p.item {
                    Item::Food => 0xffb74d,
                    Item::Wood => 0x8d6e63,
                };
                r.fill_rect(x - half, y - half, half * 2, half * 2, color);
            }
        }

        for w in &self.water {
            r.fill_rect(w.x as i32 - 9, w.y as i32 - 9, 18, 18, 0x2196f3);
        }

        for t in &self.trees {
            let (x, y) = (t.x as i32, t.y as i32);
            r.fill_rect(x - 2, y, 4, 9, 0x5d4037);
            r.fill_circle(x, y - 2, 3 + (5.0 * t.amount / TREE_MAX) as i32, 0x1b5e20);
        }

        for b in &self.bushes {
            let half = 2 + (6.0 * b.amount / b.max) as i32;
            let color = if b.planted { 0xc0ca33 } else { 0x4caf50 };
            r.fill_rect(b.x as i32 - half, b.y as i32 - half, half * 2, half * 2, color);
        }

        for c in &self.creatures {
            let (x, y) = (c.x as i32, c.y as i32);
            let radius = if c.is_adult() { 6 } else { 4 };

            if selected == Some(c.id) {
                r.fill_circle(x, y, radius + 3, 0xffffff);
                // line to its home
                let (hx, hy) = (c.home.0 as i32, c.home.1 as i32);
                r.fill_rect(hx - 3, hy - 3, 7, 7, 0xffffff);
            }

            let body = if color_by_family { family_color(c.family) } else { c.color() };
            r.fill_circle(x, y, radius, body);
            r.fill_circle(x, y, 2, action_color(c.action));
            match c.carry {
                Some((Item::Food, a)) if a > 0.05 => r.fill_rect(x + radius, y - 1, 3, 3, 0xffb74d),
                Some((Item::Wood, a)) if a > 0.05 => r.fill_rect(x + radius, y - 1, 3, 3, 0xa1887f),
                _ => {}
            }

            // bars above the head: food, water, health (full = good), boredom (full = bad)
            let bars = [(c.food, 0x4caf50), (c.water, 0x2196f3), (c.health, 0xf44336), (c.boredom, 0xe040fb)];
            for (i, &(value, color)) in bars.iter().enumerate() {
                let bx = x - 9 + i as i32 * 5;
                let full = 8;
                let h = (value * full as f32) as i32;
                r.fill_rect(bx, y - 9 - full, 3, full, 0x111111);
                r.fill_rect(bx, y - 9 - h, 3, h, color);
            }

            if c.hesitation > 0.0 {
                draw_text(r, "?", x + 8, y - 20, 1, 0xffffff);
            }
        }
    }
}

/// Run the creature's current action for `dt` seconds.
fn act(
    c: &mut Creature,
    bushes: &mut [Bush],
    trees: &mut [Tree],
    water: &[Water],
    piles: &mut Vec<Pile>,
    sheltered: bool,
    dt: f32,
) -> Option<usize> {
    match c.action {
        Action::Eat => {
            if c.carrying(Item::Food) > 0.01 {
                let bite = take_from_hands(c, (EAT_RATE * dt).min(1.0 - c.food));
                c.food += bite;
            } else if let Some((spot, x, y)) = nearest_food(c, bushes, piles) {
                if walk(c, x, y, REACH, dt) {
                    let bite = take_food(spot, bushes, piles, (EAT_RATE * dt).min(1.0 - c.food));
                    c.food += bite;
                    if let FoodSpot::Pile(k) = spot {
                        return Some(k); // ate from a pile: maybe our own stored food
                    }
                }
            }
        }
        Action::Drink => {
            if let Some(w) = water
                .iter()
                .min_by(|a, b| dist(c.x, c.y, a.x, a.y).total_cmp(&dist(c.x, c.y, b.x, b.y)))
            {
                if walk(c, w.x, w.y, REACH, dt) {
                    c.water += DRINK_RATE * dt;
                }
            }
        }
        Action::Rest => {
            let bonus = if sheltered { SHELTER_REST_BONUS } else { 1.0 };
            c.boredom -= REST_RATE * bonus * dt;
        }
        Action::Explore | Action::Follow | Action::Flee => {
            if let Some((tx, ty)) = c.target {
                walk(c, tx, ty, 4.0, dt);
            }
        }
        Action::PickFood => {
            if let Some((spot, x, y)) = nearest_food(c, bushes, piles) {
                if walk(c, x, y, REACH, dt) {
                    let room = CARRY_CAP - c.carried();
                    let got = take_food(spot, bushes, piles, (GATHER_RATE * dt).min(room));
                    put_in_hands(c, Item::Food, got);
                }
            }
        }
        Action::PickWood => {
            if let Some(t) = trees
                .iter_mut()
                .filter(|t| t.amount > 0.05)
                .min_by(|a, b| dist(c.x, c.y, a.x, a.y).total_cmp(&dist(c.x, c.y, b.x, b.y)))
            {
                if walk(c, t.x, t.y, REACH, dt) {
                    let got = (GATHER_RATE * dt).min(t.amount).min(CARRY_CAP - c.carried());
                    t.amount -= got;
                    put_in_hands(c, Item::Wood, got);
                }
            }
        }
        Action::Drop => {
            if let Some((item, amount)) = c.carry.take() {
                drop_pile(piles, c, item, amount);
            }
        }
        Action::GoHome => {
            walk(c, c.home.0, c.home.1, HOME_REACH, dt);
        }
        Action::Reproduce | Action::Share | Action::Steal => {} // instant, done at the tick
    }
    None
}

/// Drop onto a nearby pile of the same item, or start a new one. The pile
/// remembers the latest dropper and what they had been doing.
fn drop_pile(piles: &mut Vec<Pile>, c: &Creature, item: Item, amount: f32) {
    let (x, y) = (c.x, c.y);
    let existing = piles
        .iter_mut()
        .filter(|p| p.item == item && dist(x, y, p.x, p.y) <= PILE_MERGE)
        .min_by(|a, b| dist(x, y, a.x, a.y).total_cmp(&dist(x, y, b.x, b.y)));
    match existing {
        Some(p) => {
            p.amount += amount;
            p.family = c.family;
            p.dropper = (c.id, c.generation);
            p.memory = c.memory.clone();
        }
        None => piles.push(Pile::new(x, y, item, amount, c)),
    }
}

/// Nearest bush, or food pile that belongs to the creature's own family.
/// Piles are family property: other families leave them alone (they can
/// still steal from creatures directly).
fn nearest_food(c: &Creature, bushes: &[Bush], piles: &[Pile]) -> Option<(FoodSpot, f32, f32)> {
    let b = bushes
        .iter()
        .enumerate()
        .filter(|(_, b)| b.amount > 0.05)
        .map(|(k, b)| (FoodSpot::Bush(k), b.x, b.y));
    let p = piles
        .iter()
        .enumerate()
        .filter(|(_, p)| p.item == Item::Food && p.amount > 0.05 && p.family == c.family)
        .map(|(k, p)| (FoodSpot::Pile(k), p.x, p.y));
    b.chain(p)
        .min_by(|a, b| dist(c.x, c.y, a.1, a.2).total_cmp(&dist(c.x, c.y, b.1, b.2)))
}

/// Take up to `want` food from a spot. Returns how much was taken.
fn take_food(spot: FoodSpot, bushes: &mut [Bush], piles: &mut [Pile], want: f32) -> f32 {
    let amount = match spot {
        FoodSpot::Bush(k) => &mut bushes[k].amount,
        FoodSpot::Pile(k) => &mut piles[k].amount,
    };
    let got = want.min(*amount).max(0.0);
    *amount -= got;
    got
}

fn put_in_hands(c: &mut Creature, item: Item, amount: f32) {
    let held = c.carrying(item);
    c.carry = Some((item, (held + amount).min(CARRY_CAP)));
}

/// Remove up to `want` from whatever is in hand. Returns how much was removed.
fn take_from_hands(c: &mut Creature, want: f32) -> f32 {
    let Some((item, amount)) = c.carry else { return 0.0 };
    let got = want.min(amount).max(0.0);
    let left = amount - got;
    c.carry = if left > 0.001 { Some((item, left)) } else { None };
    got
}

/// Food goes into the stomach; whatever doesn't fit goes into the hands if
/// they're free or already hold food.
fn feed(c: &mut Creature, amount: f32) {
    let eaten = amount.min(1.0 - c.food);
    c.food += eaten;
    if c.carry.is_none_or(|(i, _)| i == Item::Food) {
        put_in_hands(c, Item::Food, amount - eaten);
    }
}

/// Move toward (tx, ty). Returns true once within `reach`.
fn walk(c: &mut Creature, tx: f32, ty: f32, reach: f32, dt: f32) -> bool {
    let (dx, dy) = (tx - c.x, ty - c.y);
    let d = (dx * dx + dy * dy).sqrt();
    if d <= reach {
        return true;
    }
    let step = (SPEED * dt).min(d);
    c.x += dx / d * step;
    c.y += dy / d * step;
    false
}

pub fn action_color(a: Action) -> u32 {
    match a {
        Action::Eat => 0x4caf50,
        Action::Drink => 0x2196f3,
        Action::Rest => 0xffffff,
        Action::Reproduce => 0xffeb3b,
        Action::Explore => 0x9e9e9e,
        Action::Follow => 0x00bcd4,
        Action::Share => 0x8bc34a,
        Action::Steal => 0xf44336,
        Action::Flee => 0xff9800,
        Action::PickFood => 0xffb74d,
        Action::PickWood => 0xa1887f,
        Action::Drop => 0xe91e63,
        Action::GoHome => 0x7e57c2,
    }
}

/// A distinct, stable color per family (hue spaced by the golden angle).
pub fn family_color(family: u32) -> u32 {
    let h = (family as f32 * 0.618_034).fract() * 6.0;
    let (s, v) = (0.65, 0.95);
    let c = v * s;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let to = |f: f32| ((f + m) * 255.0) as u32;
    (to(r) << 16) | (to(g) << 8) | to(b)
}

fn dist(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt()
}

fn rand_in(max: f32) -> f32 {
    30.0 + fastrand::f32() * (max - 60.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_world() -> World {
        let mut w = World::new(400.0, 400.0, 1);
        w.bushes.clear();
        w.trees.clear();
        w
    }

    #[test]
    fn bushes_run_out_and_regrow() {
        let mut w = World::new(400.0, 400.0, 0);
        w.bushes.truncate(1);
        w.bushes[0].amount = 0.0;
        for _ in 0..60 {
            w.step(1.0 / 60.0);
        }
        assert!((w.bushes[0].amount - WILD_REGROW).abs() < 0.01);
    }

    fn at(x: f32, y: f32, family: u32) -> Creature {
        Creature::new(0, x, y, family, crate::creature::random_personality(), 0.5)
    }

    #[test]
    fn dropping_merges_into_a_nearby_pile() {
        let mut piles = Vec::new();
        drop_pile(&mut piles, &at(100.0, 100.0, 0), Item::Food, 0.5);
        drop_pile(&mut piles, &at(110.0, 100.0, 1), Item::Food, 0.3); // within PILE_MERGE
        drop_pile(&mut piles, &at(110.0, 100.0, 0), Item::Wood, 0.4); // different item
        drop_pile(&mut piles, &at(300.0, 300.0, 0), Item::Food, 0.2); // far away
        assert_eq!(piles.len(), 3);
        assert!((piles[0].amount - 0.8).abs() < 1e-5);
        assert_eq!(piles[0].family, 1); // latest dropper
    }

    #[test]
    fn a_sprouting_pile_teaches_the_dropper_family() {
        let mut w = empty_world();
        let mut c = at(200.0, 200.0, 0);
        c.memory = vec![([1.0; N_STATES], Action::Drop)];
        w.piles.push(Pile::new(200.0, 200.0, Item::Food, 50.0, &c));
        for _ in 0..(600 * 10) {
            w.step(0.1);
            if w.sprouts > 0 {
                break;
            }
        }
        assert!(w.culture.knows(0, Discovery::Seeds));
        assert!(w.culture.learned(0)[Action::Drop.index()][0] > 0.0);
    }

    #[test]
    fn warmth_only_teaches_the_family_that_built_the_pile() {
        let mut w = empty_world();
        w.ticks = NIGHT_START;
        let (x, y) = (w.creatures[0].x, w.creatures[0].y);
        let builder = at(x, y, 0);
        w.piles.push(Pile::new(x, y, Item::Wood, SHELTER_WOOD + 1.0, &builder));
        w.creatures[0].family = 0;
        w.step(0.1);
        assert!(w.culture.knows(0, Discovery::Warmth));
        let n = w.culture.log.len();
        w.step(0.1); // same pile, same day: no second payoff
        assert_eq!(w.culture.rewards[0][Discovery::Warmth as usize], 1);
        assert_eq!(w.culture.log.len(), n);
    }

    #[test]
    fn big_wood_pile_is_a_shelter_from_the_cold() {
        let mut w = empty_world();
        w.ticks = NIGHT_START;
        let (x, y) = (w.creatures[0].x, w.creatures[0].y);
        let builder = at(x, y, 0);
        w.piles.push(Pile::new(x, y, Item::Wood, SHELTER_WOOD + 0.5, &builder));
        let c = &mut w.creatures[0];
        c.food = 1.0;
        c.water = 1.0;
        c.boredom = 0.0;
        c.health = 0.5;
        c.action = Action::Rest;
        w.step(1.0);
        assert!(w.creatures[0].health > 0.5, "sheltered creature should heal at night");

        w.piles.clear();
        let before = w.creatures[0].health;
        w.step(1.0);
        assert!(w.creatures[0].health < before, "unsheltered creature gets cold");
    }

    #[test]
    fn food_piles_eventually_sprout_into_bushes() {
        let mut w = empty_world();
        w.creatures.clear();
        let dropper = at(200.0, 200.0, 0);
        w.piles.push(Pile::new(200.0, 200.0, Item::Food, 5.0, &dropper));
        for _ in 0..(600 * 10) {
            w.step(0.1); // 600 seconds; expected first sprout ~100s
        }
        assert!(w.sprouts > 0);
        assert!(w.bushes.iter().all(|b| b.planted));
    }

    #[test]
    fn hesitating_creature_stands_still() {
        let mut w = empty_world();
        let c = &mut w.creatures[0];
        c.action = Action::GoHome;
        c.home = (c.x + 200.0, c.y);
        c.hesitation = 0.5;
        let x0 = c.x;
        w.step(0.25);
        assert_eq!(w.creatures[0].x, x0);
    }

    #[test]
    fn stealing_moves_food_and_hurts_both() {
        let mut w = World::new(400.0, 400.0, 2);
        w.creatures[0].food = 0.2;
        w.creatures[1].carry = Some((Item::Food, 0.5));
        w.steal(0, 1);
        assert!((w.creatures[1].carried() - 0.3).abs() < 1e-5);
        assert!(w.creatures[0].food > 0.2);
        assert!(w.creatures[0].health < 1.0 && w.creatures[1].health < 1.0);
        assert!(w.creatures[1].fear > 0.9);
    }
}
