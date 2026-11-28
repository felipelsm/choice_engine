// An NPC: body, genes (goal matrix + traits), mind tier, goal stack, active
// method, relationships, beliefs.
//
// Three kinds of personality, kept separate:
//   - GOAL GENES: a goals x states matrix, "what matters to me". Like the old
//     personality matrix, but it picks goals instead of actions.
//   - STYLE TRAITS: aggression, courage, sociability... "how I go about it".
//     They pick methods and scale goals, never needs.
//   - META TRAITS: stubbornness, deliberation, speed, patience, curiosity,
//     memory. "How I make decisions at all".

use crate::chronicle::Belief;
use crate::goals::{random_goal_genes, GoalFrame, N_GOALS};
use crate::methods::{Active, Item, Prim};

// ---------------------------------------------------------------------------
// States (goal inputs). Every state is 0..1.
// ---------------------------------------------------------------------------

pub const N_STATES: usize = 16;

pub const HUNGER: usize = 0;
pub const THIRST: usize = 1;
pub const FATIGUE: usize = 2;
pub const HEALTH: usize = 3;
pub const AGE: usize = 4;
pub const COLD: usize = 5; //        night and not near a shelter
pub const FEAR: usize = 6; //        spikes when attacked, fades
pub const LONELINESS: usize = 7;
pub const CARRYING: usize = 8;
pub const NEAR_HOME: usize = 9;
pub const THREAT: usize = 10; //     a wolf or attacker close to me
pub const KIN_THREAT: usize = 11; // family or a friend under attack nearby
pub const GRUDGE: usize = 12; //     someone I hate (or a family I hate) is near
pub const PREY_NEAR: usize = 13;
pub const REVERENCE: usize = 14; //  a being I revere is near
pub const BIAS: usize = 15; //       always 1

pub const STATE_NAMES: [&str; N_STATES] = [
    "HUNGER", "THIRST", "FATIGUE", "HEALTH", "AGE", "COLD", "FEAR", "LONELINESS", "CARRYING",
    "NEAR HOME", "THREAT", "KIN THREATENED", "GRUDGE NEAR", "PREY NEAR", "REVERED NEAR", "BIAS (ALWAYS 1)",
];
pub const STATE_SHORT: [&str; N_STATES] =
    ["HU", "TH", "FA", "HE", "AG", "CO", "FE", "LO", "CA", "NH", "TR", "KT", "GR", "PR", "RV", "1"];

pub type StateVec = [f32; N_STATES];
pub type GoalGenes = [[f32; N_STATES]; N_GOALS];

// ---------------------------------------------------------------------------
// Traits, all 0..1
// ---------------------------------------------------------------------------

pub const N_TRAITS: usize = 16;

// meta: how decisions get made
pub const STUBBORNNESS: usize = 0; // margin a new goal needs to take over
pub const DELIBERATION: usize = 1; // how long close calls make it hesitate
pub const SPEED: usize = 2; //        walking speed 0.7x .. 1.3x
pub const PATIENCE: usize = 3; //     how long before giving up on a goal
pub const CURIOSITY: usize = 4; //    chance to try a random action (play)
pub const MEMORY: usize = 5; //       how many recent actions it remembers
// style: how goals get pursued
pub const AGGRESSION: usize = 6;
pub const COURAGE: usize = 7;
pub const VENGEFULNESS: usize = 8; // how slowly grudges fade
pub const LOYALTY: usize = 9; //      joins fights for family and friends
pub const SOCIABILITY: usize = 10;
pub const THRIFT: usize = 11; //      keeps and stores food rather than sharing
pub const CHARISMA: usize = 12; //    how strongly others are pulled by its choices
pub const AMBITION: usize = 13; //    seeks prestige, rallies, leaves to found villages
pub const CONFORMITY: usize = 14; //  how much it follows others
// body
pub const STRENGTH: usize = 15; //    fights harder, but costs more food

pub const TRAIT_NAMES: [&str; N_TRAITS] = [
    "STUBBORNNESS", "DELIBERATION", "SPEED", "PATIENCE", "CURIOSITY", "MEMORY", "AGGRESSION",
    "COURAGE", "VENGEFULNESS", "LOYALTY", "SOCIABILITY", "THRIFT", "CHARISMA", "AMBITION",
    "CONFORMITY", "STRENGTH",
];
pub const TRAIT_SHORT: [&str; N_TRAITS] =
    ["STB", "DLB", "SPD", "PAT", "CUR", "MEM", "AGR", "COU", "VEN", "LOY", "SOC", "THR", "CHA", "AMB", "CNF", "STR"];

pub type Traits = [f32; N_TRAITS];

pub fn trait_index(name: &str) -> Option<usize> {
    let upper = name.to_ascii_uppercase();
    TRAIT_NAMES.iter().position(|&n| n == upper)
}

// ---------------------------------------------------------------------------
// Mind tiers
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mind {
    /// Primitives only, doesn't use or add to family culture.
    Simple,
    /// Primitives + family methods, learns from outcomes.
    Villager,
    /// + authored methods, can plan (fetches what a method needs first).
    Story,
}

impl Mind {
    pub fn name(self) -> &'static str {
        match self {
            Mind::Simple => "SIMPLE",
            Mind::Villager => "VILLAGER",
            Mind::Story => "STORY",
        }
    }
}

// ---------------------------------------------------------------------------
// Tuning
// ---------------------------------------------------------------------------

pub const MUTATION: f32 = 0.05; //       goal genes: scaled by up to +-5%
pub const TRAIT_MUTATION: f32 = 0.05; // traits: moved by up to +-0.05
pub const MATE_COST: f32 = 0.2; //       food and water, each parent
pub const BUD_COST: f32 = 0.3; //        food and water, alone
pub const MATURITY: f32 = 20.0;
pub const REPRODUCE_COOLDOWN: f32 = 15.0;
pub const MIN_LIFESPAN: f32 = 200.0;
pub const MAX_LIFESPAN: f32 = 300.0;
pub const CARRY_CAP: f32 = 1.0;
pub const MAX_RELATIONS: usize = 8;

/// Who hurt an NPC last (for causes of death and grudges).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hurt {
    Npc(u64),
    Wolf(u64),
}

/// How one NPC feels about another (positive = friend, negative = grudge).
#[derive(Clone, Copy, Debug)]
pub struct Relation {
    pub id: u64,
    pub family: u32,
    pub affinity: f32, // -1 .. 1
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Perception {
    pub crowd: f32, // others within arm's reach (0 = alone)
    pub cold: f32,
    pub threat: f32,
    pub kin_threat: f32,
    pub grudge: f32,
    pub prey_near: f32,
    pub reverence: f32,
    pub near_home: f32,
}

pub struct Npc {
    pub id: u64,
    pub name: Option<String>,
    pub x: f32,
    pub y: f32,

    // body
    pub food: f32,
    pub water: f32,
    pub fatigue: f32,
    pub health: f32,
    pub age: f32,
    pub lifespan: f32,
    pub cooldown: f32,
    pub carry: Option<(Item, f32)>,

    // identity
    pub family: u32,
    pub generation: u32,
    pub home: (f32, f32),
    pub mind: Mind,
    pub genes: GoalGenes,
    pub traits: Traits,
    pub own_methods: Vec<crate::methods::Method>, // authored, story NPCs only

    // deciding
    pub stack: Vec<GoalFrame>,
    pub active: Option<Active>,
    pub hesitation: f32,
    pub playing: bool,
    pub goal_scores: [f32; N_GOALS],
    pub goal_cooldown: [f32; N_GOALS],
    pub why: Vec<(String, f32)>, // biggest reasons behind the current goal
    pub memory: Vec<Prim>,       // recent primitives, newest last
    pub perception: Perception,

    // social
    pub relations: Vec<Relation>,
    pub family_feelings: Vec<(u32, f32)>,
    pub beliefs: Vec<Belief>,
    pub loneliness: f32,
    pub fear: f32,
    pub annoyance: f32, // mood: rises with insults / refusals / nagging, calms down over time

    // status
    pub prestige: f32,
    pub wins: u32,
    pub following: Option<u64>,
    pub followers: u32,
    pub rallying: f32,
    pub raided: bool, // got food on the current raid
    pub last_hurt: Option<(Hurt, u64)>, // by whom, at which tick
    pub wanderer: bool,                 // story NPCs that roam
}

impl Npc {
    pub fn new(id: u64, x: f32, y: f32, family: u32, mind: Mind, rng: &mut fastrand::Rng) -> Self {
        let mut traits = [0.0; N_TRAITS];
        for t in traits.iter_mut() {
            *t = rng.f32();
        }
        Self {
            id,
            name: None,
            x,
            y,
            food: 0.5 + rng.f32() * 0.5,
            water: 0.5 + rng.f32() * 0.5,
            fatigue: rng.f32() * 0.4,
            health: 1.0,
            age: MATURITY + rng.f32() * 30.0,
            lifespan: MIN_LIFESPAN + rng.f32() * (MAX_LIFESPAN - MIN_LIFESPAN),
            cooldown: 0.0,
            carry: None,
            family,
            generation: 0,
            home: (x, y),
            mind,
            genes: random_goal_genes(rng),
            traits,
            own_methods: Vec::new(),
            stack: Vec::new(),
            active: None,
            hesitation: 0.0,
            playing: false,
            goal_scores: [0.0; N_GOALS],
            goal_cooldown: [0.0; N_GOALS],
            why: Vec::new(),
            memory: Vec::new(),
            perception: Perception::default(),
            relations: Vec::new(),
            family_feelings: Vec::new(),
            beliefs: Vec::new(),
            loneliness: 0.0,
            fear: 0.0,
            annoyance: 0.0,
            prestige: 0.0,
            wins: 0,
            following: None,
            followers: 0,
            rallying: 0.0,
            raided: false,
            last_hurt: None,
            wanderer: false,
        }
    }

    pub fn label(&self) -> String {
        match &self.name {
            Some(n) => n.to_ascii_uppercase(),
            None => format!("#{}", self.id),
        }
    }

    pub fn carrying(&self, item: Item) -> f32 {
        match self.carry {
            Some((i, a)) if i == item => a,
            _ => 0.0,
        }
    }

    pub fn carried(&self) -> f32 {
        self.carry.map_or(0.0, |(_, a)| a)
    }

    pub fn is_adult(&self) -> bool {
        self.age >= MATURITY
    }

    pub fn is_dead(&self) -> bool {
        self.health <= 0.0 || self.age >= self.lifespan
    }

    pub fn can_mate(&self) -> bool {
        self.is_adult() && self.cooldown <= 0.0 && self.food >= MATE_COST && self.water >= MATE_COST
    }

    pub fn can_bud(&self) -> bool {
        self.is_adult() && self.cooldown <= 0.0 && self.food >= BUD_COST && self.water >= BUD_COST
    }

    pub fn goal(&self) -> Option<crate::goals::Goal> {
        self.stack.last().map(|f| f.goal)
    }

    pub fn memory_len(&self) -> usize {
        match self.mind {
            Mind::Simple => 3,
            Mind::Villager => 4 + (self.traits[MEMORY] * 12.0) as usize,
            Mind::Story => 20,
        }
    }

    pub fn speed_factor(&self) -> f32 {
        0.7 + 0.6 * self.traits[SPEED]
    }

    pub fn dominance(&self) -> f32 {
        self.traits[STRENGTH] * (self.wins as f32 / 5.0).min(1.0)
    }

    /// How strongly others are pulled toward this NPC's choices.
    pub fn influence(&self) -> f32 {
        let rally = if self.rallying > 0.0 { 2.0 } else { 1.0 };
        self.traits[CHARISMA] * (self.prestige + self.dominance()) * rally
    }

    pub fn state(&self) -> StateVec {
        let p = &self.perception;
        let mut s = [0.0; N_STATES];
        s[HUNGER] = 1.0 - self.food;
        s[THIRST] = 1.0 - self.water;
        s[FATIGUE] = self.fatigue;
        s[HEALTH] = self.health;
        s[AGE] = (self.age / self.lifespan).min(1.0);
        s[COLD] = p.cold;
        s[FEAR] = self.fear;
        s[LONELINESS] = self.loneliness;
        s[CARRYING] = self.carried() / CARRY_CAP;
        s[NEAR_HOME] = p.near_home;
        s[THREAT] = p.threat;
        s[KIN_THREAT] = p.kin_threat;
        s[GRUDGE] = p.grudge;
        s[PREY_NEAR] = p.prey_near;
        s[REVERENCE] = p.reverence;
        s[BIAS] = 1.0;
        s
    }

    pub fn remember(&mut self, p: Prim) {
        if self.memory.last() == Some(&p) {
            return;
        }
        self.memory.push(p);
        let len = self.memory_len();
        while self.memory.len() > len {
            self.memory.remove(0);
        }
    }

    // --- relationships ---

    pub fn affinity(&self, id: u64) -> f32 {
        self.relations.iter().find(|r| r.id == id).map_or(0.0, |r| r.affinity)
    }

    pub fn family_feeling(&self, family: u32) -> f32 {
        self.family_feelings.iter().find(|f| f.0 == family).map_or(0.0, |f| f.1)
    }

    pub fn adjust_affinity(&mut self, id: u64, family: u32, delta: f32) {
        match self.relations.iter_mut().find(|r| r.id == id) {
            Some(r) => r.affinity = (r.affinity + delta).clamp(-1.0, 1.0),
            None => {
                self.relations.push(Relation { id, family, affinity: delta.clamp(-1.0, 1.0) });
                if self.relations.len() > MAX_RELATIONS {
                    // forget the weakest feeling
                    self.relations.sort_by(|a, b| b.affinity.abs().total_cmp(&a.affinity.abs()));
                    self.relations.truncate(MAX_RELATIONS);
                }
            }
        }
    }

    pub fn adjust_family_feeling(&mut self, family: u32, delta: f32) {
        if family == self.family {
            return;
        }
        match self.family_feelings.iter_mut().find(|f| f.0 == family) {
            Some(f) => f.1 = (f.1 + delta).clamp(-1.0, 1.0),
            None => self.family_feelings.push((family, delta.clamp(-1.0, 1.0))),
        }
    }

    /// How much this NPC trusts what `other` tells it (0..1).
    pub fn trust(&self, other_id: u64, other_family: u32) -> f32 {
        if other_family == self.family {
            return 1.0;
        }
        let a = self.affinity(other_id) + 0.5 * self.family_feeling(other_family);
        (0.3 + a).clamp(0.0, 1.0)
    }

    /// Grudges fade; vengeful NPCs hold on longer.
    pub fn fade_feelings(&mut self) {
        let fade = 1.0 - 0.01 * (1.0 - self.traits[VENGEFULNESS]) - 0.001;
        for r in &mut self.relations {
            r.affinity *= fade;
        }
        for f in &mut self.family_feelings {
            f.1 *= fade;
        }
        self.relations.retain(|r| r.affinity.abs() > 0.02);
        self.family_feelings.retain(|f| f.1.abs() > 0.02);
    }

    // --- reproduction ---

    fn child(&self, id: u64, genes: GoalGenes, traits: Traits, rng: &mut fastrand::Rng) -> Npc {
        let mind = if self.mind == Mind::Story { Mind::Villager } else { self.mind };
        let mut c = Npc::new(id, self.x + rng.f32() * 16.0 - 8.0, self.y + rng.f32() * 16.0 - 8.0, self.family, mind, rng);
        c.genes = genes;
        c.traits = traits;
        c.age = 0.0;
        c.fatigue = 0.0;
        c.home = self.home;
        c.generation = self.generation + 1;
        c
    }

    /// Have a child alone: costlier, the child is a mutated copy.
    pub fn bud(&mut self, id: u64, rng: &mut fastrand::Rng) -> Npc {
        self.food -= BUD_COST;
        self.water -= BUD_COST;
        self.cooldown = REPRODUCE_COOLDOWN;
        let mut c = self.child(id, mutate_genes(&self.genes, rng), mutate_traits(&self.traits, rng), rng);
        c.food = BUD_COST;
        c.water = BUD_COST;
        c
    }

    /// Have a child with a partner: genes and traits mixed from both. The child
    /// joins this NPC's family and home. Beliefs are inherited in world.rs.
    pub fn mate(&mut self, partner: &mut Npc, id: u64, rng: &mut fastrand::Rng) -> Npc {
        for p in [&mut *self, &mut *partner] {
            p.food -= MATE_COST;
            p.water -= MATE_COST;
            p.cooldown = REPRODUCE_COOLDOWN;
        }
        let mut genes = self.genes;
        for (row, other) in genes.iter_mut().zip(partner.genes.iter()) {
            if rng.bool() {
                *row = *other;
            }
        }
        let mut traits = self.traits;
        for (t, other) in traits.iter_mut().zip(partner.traits.iter()) {
            if rng.bool() {
                *t = *other;
            }
        }
        let mut c = self.child(id, mutate_genes(&genes, rng), mutate_traits(&traits, rng), rng);
        c.food = 2.0 * MATE_COST;
        c.water = 2.0 * MATE_COST;
        c
    }
}

pub fn mutate_genes(g: &GoalGenes, rng: &mut fastrand::Rng) -> GoalGenes {
    let mut out = *g;
    for row in out.iter_mut() {
        for v in row.iter_mut() {
            *v = (*v * (1.0 + (rng.f32() * 2.0 - 1.0) * MUTATION)).max(0.0);
        }
    }
    out
}

pub fn mutate_traits(t: &Traits, rng: &mut fastrand::Rng) -> Traits {
    let mut out = *t;
    for v in out.iter_mut() {
        *v = (*v + (rng.f32() * 2.0 - 1.0) * TRAIT_MUTATION).clamp(0.0, 1.0);
    }
    out
}

/// Matrix * vector, written out the long way so it's easy to follow.
pub fn goal_scores(genes: &GoalGenes, s: &StateVec) -> [f32; N_GOALS] {
    let mut out = [0.0; N_GOALS];
    for g in 0..N_GOALS {
        for i in 0..N_STATES {
            out[g] += genes[g][i] * s[i];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mating_mixes_parents_and_costs_both() {
        let mut rng = fastrand::Rng::with_seed(3);
        let mut a = Npc::new(1, 0.0, 0.0, 0, Mind::Villager, &mut rng);
        let mut b = Npc::new(2, 0.0, 0.0, 5, Mind::Villager, &mut rng);
        a.traits = [0.0; N_TRAITS];
        b.traits = [1.0; N_TRAITS];
        (a.food, a.water, b.food, b.water) = (1.0, 1.0, 1.0, 1.0);
        let c = a.mate(&mut b, 3, &mut rng);
        assert!((a.food - 0.8).abs() < 1e-5 && (b.food - 0.8).abs() < 1e-5);
        assert_eq!(c.family, 0);
        let from_b = c.traits.iter().filter(|&&t| t > 0.5).count();
        assert!(from_b > 2 && from_b < 14, "traits should come from both parents: {from_b}");
    }

    #[test]
    fn grudges_fade_slower_for_the_vengeful() {
        let mut rng = fastrand::Rng::with_seed(4);
        let mut forgiving = Npc::new(1, 0.0, 0.0, 0, Mind::Villager, &mut rng);
        let mut vengeful = Npc::new(2, 0.0, 0.0, 0, Mind::Villager, &mut rng);
        forgiving.traits[VENGEFULNESS] = 0.0;
        vengeful.traits[VENGEFULNESS] = 1.0;
        forgiving.adjust_affinity(9, 3, -1.0);
        vengeful.adjust_affinity(9, 3, -1.0);
        for _ in 0..100 {
            forgiving.fade_feelings();
            vengeful.fade_feelings();
        }
        assert!(vengeful.affinity(9) < forgiving.affinity(9));
    }

    #[test]
    fn trust_depends_on_family_and_feelings() {
        let mut rng = fastrand::Rng::with_seed(5);
        let mut n = Npc::new(1, 0.0, 0.0, 0, Mind::Villager, &mut rng);
        assert_eq!(n.trust(2, 0), 1.0);
        n.adjust_affinity(3, 4, -0.8);
        assert_eq!(n.trust(3, 4), 0.0);
        assert!(n.trust(5, 6) > 0.0);
    }
}
