// The world: NPCs, animals, bushes, trees, water, piles; and the systems that
// tie them together.
//
// Each decision tick (once per sim second):
//   1. bodies age, get hungry, feelings and stories fade
//   2. every NPC perceives its surroundings (threats, grudges, prey, partners,
//      who it could talk to / ask / trade with / rob...)
//   3. leaders are worked out (who follows whom)
//   4. every NPC reconsiders its GOAL (goals.rs) and picks a METHOD for it
// Every frame:
//   5. NPCs run their method's current primitive (walk, eat, fight...)
//   6. fights, conversations, trades, births and payoffs are resolved
//   7. notable events go into the CHRONICLE and become legends

use std::collections::HashMap;

use crate::chronicle::{self, Belief, Chronicle, EventKind, Subject};
use crate::culture::{Culture, Discovery};
use crate::ecology::{self, Animal, AnimalEvent, PersonView, Species};
use crate::goals::{choose_goal, Goal, GoalChange, GIVE_UP_COOLDOWN, GOALS, N_GOALS};
use crate::methods::{authored, innate_for, Active, Item, Method, Prim, Target};
use crate::npc::*;
use crate::social::{self, TalkMove};
use crate::story::StoryFile;

// --- body ---
const METABOLISM_EVERY: u64 = 5;
const METABOLISM: f32 = 0.10;
const STRENGTH_COST: f32 = 0.04; //   extra food per metabolism tick at full strength
const EAT_RATE: f32 = 0.25;
const DRINK_RATE: f32 = 0.25;
const FATIGUE_RATE: f32 = 0.05;
const REST_RATE: f32 = 0.15;
const SHELTER_REST_BONUS: f32 = 2.0;
const STARVE_DAMAGE: f32 = 0.05;
const EXHAUSTION_DAMAGE: f32 = 0.03;
const COLD_DAMAGE: f32 = 0.01;
const HEAL_RATE: f32 = 0.02;
const HEAL_ABOVE: f32 = 0.5;

// --- resources ---
const WILD_BUSHES: usize = 6;
const WILD_MAX: f32 = 8.0;
const WILD_REGROW: f32 = 0.2;
const PLANTED_MAX: f32 = 4.0;
const PLANTED_REGROW: f32 = 0.1;
const MAX_BUSHES: usize = 40;
const TREES: usize = 4;
const TREE_MAX: f32 = 10.0;
const TREE_REGROW: f32 = 0.15;
const WATER_SOURCES: usize = 4;
const GATHER_RATE: f32 = 0.4;
const MEAT: f32 = 1.5;

// --- piles ---
const PILE_MERGE: f32 = 20.0;
const FOOD_SPOIL: f32 = 0.003;
const WOOD_DECAY: f32 = 0.001;
const SPROUT_CHANCE: f32 = 0.03;
const SPROUT_COST: f32 = 0.2;
pub const SHELTER_WOOD: f32 = 1.0;
const SHELTER_RADIUS: f32 = 50.0;
const PANTRY_AGE: f32 = 30.0;

// --- social / conflict ---
const NEAR: f32 = 40.0;
const TALK_REACH: f32 = 25.0;
const VIEW: f32 = 150.0;
const SHARE_AMOUNT: f32 = 0.15;
const STEAL_AMOUNT: f32 = 0.2;
const FIGHT_DAMAGE: f32 = 0.12; // per second at 0.5 strength... scaled by (0.5 + strength)
const LONELY_RATE: f32 = 0.03;
const UNLONELY_RATE: f32 = 0.10;
const FEAR_DECAY: f32 = 0.05;
const ANNOYANCE_DECAY: f32 = 0.02; // per tick
const PRESTIGE_DECAY: f32 = 0.998; // per tick
const MIGRATE_DISTANCE: f32 = 250.0;

// --- time ---
pub const DAY_LENGTH: u64 = 60;
const NIGHT_START: u64 = 36;
pub const FAMILY_SAMPLE_EVERY: u64 = 5;
pub const GENE_SAMPLE_EVERY: u64 = 10;

pub const MAX_POPULATION: usize = 600;
const START_DEER: usize = 15;
const START_WOLVES: usize = 3;

const SPEED: f32 = 60.0;
const REACH: f32 = 12.0;
const HOME_REACH: f32 = 20.0;
const HOME_SCALE: f32 = 100.0;

pub struct Bush {
    pub x: f32,
    pub y: f32,
    pub amount: f32,
    pub max: f32,
    pub regrow: f32,
    pub planted: bool,
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
    pub family: u32,
    pub dropper: (u64, u32),
    pub memory: Vec<Prim>, // what the last dropper had been doing
    pub learns: bool,      // the dropper's mind can learn from this
    pub credited_day: [u64; 2],
    pub age: f32,
    pub announced: bool, // first-shelter event already recorded
}

impl Pile {
    pub fn is_shelter(&self) -> bool {
        self.item == Item::Wood && self.amount >= SHELTER_WOOD
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeathCause {
    OldAge,
    Starved,
    Exhausted,
    Cold,
    Fight,
    Wolf,
}

pub const DEATH_CAUSES: [(DeathCause, &str); 6] = [
    (DeathCause::OldAge, "AGE"),
    (DeathCause::Starved, "STARVE"),
    (DeathCause::Exhausted, "TIRED"),
    (DeathCause::Cold, "COLD"),
    (DeathCause::Fight, "FIGHT"),
    (DeathCause::Wolf, "WOLF"),
];

#[derive(Default, Clone, Copy)]
pub struct Stats {
    pub talks: usize,
    pub insults: usize,
    pub gifts: usize,
    pub refusals: usize,
    pub trades: usize,
    pub fights: usize,
    pub raids: usize,
    pub kills: usize,
    pub wolves_slain: usize,
    pub deer_hunted: usize,
    pub migrations: usize,
    pub offerings: usize,
    pub sprouts: usize,
    pub copied_leader: usize,
    pub hesitations: usize,
}

/// What an NPC noticed this tick (targets for its goals and methods).
#[derive(Default, Clone)]
struct Senses {
    threat: Option<Target>,
    threat_pos: Option<(f32, f32)>,
    kin_threat: Option<Target>,
    grudge: Option<u64>,
    prey: Option<u64>,
    revered: Option<u64>,
    sacred: Option<(f32, f32)>,
    partner: Option<u64>,
    talk: Option<u64>,
    ask: Option<u64>,
    trade: Option<u64>,
    share: Option<u64>,
    steal: Option<u64>,
    enemy_pile: Option<usize>,
    own_pile: Option<usize>,
    bush: Option<usize>,
    water: usize,
    tree: Option<usize>,
    shelter: Option<(f32, f32)>,
    sheltered: bool,
    kin: Option<u64>,
    leader: Option<u64>,
}

enum Interaction {
    Talk { from: u64, to: u64 },
    Ask { from: u64, to: u64 },
    Trade { from: u64, to: u64 },
    Share { from: u64, to: u64 },
    Steal { from: u64, to: u64 },
    Mate { from: u64, to: u64 },
    Bud { from: u64 },
    Offer { from: u64, to: Option<u64>, place: (f32, f32) },
    Raid { thief: u64, victims: u32, place: (f32, f32) },
    Migrate { from: u64 },
}

enum Payoff {
    Sprouted { pile: usize },
    KeptWarm { pile: usize, family: u32 },
    Fed { pile: usize, family: u32 },
}

#[derive(PartialEq)]
enum Status {
    Running,
    Done,
    Failed,
}

/// A running conflict between two families: a chain of raids, fights and
/// killings. Once it's serious enough it becomes a FEUD in the chronicle and
/// a legend for both families, each blaming the other. Enough kindness across
/// the divide (gifts, trades, a shared child) brings PEACE.
#[derive(Clone, Debug)]
pub struct Feud {
    pub a: u32,
    pub b: u32,
    pub incidents: u32,
    pub last: u64,
    pub started_by: u32,
    pub event: Option<usize>, // chronicle entry while the feud is on
    pub kindness: u32,        // friendly exchanges since it started
}

/// A leader's death is a chronicle event if it had at least this many followers.
const LEGENDARY_LEADER: u32 = 8;
const FEUD_AFTER: u32 = 3; //       incidents before it counts as a feud
const FEUD_COOLS: u64 = 3 * DAY_LENGTH; // no incidents for this long: starts over
const PEACE_AFTER: u32 = 5; //      friendly exchanges that end a feud

pub struct World {
    pub width: f32,
    pub height: f32,
    pub seed: u64,
    rng: fastrand::Rng,
    pub npcs: Vec<Npc>,
    pub animals: Vec<Animal>,
    pub bushes: Vec<Bush>,
    pub trees: Vec<Tree>,
    pub water: Vec<Water>,
    pub piles: Vec<Pile>,
    pub ticks: u64,
    pub births: usize,
    pub deaths_by: [usize; 6],
    pub culture: Culture,
    pub chronicle: Chronicle,
    pub n_families: usize,
    pub stats: Stats,
    pub feuds: Vec<Feud>,

    pub pop_history: Vec<usize>,
    pub family_history: Vec<Vec<u32>>,
    pub gene_history: Vec<GoalGenes>,
    pub trait_history: Vec<Traits>,

    recent_attacks: Vec<(u64, u64, u64)>, // (attacker, victim, tick)
    starvation_today: Vec<(u32, u64, u32)>, // (family, day, deaths)
    first_shelter: Vec<bool>,
    decision_timer: f32,
    next_id: u64,
    next_animal_id: u64,
}

fn dist(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt()
}

impl World {
    pub fn new(width: f32, height: f32, founders: usize, seed: u64, story: Option<&StoryFile>) -> Self {
        let mut rng = fastrand::Rng::with_seed(seed);
        let rand_in = |rng: &mut fastrand::Rng, max: f32| 30.0 + rng.f32() * (max - 60.0);

        let bushes = (0..WILD_BUSHES)
            .map(|_| Bush {
                x: rand_in(&mut rng, width),
                y: rand_in(&mut rng, height),
                amount: WILD_MAX,
                max: WILD_MAX,
                regrow: WILD_REGROW,
                planted: false,
            })
            .collect();
        let trees =
            (0..TREES).map(|_| Tree { x: rand_in(&mut rng, width), y: rand_in(&mut rng, height), amount: TREE_MAX }).collect();
        let water =
            (0..WATER_SOURCES).map(|_| Water { x: rand_in(&mut rng, width), y: rand_in(&mut rng, height) }).collect();

        // founders: each its own family, 1 in 4 with a simple mind
        let mut npcs: Vec<Npc> = (0..founders)
            .map(|i| {
                let mind = if i % 4 == 0 { Mind::Simple } else { Mind::Villager };
                let (x, y) = (rand_in(&mut rng, width), rand_in(&mut rng, height));
                Npc::new(i as u64, x, y, i as u32, mind, &mut rng)
            })
            .collect();

        let story_npcs = story.map_or(0, |s| s.npc.len());
        let n_families = founders + story_npcs;
        let mut culture = Culture::new(n_families);

        if let Some(s) = story {
            for (k, sn) in s.npc.iter().enumerate() {
                let family = (founders + k) as u32;
                let mind = match sn.mind.to_ascii_lowercase().as_str() {
                    "simple" => Mind::Simple,
                    "villager" => Mind::Villager,
                    _ => Mind::Story,
                };
                let mut n = Npc::new((founders + k) as u64, sn.x, sn.y, family, mind, &mut rng);
                n.name = Some(sn.name.clone());
                n.wanderer = sn.wanderer;
                if let Some(l) = sn.lifespan {
                    n.lifespan = l;
                }
                for (name, v) in &sn.traits {
                    if let Some(i) = trait_index(name) {
                        n.traits[i] = v.clamp(0.0, 1.0);
                    }
                }
                if sn.wanderer {
                    n.genes[Goal::Explore.index()][BIAS] += 0.6;
                }
                for m in &sn.methods {
                    if let Some(method) = authored(m) {
                        n.own_methods.push(method.clone());
                        culture.author(family, None, Some(method), "");
                    }
                }
                for know in &sn.knows {
                    culture.author(family, Discovery::from_name(&know.discovery), None, &know.note);
                }
                npcs.push(n);
            }
        }

        let mut next_animal_id = 1_000_000;
        let mut animals = Vec::new();
        for i in 0..(START_DEER + START_WOLVES) {
            let species = if i < START_DEER { Species::Deer } else { Species::Wolf };
            let (x, y) = (rand_in(&mut rng, width), rand_in(&mut rng, height));
            let mut a = Animal::new(next_animal_id, species, x, y, &mut rng);
            a.age = 40.0;
            animals.push(a);
            next_animal_id += 1;
        }

        let next_id = npcs.len() as u64;
        let mut w = Self {
            width,
            height,
            seed,
            rng,
            npcs,
            animals,
            bushes,
            trees,
            water,
            piles: Vec::new(),
            ticks: 0,
            births: 0,
            deaths_by: [0; 6],
            culture,
            chronicle: Chronicle::new(),
            n_families,
            stats: Stats::default(),
            feuds: Vec::new(),
            pop_history: Vec::new(),
            family_history: Vec::new(),
            gene_history: Vec::new(),
            trait_history: Vec::new(),
            recent_attacks: Vec::new(),
            starvation_today: Vec::new(),
            first_shelter: vec![false; n_families],
            decision_timer: 0.0,
            next_id,
            next_animal_id,
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

    pub fn npc(&self, id: u64) -> Option<&Npc> {
        self.npcs.iter().find(|n| n.id == id)
    }

    fn index_of(&self, id: u64) -> Option<usize> {
        self.npcs.iter().position(|n| n.id == id)
    }

    fn sheltered(&self, x: f32, y: f32) -> bool {
        self.piles.iter().any(|p| p.is_shelter() && dist(x, y, p.x, p.y) <= SHELTER_RADIUS)
    }

    /// Advance the world by `dt` seconds. Returns true if a decision tick happened.
    pub fn update(&mut self, dt: f32) -> bool {
        self.decision_timer += dt;
        let mut ticked = false;
        while self.decision_timer >= 1.0 {
            self.decision_timer -= 1.0;
            self.tick();
            ticked = true;
        }
        self.step(dt);
        ticked
    }

    // =======================================================================
    // Decision tick
    // =======================================================================

    fn tick(&mut self) {
        self.ticks += 1;
        let metabolism = self.ticks % METABOLISM_EVERY == 0;
        for n in &mut self.npcs {
            n.age += 1.0;
            n.cooldown -= 1.0;
            for c in n.goal_cooldown.iter_mut() {
                *c -= 1.0;
            }
            n.annoyance = (n.annoyance - ANNOYANCE_DECAY).max(0.0);
            n.prestige *= PRESTIGE_DECAY;
            n.fade_feelings();
            chronicle::fade(&mut n.beliefs);
            if metabolism {
                n.food = (n.food - METABOLISM - STRENGTH_COST * n.traits[STRENGTH]).max(0.0);
                n.water = (n.water - METABOLISM).max(0.0);
            }
            if let Some(f) = n.stack.last_mut() {
                f.ticks += 1.0;
            }
            let hard = n.food < 0.2 || n.water < 0.2 || n.health < 0.5 || n.perception.cold > 0.0;
            if hard {
                chronicle::recall_in_hardship(&mut n.beliefs, self.rng.f32());
            }
        }
        self.culture.forget();
        ecology::tick(&mut self.animals, &mut self.next_animal_id, self.ticks, self.width, self.height, &mut self.rng);
        let now = self.ticks;
        self.recent_attacks.retain(|&(_, _, t)| now - t <= 5);

        let senses = self.perceive();
        self.update_leadership();
        for i in 0..self.npcs.len() {
            self.decide(i, &senses[i]);
        }
        self.record_history();
    }

    fn record_history(&mut self) {
        self.pop_history.push(self.npcs.len());
        if self.ticks % FAMILY_SAMPLE_EVERY == 0 {
            let mut counts = vec![0u32; self.n_families];
            for n in &self.npcs {
                counts[n.family as usize] += 1;
            }
            self.family_history.push(counts);
        }
        if self.ticks % GENE_SAMPLE_EVERY == 0 && !self.npcs.is_empty() {
            self.gene_history.push(self.average_genes().unwrap());
            self.trait_history.push(self.average_traits());
        }
    }

    // -----------------------------------------------------------------------
    // Perception
    // -----------------------------------------------------------------------

    fn perceive(&mut self) -> Vec<Senses> {
        let night = self.is_night();
        let n = self.npcs.len();
        let mut all = Vec::with_capacity(n);

        for i in 0..n {
            let me = &self.npcs[i];
            let mut s = Senses::default();
            let (x, y) = (me.x, me.y);
            s.sheltered = self.sheltered(x, y);
            let mut p = Perception {
                cold: if night && !s.sheltered { 1.0 } else { 0.0 },
                near_home: 1.0 - (dist(x, y, me.home.0, me.home.1) / HOME_SCALE).min(1.0),
                ..Default::default()
            };
            let hard = me.food < 0.4 || me.health < 0.6 || p.cold > 0.0;

            // taboo places (bad memories)
            let taboo: Vec<(f32, f32)> = me
                .beliefs
                .iter()
                .filter(|b| b.valence < 0.0 && matches!(b.subject, Subject::Place(..) | Subject::Wolves))
                .map(|b| b.place)
                .collect();
            let penalty = |px: f32, py: f32| if taboo.iter().any(|t| dist(px, py, t.0, t.1) < 80.0) { 300.0 } else { 0.0 };

            // wolves: personal threat, threats to kin
            for a in &self.animals {
                let d = dist(x, y, a.x, a.y);
                match a.species {
                    Species::Wolf => {
                        if d < 120.0 && p.threat < 1.0 - d / 240.0 {
                            p.threat = 1.0 - d / 240.0;
                            s.threat = Some(Target::Animal(a.id));
                            s.threat_pos = Some((a.x, a.y));
                        }
                    }
                    Species::Deer => {
                        if d < 200.0 && (s.prey.is_none() || p.prey_near < 1.0 - d / 250.0) {
                            p.prey_near = 1.0 - d / 250.0;
                            s.prey = Some(a.id);
                        }
                    }
                }
            }
            // attacked recently?
            for &(attacker, victim, _) in &self.recent_attacks {
                if let Some(att) = self.npcs.iter().find(|o| o.id == attacker) {
                    let d = dist(x, y, att.x, att.y);
                    if victim == me.id && d < VIEW {
                        p.threat = 1.0;
                        s.threat = Some(Target::Npc(attacker));
                        s.threat_pos = Some((att.x, att.y));
                    } else if attacker != me.id && d < VIEW {
                        if let Some(v) = self.npcs.iter().find(|o| o.id == victim) {
                            if v.family == me.family || me.affinity(victim) > 0.3 {
                                p.kin_threat = 1.0;
                                s.kin_threat = Some(Target::Npc(attacker));
                            }
                        }
                    }
                }
            }

            // other NPCs
            let best = |slot: &mut Option<(u64, f32)>, id: u64, score: f32| {
                if slot.is_none_or(|(_, s)| score < s) {
                    *slot = Some((id, score));
                }
            };
            let (mut partner, mut talk, mut ask, mut trade, mut share, mut steal, mut kin, mut grudge) =
                (None, None, None, None, None, None, None, None);
            let mut grudge_level = 0.0f32;
            for o in &self.npcs {
                if o.id == me.id {
                    continue;
                }
                let d = dist(x, y, o.x, o.y);
                if d > VIEW {
                    continue;
                }
                if d < NEAR {
                    p.crowd += 1.0 / 8.0;
                }
                let att = social::attitude(me, o.id, o.family);
                let kin_of = o.family == me.family;
                if o.can_mate() && att >= -0.1 {
                    best(&mut partner, o.id, d);
                }
                if att >= -0.6 {
                    best(&mut talk, o.id, d);
                }
                if att > -0.3 && (o.carrying(Item::Food) >= 0.15 || o.food > 0.6) {
                    best(&mut ask, o.id, d);
                }
                if let (Some((mine, _)), Some((theirs, _))) = (me.carry, o.carry) {
                    if mine != theirs {
                        best(&mut trade, o.id, d);
                    }
                }
                if o.food < 0.35 && d < NEAR * 2.0 {
                    best(&mut share, o.id, o.food);
                }
                if !kin_of && d < NEAR * 2.0 && (o.food > 0.3 || o.carrying(Item::Food) > 0.05) {
                    best(&mut steal, o.id, d);
                }
                if kin_of && d > 25.0 {
                    best(&mut kin, o.id, d);
                }
                // grudges: personal, against the family, or from stories
                let mut g = (-me.affinity(o.id)).max(-me.family_feeling(o.family));
                for b in &me.beliefs {
                    let about = match b.subject {
                        Subject::Being(id) => id == o.id,
                        Subject::Family(f) => f == o.family,
                        _ => false,
                    };
                    if about && b.valence < 0.0 {
                        g = g.max(0.3 * b.magnitude);
                    }
                }
                if g > 0.2 && g > grudge_level {
                    grudge_level = g;
                    grudge = Some(o.id);
                }
                // reverence for a legendary being who is here
                for b in &me.beliefs {
                    if b.valence > 0.0 && b.subject == Subject::Being(o.id) && b.magnitude >= 1.0 {
                        let r = (b.magnitude / 3.0).min(1.0);
                        if r > p.reverence {
                            p.reverence = r;
                            s.revered = Some(o.id);
                        }
                    }
                }
            }
            p.grudge = grudge_level.min(1.0);
            s.partner = partner.map(|v| v.0);
            s.talk = talk.map(|v| v.0);
            s.ask = ask.map(|v| v.0);
            s.trade = trade.map(|v| v.0);
            s.share = share.map(|v| v.0);
            s.steal = steal.map(|v| v.0);
            s.kin = kin.map(|v| v.0);
            s.grudge = grudge;
            s.leader = me.following;

            // a legend about someone far away: in hard times, make offerings
            if hard && s.revered.is_none() {
                let present = |id: u64| self.npcs.iter().any(|o| o.id == id && dist(x, y, o.x, o.y) < VIEW);
                s.sacred = me
                    .beliefs
                    .iter()
                    .filter(|b| b.valence > 0.0 && b.magnitude >= 1.5)
                    .filter(|b| !matches!(b.subject, Subject::Being(id) if present(id)))
                    .max_by(|a, b| a.magnitude.total_cmp(&b.magnitude))
                    .map(|b| b.place);
                if s.sacred.is_some() {
                    p.reverence = p.reverence.max(0.4);
                }
            }

            // places
            // best bush: lots of food and not too far (so foragers spread out)
            s.bush = self
                .bushes
                .iter()
                .enumerate()
                .filter(|(_, b)| b.amount > 0.05)
                .max_by(|(_, a), (_, b)| {
                    let worth = |bu: &Bush| bu.amount.min(1.0) / (dist(x, y, bu.x, bu.y) + penalty(bu.x, bu.y) + 60.0);
                    worth(a).total_cmp(&worth(b))
                })
                .map(|(k, _)| k);
            s.water = (0..self.water.len())
                .min_by(|&a, &b| {
                    let (wa, wb) = (&self.water[a], &self.water[b]);
                    (dist(x, y, wa.x, wa.y) + penalty(wa.x, wa.y)).total_cmp(&(dist(x, y, wb.x, wb.y) + penalty(wb.x, wb.y)))
                })
                .unwrap_or(0);
            s.tree = self
                .trees
                .iter()
                .enumerate()
                .filter(|(_, t)| t.amount > 0.05)
                .min_by(|(_, a), (_, b)| dist(x, y, a.x, a.y).total_cmp(&dist(x, y, b.x, b.y)))
                .map(|(k, _)| k);
            s.own_pile = self
                .piles
                .iter()
                .enumerate()
                .filter(|(_, pl)| pl.item == Item::Food && pl.family == me.family && pl.amount > 0.05)
                .min_by(|(_, a), (_, b)| dist(x, y, a.x, a.y).total_cmp(&dist(x, y, b.x, b.y)))
                .map(|(k, _)| k);
            s.enemy_pile = self
                .piles
                .iter()
                .enumerate()
                .filter(|(_, pl)| pl.item == Item::Food && pl.family != me.family && pl.amount > 0.2)
                .filter(|(_, pl)| dist(x, y, pl.x, pl.y) < VIEW)
                .min_by(|(_, a), (_, b)| {
                    let fa = me.family_feeling(a.family);
                    let fb = me.family_feeling(b.family);
                    (dist(x, y, a.x, a.y) + 200.0 * fa).total_cmp(&(dist(x, y, b.x, b.y) + 200.0 * fb))
                })
                .map(|(k, _)| k);
            s.shelter = self
                .piles
                .iter()
                .filter(|pl| pl.is_shelter())
                .map(|pl| (pl.x, pl.y, dist(x, y, pl.x, pl.y)))
                .filter(|v| v.2 < 400.0)
                .min_by(|a, b| a.2.total_cmp(&b.2))
                .map(|v| (v.0, v.1));

            self.npcs[i].perception = p;
            all.push(s);
        }
        all
    }

    // -----------------------------------------------------------------------
    // Leadership: follow the most influential trusted NPC in view
    // -----------------------------------------------------------------------

    fn update_leadership(&mut self) {
        let infl: Vec<f32> = self.npcs.iter().map(|n| n.influence()).collect();
        let mut following = vec![None; self.npcs.len()];
        for (i, me) in self.npcs.iter().enumerate() {
            let mut best: Option<(usize, f32)> = None;
            for (j, o) in self.npcs.iter().enumerate() {
                if i == j || dist(me.x, me.y, o.x, o.y) > VIEW {
                    continue;
                }
                if infl[j] < 0.1 || infl[j] < infl[i] * 1.5 || me.trust(o.id, o.family) < 0.3 {
                    continue;
                }
                if best.is_none_or(|(_, b)| infl[j] > b) {
                    best = Some((j, infl[j]));
                }
            }
            following[i] = best.map(|(j, _)| self.npcs[j].id);
        }
        let mut counts: HashMap<u64, u32> = HashMap::new();
        for (n, f) in self.npcs.iter_mut().zip(following) {
            n.following = f;
            if let Some(id) = f {
                *counts.entry(id).or_default() += 1;
            }
        }
        for n in &mut self.npcs {
            n.followers = counts.get(&n.id).copied().unwrap_or(0);
        }
    }

    // -----------------------------------------------------------------------
    // Goals and methods
    // -----------------------------------------------------------------------

    fn goal_possible(&self, g: Goal, n: &Npc, s: &Senses) -> bool {
        match g {
            Goal::FixHunger => {
                n.carrying(Item::Food) > 0.01 || s.bush.is_some() || s.own_pile.is_some() || s.ask.is_some() || s.steal.is_some()
            }
            Goal::FixThirst | Goal::Rest | Goal::Work | Goal::Explore => true,
            Goal::KeepWarm => n.perception.cold > 0.0,
            Goal::Reproduce => (n.can_mate() && s.partner.is_some()) || n.can_bud(),
            Goal::StaySafe => s.threat.is_some() || n.fear > 0.3,
            Goal::Socialize => s.talk.is_some() || s.kin.is_some(),
            Goal::Hunt => s.prey.is_some(),
            Goal::Raid => s.enemy_pile.is_some() || s.steal.is_some(),
            Goal::Avenge => s.grudge.is_some(),
            Goal::Defend => s.kin_threat.is_some(),
            Goal::Revere => s.revered.is_some() || (s.sacred.is_some() && n.carrying(Item::Food) > 0.05),
        }
    }

    fn goal_done(g: Goal, n: &Npc, s: &Senses, ticks: f32) -> bool {
        match g {
            Goal::FixHunger => n.food > 0.9,
            Goal::FixThirst => n.water > 0.9,
            Goal::Rest => n.fatigue < 0.08,
            Goal::KeepWarm => n.perception.cold == 0.0,
            Goal::Reproduce => n.cooldown > 0.0,
            Goal::StaySafe => s.threat.is_none() && n.fear < 0.15,
            Goal::Socialize => n.loneliness < 0.05 && ticks > 3.0,
            Goal::Work => ticks > 15.0,
            Goal::Explore => ticks > 12.0,
            Goal::Hunt => s.prey.is_none(),
            Goal::Raid => n.raided,
            Goal::Avenge => s.grudge.is_none(),
            Goal::Defend => s.kin_threat.is_none(),
            Goal::Revere => ticks > 8.0,
        }
    }

    fn decide(&mut self, i: usize, s: &Senses) {
        let mut rng = fastrand::Rng::with_seed(self.rng.u64(..));
        // --- score goals ---
        let n = &self.npcs[i];
        let state = n.state();
        let raw = goal_scores(&n.genes, &state);
        let leader = n.following.and_then(|id| self.npc(id));
        let leader_goal = leader.and_then(|l| l.goal());
        let leader_pull = leader.map_or(0.0, |l| l.influence() * n.traits[CONFORMITY] * 0.5);
        let mut scores = [0.0; N_GOALS];
        for g in GOALS {
            // a goal only counts if something can actually be done about it
            if !self.goal_possible(g, n, s) || !self.has_method(n, g, s) {
                continue;
            }
            let mut v = raw[g.index()] * g.trait_multiplier(&n.traits);
            if n.goal_cooldown[g.index()] > 0.0 {
                v *= 0.2;
            }
            if Some(g) == leader_goal {
                v += leader_pull;
            }
            // urgency: starving, dying of thirst or collapsing beats everything
            v += match g {
                Goal::FixHunger if n.food < 0.15 => 1.0,
                Goal::FixThirst if n.water < 0.15 => 1.0,
                Goal::Rest if n.fatigue > 0.9 => 1.0,
                _ => 0.0,
            };
            scores[g.index()] = v.max(0.001);
        }
        let current = n.stack.last().map(|f| (f.goal, f.ticks));
        let done = current.is_some_and(|(g, t)| Self::goal_done(g, n, s, t));
        let mut stack = n.stack.clone();
        let mut hesitation = 0.0;
        let traits = n.traits;
        let roll = rng.f32();
        let change = choose_goal(&mut stack, &scores, done, &traits, leader_goal, &mut hesitation, roll);

        let goal = stack.last().map(|f| f.goal);
        let goal_changed = goal != current.map(|c| c.0);
        // why: the two biggest reasons behind the current goal
        let mut why: Vec<(String, f32)> = Vec::new();
        if let Some(g) = goal {
            let mut parts: Vec<(String, f32)> = (0..N_STATES)
                .map(|k| (STATE_NAMES[k].to_string(), n.genes[g.index()][k] * state[k]))
                .collect();
            if Some(g) == leader_goal && leader_pull > 0.0 {
                parts.push(("LEADER".to_string(), leader_pull));
            }
            parts.sort_by(|a, b| b.1.total_cmp(&a.1));
            why = parts.into_iter().take(2).filter(|p| p.1 > 0.01).collect();
        }

        // --- pick a method ---
        let keep = !goal_changed
            && n.active.as_ref().is_some_and(|a| a.method.steps.len() > 1 && Some(a.goal) == goal);
        let mut new_active = None;
        let mut playing = n.playing;
        if let (Some(g), false) = (goal, keep) {
            let play_chance = 0.02 + 0.08 * traits[CURIOSITY];
            if rng.f32() < play_chance {
                // play: try any primitive that's possible right now
                let options: Vec<(Prim, Target)> =
                    Prim::ALL.iter().filter_map(|&p| self.resolve(n, p, g, s).map(|t| (p, t))).collect();
                if !options.is_empty() {
                    let (p, t) = options[rng.usize(..options.len())];
                    new_active = Some(Active { goal: g, method: Method::innate(p), step: 0, target: t, timer: 0.0 });
                    playing = true;
                }
            }
            if new_active.is_none() {
                playing = false;
                let mut best: Option<(f32, Method, Target)> = None;
                let mut consider = |score: f32, m: Method, t: Target| {
                    if best.as_ref().is_none_or(|b| score > b.0) {
                        best = Some((score, m, t));
                    }
                };
                for &p in innate_for(g) {
                    if let Some(t) = self.resolve(n, p, g, s) {
                        let score = self.method_score(n, p, g, s) + rng.f32() * 0.1;
                        consider(score, Method::innate(p), t);
                    }
                }
                if n.mind != Mind::Simple {
                    let learned = self.culture.methods(n.family).iter().map(|k| (k.method.clone(), k.value));
                    let own = n.own_methods.iter().map(|m| (m.clone(), 1.0));
                    for (m, value) in learned.chain(own) {
                        if !m.goals.contains(&g) {
                            continue;
                        }
                        if let Some(t) = self.resolve(n, m.steps[0], g, s) {
                            consider(0.3 + value + rng.f32() * 0.1, m, t);
                        }
                    }
                }
                if let Some((_, m, t)) = best {
                    new_active = Some(Active { goal: g, method: m, step: 0, target: t, timer: 0.0 });
                }
            }
        }

        // --- apply ---
        let gave_up = change == GoalChange::GaveUp;
        if change == GoalChange::CopiedLeader {
            self.stats.copied_leader += 1;
        }
        if change == GoalChange::Hesitating {
            self.stats.hesitations += 1;
        }
        let n = &mut self.npcs[i];
        if gave_up {
            if let Some((g, _)) = current {
                n.goal_cooldown[g.index()] = GIVE_UP_COOLDOWN;
            }
        }
        if goal_changed {
            n.raided = false;
        }
        n.stack = stack;
        n.goal_scores = scores;
        n.why = why;
        n.playing = playing;
        if hesitation > 0.0 {
            n.hesitation = hesitation;
        }
        if goal.is_none() {
            n.active = None;
        } else if !keep {
            if let Some(a) = &new_active {
                n.remember(a.prim());
            }
            n.active = new_active;
        }
    }

    /// Is there at least one method for this goal that can run right now?
    fn has_method(&self, n: &Npc, g: Goal, s: &Senses) -> bool {
        if innate_for(g).iter().any(|&p| self.resolve(n, p, g, s).is_some()) {
            return true;
        }
        if n.mind == Mind::Simple {
            return false;
        }
        let learned = self.culture.methods(n.family).iter().map(|k| &k.method);
        learned
            .chain(n.own_methods.iter())
            .any(|m| m.goals.contains(&g) && self.resolve(n, m.steps[0], g, s).is_some())
    }

    fn method_score(&self, n: &Npc, p: Prim, g: Goal, s: &Senses) -> f32 {
        let t = &n.traits;
        let d_to = |x: f32, y: f32| dist(n.x, n.y, x, y) / 800.0;
        match p {
            Prim::EatHand => 2.0,
            Prim::EatBush => 1.0 - s.bush.map_or(1.0, |k| d_to(self.bushes[k].x, self.bushes[k].y)),
            Prim::EatPile => 1.0 + 0.3 * t[THRIFT] - s.own_pile.map_or(1.0, |k| d_to(self.piles[k].x, self.piles[k].y)),
            Prim::Ask => 0.3 + 0.4 * t[SOCIABILITY] - 0.3 * t[AGGRESSION] + 0.3 * (1.0 - n.food),
            Prim::Trade => 0.5 + 0.3 * t[SOCIABILITY],
            Prim::Steal => 0.1 + 0.9 * t[AGGRESSION] - 0.3 * t[LOYALTY],
            Prim::StealPile => 0.6 + 0.4 * t[AGGRESSION],
            Prim::Drink | Prim::Wander => 1.0,
            Prim::Rest => {
                if g == Goal::KeepWarm { 0.2 } else { 1.0 + if s.sheltered { 0.3 } else { 0.0 } }
            }
            Prim::GoShelter => if g == Goal::KeepWarm { 1.2 } else { 0.8 },
            Prim::FollowKin => 0.4 + 0.4 * t[LOYALTY],
            Prim::FollowLeader => 0.3 + 0.7 * t[CONFORMITY] + if g == Goal::Revere { 0.4 } else { 0.0 },
            Prim::Talk => 0.5 + 0.5 * t[SOCIABILITY],
            Prim::Share => 0.2 + 0.5 * (1.0 - t[THRIFT]) + 0.3 * t[SOCIABILITY],
            Prim::PickFood => 0.4 + 0.3 * t[THRIFT],
            Prim::PickWood => 0.3 + 0.3 * n.perception.cold,
            Prim::Drop => 0.2 + 0.5 * n.perception.near_home,
            Prim::GoHome => 0.3 + 0.4 * (n.carried() / CARRY_CAP),
            Prim::Guard => 0.2 + 0.4 * t[LOYALTY],
            Prim::Attack => match g {
                Goal::Hunt => 0.8,
                Goal::Avenge => 1.0,
                Goal::Defend => 0.6 + 0.6 * t[COURAGE],
                _ => 0.2 + 0.8 * t[COURAGE] + 0.3 * t[STRENGTH],
            },
            Prim::Flee => 1.2 - 0.8 * t[COURAGE],
            Prim::Mate => 0.6 + 0.3 * t[SOCIABILITY],
            Prim::Bud => 0.4,
            Prim::Offer => 1.0,
            Prim::Rally => 0.6 * t[CHARISMA] + 0.4 * t[AMBITION] - 0.2,
        }
    }

    /// Can this primitive be done now, and at what? None = impossible.
    fn resolve(&self, n: &Npc, p: Prim, g: Goal, s: &Senses) -> Option<Target> {
        let room = n.carried() < CARRY_CAP - 0.05;
        // a crumb in your hands doesn't stop you picking up something else
        let free_for = |item: Item| n.carried() < 0.05 || n.carry.is_none_or(|(i, _)| i == item);
        let away_from_home = dist(n.x, n.y, n.home.0, n.home.1) > HOME_REACH;
        match p {
            Prim::EatHand => (n.carrying(Item::Food) > 0.01).then_some(Target::Here),
            Prim::EatBush => s.bush.map(Target::Bush),
            Prim::EatPile => s.own_pile.map(Target::Pile),
            Prim::Drink => Some(Target::Water(s.water)),
            Prim::Rest => Some(Target::Here),
            Prim::GoShelter => s.shelter.filter(|_| !s.sheltered).map(|(x, y)| Target::Pos(x, y)),
            Prim::Wander => {
                let r = if n.wanderer { 350.0 } else { 150.0 };
                let mut rng = fastrand::Rng::with_seed(n.id ^ self.ticks.wrapping_mul(2654435761));
                let tx = (n.x + rng.f32() * 2.0 * r - r).clamp(10.0, self.width - 10.0);
                let ty = (n.y + rng.f32() * 2.0 * r - r).clamp(10.0, self.height - 10.0);
                Some(Target::Pos(tx, ty))
            }
            Prim::PickFood => (room && free_for(Item::Food)).then_some(()).and(s.bush.map(Target::Bush)),
            Prim::PickWood => (room && free_for(Item::Wood)).then_some(()).and(s.tree.map(Target::Tree)),
            Prim::Drop => (n.carried() > 0.05).then_some(Target::Here),
            Prim::GoHome => away_from_home.then_some(Target::Pos(n.home.0, n.home.1)),
            Prim::FollowKin => s.kin.map(Target::Npc),
            Prim::FollowLeader => {
                if g == Goal::Revere { s.revered.map(Target::Npc) } else { s.leader.map(Target::Npc) }
            }
            Prim::Talk => s.talk.map(Target::Npc),
            Prim::Ask => (n.food < 0.6).then_some(()).and(s.ask.map(Target::Npc)),
            Prim::Trade => n.carry.and(s.trade).map(Target::Npc),
            Prim::Share => {
                (n.carrying(Item::Food) >= SHARE_AMOUNT || n.food > 0.6).then_some(()).and(s.share.map(Target::Npc))
            }
            Prim::Steal => s.steal.map(Target::Npc),
            Prim::StealPile => (room && free_for(Item::Food)).then_some(()).and(s.enemy_pile.map(Target::Pile)),
            Prim::Attack => match g {
                Goal::Hunt => s.prey.map(Target::Animal),
                Goal::Avenge => s.grudge.map(Target::Npc),
                Goal::Defend => s.kin_threat,
                Goal::StaySafe => s.threat,
                _ => None,
            },
            Prim::Flee => s.threat_pos.map(|(tx, ty)| {
                let (dx, dy) = (n.x - tx, n.y - ty);
                let d = (dx * dx + dy * dy).sqrt().max(1.0);
                let fx = (n.x + dx / d * 120.0).clamp(10.0, self.width - 10.0);
                let fy = (n.y + dy / d * 120.0).clamp(10.0, self.height - 10.0);
                Target::Pos(fx, fy)
            }),
            Prim::Mate => n.can_mate().then_some(()).and(s.partner.map(Target::Npc)),
            Prim::Bud => n.can_bud().then_some(Target::Here),
            Prim::Offer => {
                if g != Goal::Revere || n.carrying(Item::Food) < 0.05 {
                    None
                } else if let Some(id) = s.revered {
                    Some(Target::Npc(id))
                } else {
                    s.sacred.map(|(x, y)| Target::Pos(x, y))
                }
            }
            Prim::Rally => (n.rallying <= 0.0 && matches!(g, Goal::Hunt | Goal::Raid | Goal::Defend))
                .then_some(Target::Here),
            Prim::Guard => Some(Target::Pos(n.home.0, n.home.1)),
        }
    }

    // =======================================================================
    // Continuous step
    // =======================================================================

    fn step(&mut self, dt: f32) {
        let night = self.is_night();
        let day = self.day();
        for b in &mut self.bushes {
            b.amount = (b.amount + b.regrow * dt).min(b.max);
        }
        for t in &mut self.trees {
            t.amount = (t.amount + TREE_REGROW * dt).min(TREE_MAX);
        }

        let mut payoffs = Vec::new();
        let mut new_bushes = Vec::new();
        for (k, p) in self.piles.iter_mut().enumerate() {
            p.age += dt;
            match p.item {
                Item::Food => {
                    p.amount -= FOOD_SPOIL * dt;
                    if p.amount >= SPROUT_COST
                        && self.bushes.len() + new_bushes.len() < MAX_BUSHES
                        && self.rng.f32() < SPROUT_CHANCE * dt
                    {
                        p.amount -= SPROUT_COST;
                        new_bushes.push(Bush {
                            x: p.x + self.rng.f32() * 30.0 - 15.0,
                            y: p.y + self.rng.f32() * 30.0 - 15.0,
                            amount: SPROUT_COST,
                            max: PLANTED_MAX,
                            regrow: PLANTED_REGROW,
                            planted: true,
                        });
                        self.stats.sprouts += 1;
                        payoffs.push(Payoff::Sprouted { pile: k });
                    }
                }
                Item::Wood => p.amount -= WOOD_DECAY * dt,
            }
        }
        self.bushes.extend(new_bushes);

        let shelters: Vec<(usize, f32, f32)> =
            self.piles.iter().enumerate().filter(|(_, p)| p.is_shelter()).map(|(k, p)| (k, p.x, p.y)).collect();
        let mut positions: HashMap<u64, (f32, f32)> = self.npcs.iter().map(|n| (n.id, (n.x, n.y))).collect();
        for a in &self.animals {
            positions.insert(a.id, (a.x, a.y));
        }

        let mut interactions: Vec<Interaction> = Vec::new();
        let mut hits: Vec<(Target, u64, f32)> = Vec::new(); // (victim, attacker, damage)
        let (w, h) = (self.width, self.height);

        {
            let World { npcs, bushes, trees, water, piles, rng, .. } = self;
            for n in npcs.iter_mut() {
                let shelter = shelters.iter().find(|&&(_, sx, sy)| dist(n.x, n.y, sx, sy) <= SHELTER_RADIUS);
                let sheltered = shelter.is_some();
                if let (true, Some(&(k, _, _))) = (night, shelter) {
                    payoffs.push(Payoff::KeptWarm { pile: k, family: n.family });
                }
                n.fatigue += FATIGUE_RATE * dt;
                n.rallying -= dt;

                if n.hesitation > 0.0 {
                    n.hesitation -= dt;
                } else if let Some(mut a) = n.active.take() {
                    let mut ctx = Ctx {
                        bushes: &mut *bushes,
                        trees: &mut *trees,
                        water: &mut *water,
                        piles: &mut *piles,
                        positions: &positions,
                        interactions: &mut interactions,
                        hits: &mut hits,
                        payoffs: &mut payoffs,
                        sheltered,
                        rng: &mut *rng,
                        w,
                        h,
                    };
                    let status = run(n, &mut a, &mut ctx, dt);
                    a.timer += dt;
                    match status {
                        Status::Running => n.active = Some(a),
                        Status::Done if a.step + 1 < a.method.steps.len() => {
                            a.step += 1;
                            a.timer = 0.0;
                            let next = a.prim();
                            n.remember(next);
                            a.target = next_target(n, next, &a, piles, bushes, trees, water);
                            n.active = Some(a);
                        }
                        Status::Done if n.mind == Mind::Story && a.method.steps.len() > 1 => {
                            // planner: repeat the method while the goal lasts
                            a.step = 0;
                            a.timer = 0.0;
                            let first = a.prim();
                            a.target = next_target(n, first, &a, piles, bushes, trees, water);
                            n.active = Some(a);
                        }
                        _ => {}
                    }
                }

                if n.perception.crowd == 0.0 {
                    n.loneliness += LONELY_RATE * dt;
                } else {
                    n.loneliness -= UNLONELY_RATE * dt;
                }
                n.fear -= FEAR_DECAY * dt;
                n.food = n.food.clamp(0.0, 1.0);
                n.water = n.water.clamp(0.0, 1.0);
                n.fatigue = n.fatigue.clamp(0.0, 1.0);
                n.loneliness = n.loneliness.clamp(0.0, 1.0);
                n.fear = n.fear.clamp(0.0, 1.0);

                let starving = n.food <= 0.0 || n.water <= 0.0;
                let exhausted = n.fatigue >= 1.0;
                let cold = night && !sheltered;
                if starving {
                    n.health -= STARVE_DAMAGE * dt;
                }
                if exhausted {
                    n.health -= EXHAUSTION_DAMAGE * dt;
                }
                if cold {
                    n.health -= COLD_DAMAGE * dt;
                }
                if !starving && !exhausted && !cold && n.food > HEAL_ABOVE && n.water > HEAL_ABOVE {
                    n.health += HEAL_RATE * dt;
                }
                n.health = n.health.min(1.0);
            }
        }

        // animals
        let people: Vec<PersonView> = self
            .npcs
            .iter()
            .map(|n| PersonView {
                id: n.id,
                x: n.x,
                y: n.y,
                alone: !self.npcs.iter().any(|o| o.id != n.id && dist(n.x, n.y, o.x, o.y) < 50.0),
                sheltered: shelters.iter().any(|&(_, sx, sy)| dist(n.x, n.y, sx, sy) <= SHELTER_RADIUS),
            })
            .collect();
        let mut bush_view: Vec<(f32, f32, f32)> = self.bushes.iter().map(|b| (b.x, b.y, b.amount)).collect();
        let mut animal_events = Vec::new();
        ecology::step(&mut self.animals, &mut bush_view, &people, night, dt, w, h, &mut self.rng, &mut animal_events);
        for (b, v) in self.bushes.iter_mut().zip(bush_view) {
            b.amount = v.2;
        }
        for e in animal_events {
            let AnimalEvent::Bite { npc, wolf, dmg } = e;
            if let Some(n) = self.npcs.iter_mut().find(|n| n.id == npc) {
                n.health -= dmg;
                n.fear = 1.0;
                n.last_hurt = Some((Hurt::Wolf(wolf), self.ticks));
            }
        }

        self.apply_hits(hits);
        for it in interactions {
            self.interact(it);
        }
        self.resolve_payoffs(payoffs, day);
        self.check_shelters();
        self.remove_dead_animals();
        self.remove_dead_npcs(night);
    }

    fn apply_hits(&mut self, hits: Vec<(Target, u64, f32)>) {
        for (victim, attacker, dmg) in hits {
            match victim {
                Target::Npc(id) => {
                    let attacker_family = self.npc(attacker).map(|a| a.family).unwrap_or(0);
                    let new_fight = !self.recent_attacks.iter().any(|&(a, v, _)| a == attacker && v == id);
                    if let Some(v) = self.npcs.iter_mut().find(|n| n.id == id) {
                        v.health -= dmg;
                        v.fear = 1.0;
                        v.last_hurt = Some((Hurt::Npc(attacker), self.ticks));
                        if new_fight {
                            v.adjust_affinity(attacker, attacker_family, -0.4);
                            v.adjust_family_feeling(attacker_family, -0.1);
                        }
                    }
                    if new_fight {
                        self.stats.fights += 1;
                        let victim_family = self.npc(id).map(|v| v.family);
                        let place = self.npc(id).map_or((0.0, 0.0), |v| (v.x, v.y));
                        if let Some(vf) = victim_family {
                            self.incident(attacker_family, vf, place);
                        }
                    }
                    self.recent_attacks.retain(|&(a, v, _)| !(a == attacker && v == id));
                    self.recent_attacks.push((attacker, id, self.ticks));
                }
                Target::Animal(id) => {
                    if let Some(a) = self.animals.iter_mut().find(|a| a.id == id) {
                        a.health -= dmg;
                        a.killed_by = Some(attacker);
                    }
                }
                _ => {}
            }
        }
    }

    fn pair(&mut self, a: u64, b: u64) -> Option<(&mut Npc, &mut Npc)> {
        let (i, j) = (self.index_of(a)?, self.index_of(b)?);
        if i == j {
            return None;
        }
        if i < j {
            let (l, r) = self.npcs.split_at_mut(j);
            Some((&mut l[i], &mut r[0]))
        } else {
            let (l, r) = self.npcs.split_at_mut(i);
            Some((&mut r[0], &mut l[j]))
        }
    }

    fn interact(&mut self, it: Interaction) {
        let tick = self.ticks;
        let mut rng = fastrand::Rng::with_seed(self.rng.u64(..));
        match it {
            Interaction::Talk { from, to } => {
                let roll = rng.f32();
                let (mut insults, mut teach) = (0, None);
                {
                    let Some((a, b)) = self.pair(from, to) else { return };
                    let att = social::attitude(a, b.id, b.family);
                    let m = social::talk_move(a, att, roll);
                    let r = social::react(b, m);
                    b.adjust_affinity(a.id, a.family, r.affinity);
                    b.annoyance = (b.annoyance + r.annoyance).clamp(0.0, 1.0);
                    a.loneliness = (a.loneliness - 0.3).max(0.0);
                    b.loneliness = (b.loneliness - 0.3).max(0.0);
                    if m == TalkMove::Insult {
                        insults = 1;
                        if r.snaps {
                            // insult back: now it's a grudge on both sides
                            a.adjust_affinity(b.id, b.family, -0.3);
                            b.adjust_affinity(a.id, a.family, -0.3);
                            insults = 2;
                        }
                    } else {
                        a.adjust_affinity(b.id, b.family, 0.05);
                        // gossip: a story each way, and prejudice about a third family
                        swap_stories(a, b, &mut rng);
                        if let Some(&(f, feel)) =
                            a.family_feelings.iter().filter(|f| f.1 < -0.3).min_by(|x, y| x.1.total_cmp(&y.1))
                        {
                            let trust = b.trust(a.id, a.family);
                            b.adjust_family_feeling(f, 0.3 * feel * trust);
                        }
                        if a.family != b.family && b.trust(a.id, a.family) > 0.5 {
                            teach = Some((a.family, b.family));
                        }
                    }
                }
                self.stats.talks += 1;
                self.stats.insults += insults;
                if let Some((f, t)) = teach {
                    self.culture.teach(f, t, tick);
                }
            }
            Interaction::Ask { from, to } => {
                let rescue;
                let gave;
                {
                    let Some((a, b)) = self.pair(from, to) else { return };
                    let desperate = a.food < 0.1 && a.health < 0.6;
                    let att = social::attitude(b, a.id, a.family);
                    let give = social::answer_ask(b, att, 1.0 - a.food);
                    gave = give > 0.0;
                    if gave {
                        give_food(b, a, give);
                        a.adjust_affinity(b.id, b.family, 0.2);
                        b.adjust_affinity(a.id, a.family, 0.05);
                        b.prestige += 0.03;
                    } else {
                        a.annoyance = (a.annoyance + 0.15).min(1.0);
                        a.adjust_affinity(b.id, b.family, -0.1);
                        b.annoyance = (b.annoyance + 0.05).min(1.0);
                    }
                    rescue = (gave && desperate).then(|| (b.id, b.family, a.family, (a.x, a.y), b.label()));
                }
                if gave {
                    self.stats.gifts += 1;
                    if let (Some(a), Some(b)) = (self.npc(from), self.npc(to)) {
                        let (fa, fb, place) = (a.family, b.family, (a.x, a.y));
                        self.kindness(fa, fb, place);
                    }
                } else {
                    self.stats.refusals += 1;
                }
                if let Some((hero, hf, fam, place, label)) = rescue {
                    self.rescue(hero, hf, fam, place, &label);
                }
            }
            Interaction::Trade { from, to } => {
                let done;
                {
                    let Some((a, b)) = self.pair(from, to) else { return };
                    let (Some((offered, give)), Some((wanted, _))) = (a.carry, b.carry) else { return };
                    let att = social::attitude(b, a.id, a.family);
                    match social::answer_trade(b, att, offered, give, wanted) {
                        Some(out) => {
                            a.carry = Some((wanted, out));
                            let left = b.carrying(wanted) - out;
                            if left > 0.01 {
                                b.carry = Some((wanted, left));
                                if offered == Item::Food {
                                    b.food = (b.food + give).min(1.0);
                                }
                            } else {
                                b.carry = Some((offered, give));
                            }
                            a.adjust_affinity(b.id, b.family, 0.1);
                            b.adjust_affinity(a.id, a.family, 0.1);
                            done = true;
                        }
                        None => {
                            a.annoyance = (a.annoyance + 0.1).min(1.0);
                            a.adjust_affinity(b.id, b.family, -0.05);
                            done = false;
                        }
                    }
                }
                if done {
                    self.stats.trades += 1;
                    if let (Some(a), Some(b)) = (self.npc(from), self.npc(to)) {
                        let (fa, fb, place) = (a.family, b.family, (a.x, a.y));
                        self.kindness(fa, fb, place);
                    }
                } else {
                    self.stats.refusals += 1;
                }
            }
            Interaction::Share { from, to } => {
                let rescue;
                {
                    let Some((a, b)) = self.pair(from, to) else { return };
                    if a.carrying(Item::Food) < SHARE_AMOUNT && a.food <= 0.6 {
                        return;
                    }
                    let desperate = b.food < 0.1 && b.health < 0.6;
                    give_food(a, b, SHARE_AMOUNT);
                    b.adjust_affinity(a.id, a.family, 0.25);
                    a.prestige += 0.05;
                    rescue = desperate.then(|| (a.id, a.family, b.family, (b.x, b.y), a.label()));
                }
                self.stats.gifts += 1;
                if let (Some(a), Some(b)) = (self.npc(from), self.npc(to)) {
                    let (fa, fb, place) = (a.family, b.family, (a.x, a.y));
                    self.kindness(fa, fb, place);
                }
                if let Some((hero, hf, fam, place, label)) = rescue {
                    self.rescue(hero, hf, fam, place, &label);
                }
            }
            Interaction::Steal { from, to } => {
                let (thief_fam, victim_fam, place);
                {
                    let Some((a, b)) = self.pair(from, to) else { return };
                    let taken = if b.carrying(Item::Food) > 0.05 {
                        take_from_hands(b, STEAL_AMOUNT)
                    } else {
                        let t = b.food.min(STEAL_AMOUNT);
                        b.food -= t;
                        t
                    };
                    feed(a, taken);
                    a.raided = true;
                    b.fear = 1.0;
                    b.adjust_affinity(a.id, a.family, -0.5);
                    b.adjust_family_feeling(a.family, -0.2);
                    (thief_fam, victim_fam, place) = (a.family, b.family, (b.x, b.y));
                }
                self.recent_attacks.push((from, to, tick));
                self.raid_event(from, thief_fam, victim_fam, place);
            }
            Interaction::Mate { from, to } => {
                let id = self.next_id;
                let mut child = None;
                if let Some((a, b)) = self.pair(from, to) {
                    if a.can_mate() && b.can_mate() && social::attitude(b, a.id, a.family) >= -0.1 {
                        let mut c = a.mate(b, id, &mut rng);
                        inherit_stories(&mut c, a, &mut rng);
                        inherit_stories(&mut c, b, &mut rng);
                        a.prestige += 0.02;
                        b.prestige += 0.02;
                        child = Some(c);
                    } else {
                        a.annoyance = (a.annoyance + 0.1).min(1.0);
                    }
                }
                if let Some(c) = child {
                    self.add_child(c);
                    if let (Some(a), Some(b)) = (self.npc(from), self.npc(to)) {
                        let (fa, fb, place) = (a.family, b.family, (a.x, a.y));
                        for _ in 0..3 {
                            self.kindness(fa, fb, place); // a shared child counts a lot
                        }
                    }
                }
            }
            Interaction::Bud { from } => {
                let id = self.next_id;
                let Some(i) = self.index_of(from) else { return };
                if !self.npcs[i].can_bud() {
                    return;
                }
                let mut c = self.npcs[i].bud(id, &mut rng);
                inherit_stories(&mut c, &self.npcs[i], &mut rng);
                self.npcs[i].prestige += 0.02;
                self.add_child(c);
            }
            Interaction::Offer { from, to, place } => {
                let Some(i) = self.index_of(from) else { return };
                let Some((Item::Food, amount)) = self.npcs[i].carry else { return };
                self.npcs[i].carry = None;
                let (fam, who) = (self.npcs[i].family, (self.npcs[i].id, self.npcs[i].generation));
                drop_onto(&mut self.piles, place.0, place.1, Item::Food, amount, fam, who, Vec::new(), false);
                self.stats.offerings += 1;
                if let Some(t) = to.and_then(|id| self.index_of(id)) {
                    self.npcs[t].prestige += 0.05;
                    self.npcs[t].adjust_affinity(from, fam, 0.1);
                }
            }
            Interaction::Raid { thief, victims, place } => {
                let thief_family = self.npc(thief).map(|n| n.family).unwrap_or(0);
                for n in &mut self.npcs {
                    if n.family == victims && dist(n.x, n.y, place.0, place.1) < VIEW {
                        n.adjust_affinity(thief, thief_family, -0.4);
                        n.adjust_family_feeling(thief_family, -0.2);
                    }
                }
                self.raid_event(thief, thief_family, victims, place);
            }
            Interaction::Migrate { from } => {
                // the migrant and whoever follows it found a new home and a new
                // clan, taking their family's know-how and stories along
                let Some(i) = self.index_of(from) else { return };
                let (x, y, old, label) = (self.npcs[i].x, self.npcs[i].y, self.npcs[i].family, self.npcs[i].label());
                let joiners: Vec<usize> = (0..self.npcs.len())
                    .filter(|&k| {
                        let f = &self.npcs[k];
                        k != i && f.following == Some(from) && dist(f.x, f.y, x, y) < VIEW && rng.f32() < f.traits[CONFORMITY]
                    })
                    .collect();
                if joiners.is_empty() {
                    // alone it just settles here, still in its old family
                    self.npcs[i].home = (x, y);
                    return;
                }
                let new = self.n_families as u32;
                self.n_families += 1;
                let know_how = self.culture.families[old as usize].clone();
                self.culture.families.push(know_how);
                self.first_shelter.push(false);
                let moved = joiners.len();
                for k in joiners.into_iter().chain([i]) {
                    self.npcs[k].home = (x, y);
                    self.npcs[k].family = new;
                }
                self.stats.migrations += 1;
                let text = format!("{label} LEFT FAMILY {old} WITH {moved} FOLLOWERS AND FOUNDED FAMILY {new}");
                self.chronicle.record(
                    self.ticks,
                    EventKind::Migration,
                    Subject::Being(from),
                    Some(new),
                    Some(old),
                    (x, y),
                    1.0 + moved as f32,
                    text,
                );
            }
        }
    }

    fn add_child(&mut self, c: Npc) {
        if self.npcs.len() >= MAX_POPULATION {
            return;
        }
        self.next_id += 1;
        self.births += 1;
        self.npcs.push(c);
    }

    fn rescue(&mut self, hero: u64, hero_family: u32, family: u32, place: (f32, f32), label: &str) {
        if let Some(n) = self.npcs.iter_mut().find(|n| n.id == hero) {
            n.prestige += 0.3;
        }
        // the same hero helping the same family again soon: one bigger story
        let since = self.ticks.saturating_sub(DAY_LENGTH);
        if let Some(e) = self.chronicle.recent_mut(EventKind::Rescue, since, |e| {
            e.subject == Subject::Being(hero) && e.affected == Some(family)
        }) {
            e.magnitude = (e.magnitude + 0.2).min(3.0);
            return;
        }
        let text = format!("{label} FED A STARVING MEMBER OF FAMILY {family}");
        self.event(EventKind::Rescue, Subject::Being(hero), Some(hero_family), Some(family), place, 1.0, text);
    }

    /// Something hostile happened between two families.
    fn incident(&mut self, actor: u32, victim: u32, place: (f32, f32)) {
        if actor == victim {
            return;
        }
        let now = self.ticks;
        let k = match self.feuds.iter().position(|f| (f.a, f.b) == (actor, victim) || (f.a, f.b) == (victim, actor)) {
            Some(k) => k,
            None => {
                self.feuds.push(Feud { a: actor, b: victim, incidents: 0, last: now, started_by: actor, event: None, kindness: 0 });
                self.feuds.len() - 1
            }
        };
        let f = &mut self.feuds[k];
        if f.event.is_none() && now.saturating_sub(f.last) > FEUD_COOLS {
            f.incidents = 0; // old trouble forgotten: this is a fresh start
            f.started_by = actor;
        }
        f.incidents += 1;
        f.last = now;
        f.kindness = 0;
        let f = f.clone();
        match f.event {
            Some(id) => {
                self.chronicle.events[id].magnitude += 0.3;
                self.feud_beliefs(k, id, place);
            }
            None if f.incidents >= FEUD_AFTER => {
                let text = format!("FAMILIES {} AND {} ARE AT WAR ({} STRUCK FIRST)", f.a, f.b, f.started_by);
                let id = self.chronicle.record(
                    now,
                    EventKind::Feud,
                    Subject::Family(f.started_by),
                    Some(f.started_by),
                    Some(if f.started_by == f.a { f.b } else { f.a }),
                    place,
                    1.0 + 0.3 * f.incidents as f32,
                    text,
                );
                self.feuds[k].event = Some(id);
                self.feud_beliefs(k, id, place);
            }
            None => {}
        }
    }

    /// Both families remember the feud. Each side's story is about the OTHER
    /// family, so each blames the other, whoever really struck first.
    fn feud_beliefs(&mut self, k: usize, event: usize, place: (f32, f32)) {
        let (a, b) = (self.feuds[k].a, self.feuds[k].b);
        let magnitude = self.chronicle.events[event].magnitude.clamp(0.5, 3.0);
        for n in &mut self.npcs {
            let other = if n.family == a { b } else if n.family == b { a } else { continue };
            n.adjust_family_feeling(other, -0.1);
            chronicle::absorb(
                &mut n.beliefs,
                Belief { event, subject: Subject::Family(other), valence: -1.0, magnitude, hops: 0, place },
            );
        }
    }

    /// Something friendly happened between two families: enough of it ends a feud.
    fn kindness(&mut self, x: u32, y: u32, place: (f32, f32)) {
        if x == y {
            return;
        }
        let Some(k) = self.feuds.iter().position(|f| f.event.is_some() && ((f.a, f.b) == (x, y) || (f.a, f.b) == (y, x)))
        else {
            return;
        };
        self.feuds[k].kindness += 1;
        if self.feuds[k].kindness < PEACE_AFTER {
            return;
        }
        let (a, b) = (self.feuds[k].a, self.feuds[k].b);
        self.feuds[k].event = None;
        self.feuds[k].incidents = 0;
        let text = format!("FAMILIES {a} AND {b} MADE PEACE");
        let id = self.chronicle.record(self.ticks, EventKind::Peace, Subject::Family(a), Some(a), Some(b), place, 1.0, text);
        for n in &mut self.npcs {
            let other = if n.family == a { b } else if n.family == b { a } else { continue };
            n.adjust_family_feeling(other, 0.4);
            chronicle::absorb(
                &mut n.beliefs,
                Belief { event: id, subject: Subject::Family(other), valence: 1.0, magnitude: 1.0, hops: 0, place },
            );
        }
    }

    fn raid_event(&mut self, thief: u64, thief_family: u32, victims: u32, place: (f32, f32)) {
        self.stats.raids += 1;
        self.incident(thief_family, victims, place);
        let since = self.ticks.saturating_sub(DAY_LENGTH);
        if let Some(e) = self.chronicle.recent_mut(EventKind::Raid, since, |e| {
            e.actor_family == Some(thief_family) && e.affected == Some(victims)
        }) {
            e.magnitude += 0.3;
            return;
        }
        let label = self.npc(thief).map(|n| n.label()).unwrap_or_default();
        let text = format!("FAMILY {thief_family} ({label}) ROBBED FAMILY {victims}");
        self.event(EventKind::Raid, Subject::Family(thief_family), Some(thief_family), Some(victims), place, 1.0, text);
    }

    /// Record an event and hand out beliefs to whoever witnessed or was affected.
    #[allow(clippy::too_many_arguments)]
    fn event(
        &mut self,
        kind: EventKind,
        subject: Subject,
        actor_family: Option<u32>,
        affected: Option<u32>,
        place: (f32, f32),
        magnitude: f32,
        text: String,
    ) {
        let id = self.chronicle.record(self.ticks, kind, subject, actor_family, affected, place, magnitude, text);
        for n in &mut self.npcs {
            let near = dist(n.x, n.y, place.0, place.1) < VIEW;
            let ours = affected == Some(n.family);
            let theirs = actor_family == Some(n.family);
            let valence = match kind {
                EventKind::Rescue | EventKind::FirstShelter => {
                    if ours { 1.0 } else if near { 0.3 } else { continue }
                }
                EventKind::Raid | EventKind::Killing => {
                    if ours {
                        if let Some(f) = actor_family {
                            n.adjust_family_feeling(f, -0.15 * magnitude);
                        }
                        -1.0
                    } else if near && !theirs {
                        -0.3
                    } else {
                        continue;
                    }
                }
                EventKind::WolfAttack | EventKind::Famine => {
                    if ours || near { -1.0 } else { continue }
                }
                EventKind::WolfSlain => {
                    if ours || near || theirs { 1.0 } else { continue }
                }
                EventKind::Discovery => {
                    if ours { 1.0 } else { continue }
                }
                EventKind::LeaderDied => {
                    let follower = matches!(subject, Subject::Being(l) if n.following == Some(l));
                    if ours || follower { 1.0 } else { continue }
                }
                EventKind::Migration | EventKind::Feud | EventKind::Peace => continue,
            };
            let hops = if near { 0 } else { 1 };
            chronicle::absorb(
                &mut n.beliefs,
                Belief { event: id, subject, valence, magnitude: magnitude.clamp(0.5, 3.0), hops, place },
            );
        }
    }

    fn resolve_payoffs(&mut self, payoffs: Vec<Payoff>, day: u64) {
        for payoff in payoffs {
            let (k, d) = match payoff {
                Payoff::Sprouted { pile } => (pile, Discovery::Seeds),
                Payoff::KeptWarm { pile, family } | Payoff::Fed { pile, family } => {
                    let (d, slot) = if matches!(payoff, Payoff::KeptWarm { .. }) {
                        (Discovery::Warmth, 0)
                    } else {
                        (Discovery::Pantry, 1)
                    };
                    let Some(p) = self.piles.get_mut(pile) else { continue };
                    let too_fresh = d == Discovery::Pantry && p.age < PANTRY_AGE;
                    if p.family != family || p.credited_day[slot] == day || too_fresh {
                        continue;
                    }
                    p.credited_day[slot] = day;
                    (pile, d)
                }
            };
            let Some(p) = self.piles.get(k) else { continue };
            if !p.learns {
                continue;
            }
            let (family, dropper, place, memory) = (p.family, p.dropper, (p.x, p.y), p.memory.clone());
            if self.culture.reward(family, d, &memory, self.ticks, dropper) {
                if let Some(n) = self.npcs.iter_mut().find(|n| n.id == dropper.0) {
                    n.prestige += 0.3;
                }
                let label = self.npc(dropper.0).map(|n| n.label()).unwrap_or(format!("#{}", dropper.0));
                let text = format!("{label} OF FAMILY {family}: {}", d.fact());
                self.event(EventKind::Discovery, Subject::Being(dropper.0), Some(family), Some(family), place, 1.0, text);
            }
        }
    }

    fn check_shelters(&mut self) {
        let mut new = Vec::new();
        for p in &mut self.piles {
            if p.is_shelter() && !p.announced {
                p.announced = true;
                let f = p.family as usize;
                if f < self.first_shelter.len() && !self.first_shelter[f] {
                    self.first_shelter[f] = true;
                    new.push((p.family, p.dropper.0, (p.x, p.y)));
                }
            }
        }
        for (family, builder, place) in new {
            let label = self.npc(builder).map(|n| n.label()).unwrap_or(format!("#{builder}"));
            let text = format!("{label} BUILT FAMILY {family}'S FIRST SHELTER");
            self.event(EventKind::FirstShelter, Subject::Being(builder), Some(family), Some(family), place, 1.0, text);
        }
        self.piles.retain(|p| p.amount > 0.01);
    }

    fn remove_dead_animals(&mut self) {
        let dead: Vec<(Species, f32, f32, Option<u64>, u64)> = self
            .animals
            .iter()
            .filter(|a| a.is_dead())
            .map(|a| (a.species, a.x, a.y, a.killed_by.filter(|_| a.health <= 0.0), a.id))
            .collect();
        self.animals.retain(|a| !a.is_dead());
        for (species, x, y, killer, _) in dead {
            let Some(k) = killer.and_then(|id| self.index_of(id)) else { continue };
            let (fam, kid_gen, kid, label) = (self.npcs[k].family, self.npcs[k].generation, self.npcs[k].id, self.npcs[k].label());
            match species {
                Species::Deer => {
                    drop_onto(&mut self.piles, x, y, Item::Food, MEAT, fam, (kid, kid_gen), Vec::new(), false);
                    self.npcs[k].prestige += 0.1;
                    self.stats.deer_hunted += 1;
                }
                Species::Wolf => {
                    self.npcs[k].prestige += 0.4;
                    self.npcs[k].wins += 1;
                    self.stats.wolves_slain += 1;
                    let text = format!("{label} OF FAMILY {fam} KILLED A WOLF");
                    self.event(EventKind::WolfSlain, Subject::Being(kid), Some(fam), Some(fam), (x, y), 1.5, text);
                }
            }
        }
    }

    fn remove_dead_npcs(&mut self, night: bool) {
        let now = self.ticks;
        let mut dead: Vec<(usize, DeathCause)> = Vec::new();
        for (i, n) in self.npcs.iter().enumerate() {
            if !n.is_dead() {
                continue;
            }
            let recent = n.last_hurt.filter(|&(_, t)| now - t <= 5).map(|h| h.0);
            let cause = if n.age >= n.lifespan {
                DeathCause::OldAge
            } else if let Some(Hurt::Npc(_)) = recent {
                DeathCause::Fight
            } else if let Some(Hurt::Wolf(_)) = recent {
                DeathCause::Wolf
            } else if n.food <= 0.0 || n.water <= 0.0 {
                DeathCause::Starved
            } else if n.fatigue >= 1.0 {
                DeathCause::Exhausted
            } else if night && n.perception.cold > 0.0 {
                DeathCause::Cold
            } else {
                DeathCause::Fight
            };
            dead.push((i, cause));
        }
        if dead.is_empty() {
            return;
        }

        let mut events = Vec::new();
        for &(i, cause) in &dead {
            self.deaths_by[cause as usize] += 1;
            let n = &self.npcs[i];
            let place = (n.x, n.y);
            match (cause, n.last_hurt) {
                (DeathCause::Fight, Some((Hurt::Npc(killer), _))) => {
                    let kf = self.npc(killer).map(|k| k.family);
                    let kl = self.npc(killer).map(|k| k.label()).unwrap_or(format!("#{killer}"));
                    let text = format!("{kl} KILLED {} OF FAMILY {}", n.label(), n.family);
                    events.push((EventKind::Killing, Subject::Being(killer), kf, Some(n.family), place, 1.5, text));
                    self.stats.kills += 1;
                    if let Some(k) = self.npcs.iter_mut().find(|k| k.id == killer) {
                        k.wins += 1;
                    }
                }
                (DeathCause::Wolf, _) => {
                    let text = format!("WOLVES KILLED {} OF FAMILY {}", n.label(), n.family);
                    events.push((EventKind::WolfAttack, Subject::Wolves, None, Some(n.family), place, 1.5, text));
                }
                (DeathCause::Starved, _) => {
                    let day = self.day();
                    let entry = match self.starvation_today.iter_mut().find(|e| e.0 == n.family && e.1 == day) {
                        Some(e) => e,
                        None => {
                            self.starvation_today.push((n.family, day, 0));
                            self.starvation_today.last_mut().unwrap()
                        }
                    };
                    entry.2 += 1;
                    if entry.2 == 3 {
                        let text = format!("FAMINE: FAMILY {} LOST 3 TO HUNGER IN ONE DAY", n.family);
                        events.push((EventKind::Famine, Subject::Place(n.home.0, n.home.1), None, Some(n.family), n.home, 2.0, text));
                    }
                }
                _ => {}
            }
            let n = &self.npcs[i];
            if n.followers >= LEGENDARY_LEADER {
                let text = format!("{} DIED, A LEADER WITH {} FOLLOWERS", n.label(), n.followers);
                let m = 1.0 + n.followers as f32 * 0.2;
                events.push((EventKind::LeaderDied, Subject::Being(n.id), Some(n.family), Some(n.family), place, m, text));
            }
        }
        for (kind, subject, actor, affected, place, mag, text) in events {
            if let (EventKind::Killing, Some(a), Some(v)) = (kind, actor, affected) {
                self.incident(a, v, place);
                self.incident(a, v, place); // a killing counts double
            }
            self.event(kind, subject, actor, affected, place, mag, text);
        }
        let day = self.day();
        self.starvation_today.retain(|e| e.1 == day);
        let dead_idx: Vec<usize> = dead.iter().map(|d| d.0).collect();
        let mut k = 0;
        self.npcs.retain(|_| {
            let keep = !dead_idx.contains(&k);
            k += 1;
            keep
        });
    }

    // =======================================================================
    // Stats
    // =======================================================================

    pub fn average_genes(&self) -> Option<GoalGenes> {
        if self.npcs.is_empty() {
            return None;
        }
        let mut avg = [[0.0; N_STATES]; N_GOALS];
        for n in &self.npcs {
            for g in 0..N_GOALS {
                for s in 0..N_STATES {
                    avg[g][s] += n.genes[g][s];
                }
            }
        }
        let c = self.npcs.len() as f32;
        avg.iter_mut().flatten().for_each(|v| *v /= c);
        Some(avg)
    }

    pub fn average_traits(&self) -> Traits {
        let mut avg = [0.0; N_TRAITS];
        for n in &self.npcs {
            for (a, t) in avg.iter_mut().zip(n.traits.iter()) {
                *a += t;
            }
        }
        let c = self.npcs.len().max(1) as f32;
        avg.iter_mut().for_each(|v| *v /= c);
        avg
    }

    pub fn goal_counts(&self) -> [usize; N_GOALS] {
        let mut c = [0; N_GOALS];
        for n in &self.npcs {
            if let Some(g) = n.goal() {
                c[g.index()] += 1;
            }
        }
        c
    }

    pub fn families_alive(&self) -> usize {
        let mut alive = vec![false; self.n_families];
        for n in &self.npcs {
            alive[n.family as usize] = true;
        }
        alive.iter().filter(|&&a| a).count()
    }

    pub fn shelters(&self) -> usize {
        self.piles.iter().filter(|p| p.is_shelter()).count()
    }

    pub fn planted_bushes(&self) -> usize {
        self.bushes.iter().filter(|b| b.planted).count()
    }

    pub fn animal_count(&self, s: Species) -> usize {
        self.animals.iter().filter(|a| a.species == s).count()
    }

    pub fn leaders(&self) -> Vec<&Npc> {
        let mut l: Vec<&Npc> = self.npcs.iter().filter(|n| n.followers >= 3).collect();
        l.sort_by(|a, b| b.followers.cmp(&a.followers));
        l
    }

    pub fn active_feuds(&self) -> Vec<&Feud> {
        self.feuds.iter().filter(|f| f.event.is_some()).collect()
    }

    pub fn max_generation(&self) -> u32 {
        self.npcs.iter().map(|n| n.generation).max().unwrap_or(0)
    }

    pub fn npc_at(&self, x: f32, y: f32, radius: f32) -> Option<u64> {
        self.npcs
            .iter()
            .map(|n| (n.id, dist(x, y, n.x, n.y)))
            .filter(|v| v.1 <= radius)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|v| v.0)
    }
}

// ===========================================================================
// Running primitives
// ===========================================================================

struct Ctx<'a> {
    bushes: &'a mut Vec<Bush>,
    trees: &'a mut Vec<Tree>,
    water: &'a mut Vec<Water>,
    piles: &'a mut Vec<Pile>,
    positions: &'a HashMap<u64, (f32, f32)>,
    interactions: &'a mut Vec<Interaction>,
    hits: &'a mut Vec<(Target, u64, f32)>,
    payoffs: &'a mut Vec<Payoff>,
    sheltered: bool,
    rng: &'a mut fastrand::Rng,
    w: f32,
    h: f32,
}

fn walk(n: &mut Npc, tx: f32, ty: f32, reach: f32, speed: f32, dt: f32) -> bool {
    let (dx, dy) = (tx - n.x, ty - n.y);
    let d = (dx * dx + dy * dy).sqrt();
    if d <= reach {
        return true;
    }
    let step = (speed * dt).min(d);
    n.x += dx / d * step;
    n.y += dy / d * step;
    false
}

fn target_pos(t: Target, c: &Ctx) -> Option<(f32, f32)> {
    match t {
        Target::Pos(x, y) => Some((x, y)),
        Target::Npc(id) | Target::Animal(id) => c.positions.get(&id).copied(),
        Target::Bush(k) => c.bushes.get(k).map(|b| (b.x, b.y)),
        Target::Pile(k) => c.piles.get(k).map(|p| (p.x, p.y)),
        Target::Water(k) => c.water.get(k).map(|w| (w.x, w.y)),
        Target::Tree(k) => c.trees.get(k).map(|t| (t.x, t.y)),
        Target::Here => None,
    }
}

/// Target for the next step of a multi-step method.
fn next_target(n: &Npc, p: Prim, a: &Active, piles: &[Pile], bushes: &[Bush], trees: &[Tree], water: &[Water]) -> Target {
    let near = |xs: &mut dyn Iterator<Item = (usize, f32, f32)>| -> Option<usize> {
        xs.min_by(|a, b| dist(n.x, n.y, a.1, a.2).total_cmp(&dist(n.x, n.y, b.1, b.2))).map(|v| v.0)
    };
    match p {
        Prim::GoHome | Prim::Guard => Target::Pos(n.home.0, n.home.1),
        Prim::PickFood | Prim::EatBush => {
            near(&mut bushes.iter().enumerate().filter(|(_, b)| b.amount > 0.05).map(|(k, b)| (k, b.x, b.y)))
                .map_or(Target::Here, Target::Bush)
        }
        Prim::PickWood => near(&mut trees.iter().enumerate().filter(|(_, t)| t.amount > 0.05).map(|(k, t)| (k, t.x, t.y)))
            .map_or(Target::Here, Target::Tree),
        Prim::EatPile => near(
            &mut piles
                .iter()
                .enumerate()
                .filter(|(_, p)| p.item == Item::Food && p.family == n.family && p.amount > 0.05)
                .map(|(k, p)| (k, p.x, p.y)),
        )
        .map_or(Target::Here, Target::Pile),
        Prim::Drink => near(&mut water.iter().enumerate().map(|(k, w)| (k, w.x, w.y))).map_or(Target::Here, Target::Water),
        Prim::Attack => a.target,
        _ => Target::Here,
    }
}

fn run(n: &mut Npc, a: &mut Active, c: &mut Ctx, dt: f32) -> Status {
    let speed = SPEED * n.speed_factor();
    let p = a.prim();
    let pos = target_pos(a.target, c);
    let go = |n: &mut Npc, reach: f32, mult: f32| -> Option<bool> { pos.map(|(x, y)| walk(n, x, y, reach, speed * mult, dt)) };

    match p {
        Prim::EatHand => {
            let bite = (EAT_RATE * dt).min(n.carrying(Item::Food)).min(1.0 - n.food);
            take_from_hands(n, bite);
            n.food += bite;
            if n.carrying(Item::Food) <= 0.01 || n.food >= 0.98 { Status::Done } else { Status::Running }
        }
        Prim::EatBush | Prim::PickFood => {
            let Target::Bush(k) = a.target else { return Status::Failed };
            if c.bushes.get(k).is_none_or(|b| b.amount <= 0.01) {
                return Status::Failed;
            }
            if go(n, REACH, 1.0) != Some(true) {
                return Status::Running;
            }
            let b = &mut c.bushes[k];
            if p == Prim::EatBush {
                let bite = (EAT_RATE * dt).min(b.amount).min(1.0 - n.food);
                b.amount -= bite;
                n.food += bite;
                if n.food >= 0.98 || b.amount <= 0.01 { Status::Done } else { Status::Running }
            } else {
                let got = (GATHER_RATE * dt).min(b.amount).min(CARRY_CAP - n.carried());
                b.amount -= got;
                put_in_hands(n, Item::Food, got);
                if n.carried() >= CARRY_CAP - 0.01 || b.amount <= 0.01 { Status::Done } else { Status::Running }
            }
        }
        Prim::EatPile | Prim::StealPile => {
            let Target::Pile(k) = a.target else { return Status::Failed };
            if c.piles.get(k).is_none_or(|p| p.amount <= 0.01 || p.item != Item::Food) {
                return Status::Failed;
            }
            if go(n, REACH, 1.0) != Some(true) {
                return Status::Running;
            }
            let pile = &mut c.piles[k];
            if p == Prim::EatPile {
                let bite = (EAT_RATE * dt).min(pile.amount).min(1.0 - n.food);
                pile.amount -= bite;
                n.food += bite;
                c.payoffs.push(Payoff::Fed { pile: k, family: n.family });
                if n.food >= 0.98 || pile.amount <= 0.01 { Status::Done } else { Status::Running }
            } else {
                if !n.raided {
                    n.raided = true;
                    c.interactions.push(Interaction::Raid { thief: n.id, victims: pile.family, place: (pile.x, pile.y) });
                }
                let got = (GATHER_RATE * dt).min(pile.amount).min(CARRY_CAP - n.carried());
                pile.amount -= got;
                put_in_hands(n, Item::Food, got);
                if n.carried() >= CARRY_CAP - 0.01 || pile.amount <= 0.01 { Status::Done } else { Status::Running }
            }
        }
        Prim::Drink => match go(n, REACH, 1.0) {
            Some(true) => {
                n.water += DRINK_RATE * dt;
                if n.water >= 0.98 { Status::Done } else { Status::Running }
            }
            Some(false) => Status::Running,
            None => Status::Failed,
        },
        Prim::Rest => {
            let bonus = if c.sheltered { SHELTER_REST_BONUS } else { 1.0 };
            n.fatigue -= REST_RATE * bonus * dt;
            if n.fatigue <= 0.05 { Status::Done } else { Status::Running }
        }
        Prim::GoShelter | Prim::Wander | Prim::GoHome | Prim::Flee => {
            let mult = if p == Prim::Flee { 1.25 } else { 1.0 };
            let reach = if p == Prim::GoHome { HOME_REACH } else { 6.0 };
            match go(n, reach, mult) {
                Some(true) => {
                    if p == Prim::Wander && dist(n.x, n.y, n.home.0, n.home.1) > MIGRATE_DISTANCE {
                        let chance = 0.15 * n.traits[AMBITION] + if n.wanderer { 0.0 } else { 0.02 };
                        if c.rng.f32() < chance && !n.wanderer {
                            c.interactions.push(Interaction::Migrate { from: n.id });
                        }
                    }
                    Status::Done
                }
                Some(false) => Status::Running,
                None => Status::Failed,
            }
        }
        Prim::PickWood => {
            let Target::Tree(k) = a.target else { return Status::Failed };
            if c.trees.get(k).is_none_or(|t| t.amount <= 0.05) {
                return Status::Failed;
            }
            if go(n, REACH, 1.0) != Some(true) {
                return Status::Running;
            }
            let t = &mut c.trees[k];
            let got = (GATHER_RATE * dt).min(t.amount).min(CARRY_CAP - n.carried());
            t.amount -= got;
            put_in_hands(n, Item::Wood, got);
            if n.carried() >= CARRY_CAP - 0.01 || t.amount <= 0.05 { Status::Done } else { Status::Running }
        }
        Prim::Drop => {
            if let Some((item, amount)) = n.carry.take() {
                let learns = n.mind != Mind::Simple;
                drop_onto(c.piles, n.x, n.y, item, amount, n.family, (n.id, n.generation), n.memory.clone(), learns);
            }
            Status::Done
        }
        Prim::FollowKin | Prim::FollowLeader => match go(n, 20.0, 1.0) {
            Some(true) => Status::Done,
            Some(false) => Status::Running,
            None => Status::Failed,
        },
        Prim::Talk | Prim::Ask | Prim::Trade | Prim::Share | Prim::Steal | Prim::Mate => {
            let Target::Npc(to) = a.target else { return Status::Failed };
            match go(n, TALK_REACH, 1.0) {
                Some(true) => {
                    let from = n.id;
                    c.interactions.push(match p {
                        Prim::Talk => Interaction::Talk { from, to },
                        Prim::Ask => Interaction::Ask { from, to },
                        Prim::Trade => Interaction::Trade { from, to },
                        Prim::Share => Interaction::Share { from, to },
                        Prim::Steal => Interaction::Steal { from, to },
                        _ => Interaction::Mate { from, to },
                    });
                    Status::Done
                }
                Some(false) if a.timer > 15.0 => Status::Failed,
                Some(false) => Status::Running,
                None => Status::Failed,
            }
        }
        Prim::Attack => match go(n, REACH, 1.1) {
            Some(true) => {
                let dmg = FIGHT_DAMAGE * (0.5 + n.traits[STRENGTH]) * (0.5 + 0.5 * n.health) * dt;
                c.hits.push((a.target, n.id, dmg));
                Status::Running
            }
            Some(false) if a.timer > 20.0 => Status::Failed,
            Some(false) => Status::Running,
            None => Status::Done, // target is gone (dead or fled out of the world)
        },
        Prim::Bud => {
            c.interactions.push(Interaction::Bud { from: n.id });
            Status::Done
        }
        Prim::Offer => match (a.target, go(n, 15.0, 1.0)) {
            (Target::Npc(id), Some(true)) => {
                let place = (n.x, n.y);
                c.interactions.push(Interaction::Offer { from: n.id, to: Some(id), place });
                Status::Done
            }
            (_, Some(true)) => {
                c.interactions.push(Interaction::Offer { from: n.id, to: None, place: (n.x, n.y) });
                Status::Done
            }
            (_, Some(false)) => Status::Running,
            _ => Status::Failed,
        },
        Prim::Rally => {
            n.rallying = 8.0;
            Status::Done
        }
        Prim::Guard => match go(n, HOME_REACH, 1.0) {
            Some(true) if a.timer > 8.0 => Status::Done,
            Some(_) => Status::Running,
            None => Status::Failed,
        },
    }
    .clamp_world(n, c.w, c.h)
}

trait ClampWorld {
    fn clamp_world(self, n: &mut Npc, w: f32, h: f32) -> Self;
}

impl ClampWorld for Status {
    fn clamp_world(self, n: &mut Npc, w: f32, h: f32) -> Self {
        n.x = n.x.clamp(5.0, w - 5.0);
        n.y = n.y.clamp(5.0, h - 5.0);
        self
    }
}

#[allow(clippy::too_many_arguments)]
fn drop_onto(
    piles: &mut Vec<Pile>,
    x: f32,
    y: f32,
    item: Item,
    amount: f32,
    family: u32,
    dropper: (u64, u32),
    memory: Vec<Prim>,
    learns: bool,
) {
    let existing = piles
        .iter_mut()
        .filter(|p| p.item == item && dist(x, y, p.x, p.y) <= PILE_MERGE)
        .min_by(|a, b| dist(x, y, a.x, a.y).total_cmp(&dist(x, y, b.x, b.y)));
    match existing {
        Some(p) => {
            p.amount += amount;
            p.family = family;
            p.dropper = dropper;
            p.memory = memory;
            p.learns = learns;
        }
        None => piles.push(Pile {
            x,
            y,
            item,
            amount,
            family,
            dropper,
            memory,
            learns,
            credited_day: [u64::MAX; 2],
            age: 0.0,
            announced: false,
        }),
    }
}

fn put_in_hands(n: &mut Npc, item: Item, amount: f32) {
    let held = n.carrying(item); // a crumb of something else is just dropped
    n.carry = Some((item, (held + amount).min(CARRY_CAP)));
}

fn take_from_hands(n: &mut Npc, want: f32) -> f32 {
    let Some((item, amount)) = n.carry else { return 0.0 };
    let got = want.min(amount).max(0.0);
    let left = amount - got;
    n.carry = if left > 0.02 { Some((item, left)) } else { None };
    got
}

/// Food goes into the stomach; extra goes into free hands.
fn feed(n: &mut Npc, amount: f32) {
    let eaten = amount.min(1.0 - n.food);
    n.food += eaten;
    if n.carry.is_none_or(|(i, _)| i == Item::Food) {
        put_in_hands(n, Item::Food, amount - eaten);
    }
}

fn give_food(giver: &mut Npc, receiver: &mut Npc, amount: f32) {
    if giver.carrying(Item::Food) >= amount {
        take_from_hands(giver, amount);
    } else {
        giver.food -= amount;
    }
    feed(receiver, amount);
}

/// Each tells the other its biggest story.
fn swap_stories(a: &mut Npc, b: &mut Npc, rng: &mut fastrand::Rng) {
    let top = |n: &Npc| n.beliefs.iter().max_by(|x, y| x.magnitude.total_cmp(&y.magnitude)).copied();
    let (ta, tb) = (top(a), top(b));
    let present = |sub: Subject, x: &Npc, y: &Npc| matches!(sub, Subject::Being(id) if id == x.id || id == y.id);
    if let Some(s) = ta {
        let trust = b.trust(a.id, a.family);
        if let Some(t) = chronicle::retell(&s, trust, present(s.subject, a, b), rng) {
            if let Subject::Family(f) = t.subject {
                if t.valence < 0.0 {
                    b.adjust_family_feeling(f, -0.05 * t.magnitude);
                }
            }
            chronicle::absorb(&mut b.beliefs, t);
        }
    }
    if let Some(s) = tb {
        let trust = a.trust(b.id, b.family);
        if let Some(t) = chronicle::retell(&s, trust, present(s.subject, a, b), rng) {
            chronicle::absorb(&mut a.beliefs, t);
        }
    }
}

/// A child hears its parent's stories (retold, so distorted).
fn inherit_stories(child: &mut Npc, parent: &Npc, rng: &mut fastrand::Rng) {
    for b in &parent.beliefs {
        if let Some(t) = chronicle::retell(b, 1.0, false, rng) {
            chronicle::absorb(&mut child.beliefs, t);
        }
    }
    for &(f, feel) in &parent.family_feelings {
        child.adjust_family_feeling(f, feel * 0.7);
    }
}
