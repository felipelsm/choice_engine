// A creature = position + body + memory + personality matrix + caution gene.
//
// Each decision tick:
//
//   scores = personality * state_vector
//
// The personality matrix has one row per action and one column per state.
// Row i = "how much does each state push me toward action i".
//
// Choosing among the scores (see `decide`):
// - only actions that are possible right now count (no sharing food you don't
//   have); impossible ones fall through to the next best
// - when the top two scores are close, the creature has a dilemma. How it
//   handles that depends on its CAUTION gene:
//     cautious -> hesitates (loses time standing still) but picks the best
//     rash     -> acts at once but sometimes picks the runner-up by mistake
//   Evolution decides which pays off.

use crate::culture::{Memory, MEMORY_LEN};
use Action::*;

// ---------------------------------------------------------------------------
// States (inputs). Every state is 0..1.
// ---------------------------------------------------------------------------

pub const N_STATES: usize = 15;

pub const HUNGER: usize = 0; //      1 - food in stomach
pub const THIRST: usize = 1; //      1 - water in stomach
pub const BOREDOM: usize = 2; //     rises over time, rest lowers it
pub const HEALTH: usize = 3; //      0 = dead
pub const AGE: usize = 4; //         age / lifespan
pub const FOOD_FAR: usize = 5; //    distance to nearest food (bush or pile)
pub const CROWDING: usize = 6; //    how many others are close
pub const LONELINESS: usize = 7; //  rises when alone, drops near others
pub const KIN_NEAR: usize = 8; //    how many family members are close
pub const FEAR: usize = 9; //        spikes when robbed, fades over time
pub const CARRY_FOOD: usize = 10; // food in hands / capacity
pub const CARRY_WOOD: usize = 11; // wood in hands / capacity
pub const NEAR_HOME: usize = 12; //  1 at home, 0 far away
pub const COLD: usize = 13; //       1 at night when not near a shelter
pub const BIAS: usize = 14; //       always 1: lets each action have a default pull

pub const STATE_NAMES: [&str; N_STATES] = [
    "HUNGER", "THIRST", "BOREDOM", "HEALTH", "AGE", "FOOD FAR", "CROWDING", "LONELINESS",
    "KIN NEAR", "FEAR", "CARRY FOOD", "CARRY WOOD", "NEAR HOME", "COLD", "BIAS (ALWAYS 1)",
];
pub const STATE_SHORT: [&str; N_STATES] = [
    "HU", "TH", "BO", "HE", "AG", "FD", "CR", "LO", "KI", "FE", "CF", "CW", "NH", "CO", "1",
];

// ---------------------------------------------------------------------------
// Actions (outputs). The last four are primitives: there is no "farm" or
// "build" action. Farming and shelters can only emerge from the physics of
// dropping things (see world.rs).
// ---------------------------------------------------------------------------

pub const N_ACTIONS: usize = 13;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Eat,       // eat carried food, or walk to the nearest bush / food pile
    Drink,     // walk to water
    Rest,      // lower boredom (faster near a shelter)
    Reproduce, // make a child
    Explore,   // wander somewhere new
    Follow,    // move toward family members
    Share,     // give carried food to the hungriest neighbor
    Steal,     // take food from a nearby non-family creature (both get hurt)
    Flee,      // run away from whoever robbed us
    PickFood,  // walk to food and pick it up instead of eating it
    PickWood,  // walk to a tree and pick up wood
    Drop,      // drop whatever we're carrying right here
    GoHome,    // walk home
}

pub const ACTIONS: [Action; N_ACTIONS] = [
    Eat, Drink, Rest, Reproduce, Explore, Follow, Share, Steal, Flee, PickFood, PickWood, Drop, GoHome,
];

impl Action {
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Eat => "EAT",
            Drink => "DRINK",
            Rest => "REST",
            Reproduce => "REPRODUCE",
            Explore => "EXPLORE",
            Follow => "FOLLOW",
            Share => "SHARE",
            Steal => "STEAL",
            Flee => "FLEE",
            PickFood => "PICK UP FOOD",
            PickWood => "PICK UP WOOD",
            Drop => "DROP",
            GoHome => "GO HOME",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Eat => "EAT",
            Drink => "DRNK",
            Rest => "REST",
            Reproduce => "REPR",
            Explore => "EXPL",
            Follow => "FOLW",
            Share => "SHAR",
            Steal => "STEL",
            Flee => "FLEE",
            PickFood => "PKFD",
            PickWood => "PKWD",
            Drop => "DROP",
            GoHome => "HOME",
        }
    }
}

pub type StateVec = [f32; N_STATES];
pub type ActionVec = [f32; N_ACTIONS];
pub type Personality = [[f32; N_STATES]; N_ACTIONS];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    Food,
    Wood,
}

// ---------------------------------------------------------------------------
// Tuning
// ---------------------------------------------------------------------------

/// Children get every personality entry scaled by a random factor in [1 - this, 1 + this].
pub const MUTATION: f32 = 0.05;
/// Children's caution moves by up to this much (added, not scaled).
pub const CAUTION_MUTATION: f32 = 0.05;

/// Reproducing costs this much food, water and boredom (30% of full).
pub const REPRODUCE_COST: f32 = 0.30;
/// Ticks before a creature can reproduce, and between children.
pub const MATURITY: f32 = 20.0;
pub const REPRODUCE_COOLDOWN: f32 = 15.0;

/// Lifespan in ticks is picked between these.
pub const MIN_LIFESPAN: f32 = 200.0;
pub const MAX_LIFESPAN: f32 = 300.0;

/// Max amount a creature can carry (1.0 = one full stomach of food).
pub const CARRY_CAP: f32 = 1.0;

/// Hesitation: a fully cautious creature facing a perfect tie stands still
/// this many seconds. A fully rash creature facing a perfect tie picks the
/// runner-up with this probability.
pub const MAX_HESITATION: f32 = 1.0;
pub const MAX_MISTAKE: f32 = 0.5;

/// Play / exploration (epsilon-greedy): this fraction of decisions picks a
/// random possible action instead of the best one. Without it nobody would
/// ever try dropping things, so nothing could be discovered.
pub const PLAY_CHANCE: f32 = 0.05;

/// Random personalities: every entry is uniform in 0..RANDOM_MAX, and each
/// "instinct" (action, state) pair below gets its strength added on top.
/// The primitives (pick up / drop / go home) only get weak "curiosity":
/// creatures sometimes pick things up and put them down again, with no idea
/// why that might be useful. Nothing tells them to bring things home; that
/// has to be discovered (see culture.rs) or evolved.
pub const RANDOM_MAX: f32 = 0.3;
pub const INSTINCTS: [(Action, usize, f32); 13] = [
    (Eat, HUNGER, 1.0),
    (Drink, THIRST, 1.0),
    (Rest, BOREDOM, 1.0),
    (Reproduce, HEALTH, 0.6), // health is usually ~1, so keep this modest
    (Explore, FOOD_FAR, 1.0),
    (Follow, LONELINESS, 1.0),
    (Share, KIN_NEAR, 1.0),
    (Steal, CROWDING, 0.5),
    (Flee, FEAR, 1.0),
    // curiosity
    (PickFood, BIAS, 0.25),
    (PickWood, BIAS, 0.2),
    (Drop, CARRY_FOOD, 0.2),
    (Drop, CARRY_WOOD, 0.3),
];

/// What the world tells a creature about its surroundings each tick.
#[derive(Clone, Copy, Default)]
pub struct Perception {
    pub food_far: f32,
    pub crowding: f32,
    pub kin_near: f32,
    pub near_home: f32,
    pub cold: f32,
}

pub struct Creature {
    pub id: u64,
    pub x: f32,
    pub y: f32,

    // body
    pub food: f32,    // 0 = empty, 1 = full
    pub water: f32,   // 0 = empty, 1 = full
    pub boredom: f32, // 0 = fine, 1 = exhausted
    pub health: f32,  // 0 = dead, 1 = healthy
    pub age: f32,     // in ticks
    pub lifespan: f32,
    pub cooldown: f32, // ticks until it can reproduce again

    // social / home
    pub family: u32,
    pub generation: u32,
    pub carry: Option<(Item, f32)>,
    pub home: (f32, f32),
    pub loneliness: f32,
    pub fear: f32,
    pub threat: Option<(f32, f32)>, // where the last attacker was

    // decision
    pub personality: Personality,
    pub caution: f32,    // gene, 0 = rash, 1 = cautious
    pub hesitation: f32, // seconds left standing still
    pub mistake: bool,   // picked the runner-up this tick
    pub playing: bool,   // tried a random action this tick
    pub action: Action,
    pub target: Option<(f32, f32)>, // for explore / follow / flee
    pub perception: Perception,
    pub last_scores: ActionVec, // raw scores from the last decision
    pub memory: Memory,         // last MEMORY_LEN (state, action) decisions, newest last
}

impl Creature {
    pub fn new(id: u64, x: f32, y: f32, family: u32, personality: Personality, caution: f32) -> Self {
        Self {
            id,
            x,
            y,
            food: 0.5 + fastrand::f32() * 0.5,
            water: 0.5 + fastrand::f32() * 0.5,
            boredom: fastrand::f32() * 0.5,
            health: 1.0,
            age: MATURITY + fastrand::f32() * 30.0, // founders start as young adults
            lifespan: MIN_LIFESPAN + fastrand::f32() * (MAX_LIFESPAN - MIN_LIFESPAN),
            cooldown: 0.0,
            family,
            generation: 0,
            carry: None,
            home: (x, y),
            loneliness: 0.0,
            fear: 0.0,
            threat: None,
            personality,
            caution,
            hesitation: 0.0,
            mistake: false,
            playing: false,
            action: Rest,
            target: None,
            perception: Perception { food_far: 0.5, near_home: 1.0, ..Default::default() },
            last_scores: [0.0; N_ACTIONS],
            memory: Vec::with_capacity(MEMORY_LEN),
        }
    }

    pub fn carrying(&self, item: Item) -> f32 {
        match self.carry {
            Some((i, amount)) if i == item => amount,
            _ => 0.0,
        }
    }

    pub fn carried(&self) -> f32 {
        self.carry.map_or(0.0, |(_, a)| a)
    }

    pub fn state(&self) -> StateVec {
        let p = &self.perception;
        let mut s = [0.0; N_STATES];
        s[HUNGER] = 1.0 - self.food;
        s[THIRST] = 1.0 - self.water;
        s[BOREDOM] = self.boredom;
        s[HEALTH] = self.health;
        s[AGE] = (self.age / self.lifespan).min(1.0);
        s[FOOD_FAR] = p.food_far;
        s[CROWDING] = p.crowding;
        s[LONELINESS] = self.loneliness;
        s[KIN_NEAR] = p.kin_near;
        s[FEAR] = self.fear;
        s[CARRY_FOOD] = self.carrying(Item::Food) / CARRY_CAP;
        s[CARRY_WOOD] = self.carrying(Item::Wood) / CARRY_CAP;
        s[NEAR_HOME] = p.near_home;
        s[COLD] = p.cold;
        s[BIAS] = 1.0;
        s
    }

    pub fn is_adult(&self) -> bool {
        self.age >= MATURITY
    }

    pub fn is_dead(&self) -> bool {
        self.health <= 0.0 || self.age >= self.lifespan
    }

    pub fn can_reproduce(&self) -> bool {
        self.is_adult()
            && self.cooldown <= 0.0
            && self.food >= REPRODUCE_COST
            && self.water >= REPRODUCE_COST
    }

    /// Runs once per decision tick. `possible[i]` says whether action i can be
    /// done right now. `culture` is the family's learned bonus, added on top of
    /// the genes. Uses `roll` (0..1) for the rash-mistake chance so tests can
    /// control it.
    #[cfg(test)]
    pub fn decide_with(&mut self, possible: &[bool; N_ACTIONS], culture: &Personality, roll: f32) {
        self.decide_full(possible, culture, roll, 1.0);
    }

    /// Like `decide_with`, also controlling the play roll.
    pub fn decide_full(&mut self, possible: &[bool; N_ACTIONS], culture: &Personality, roll: f32, play_roll: f32) {
        let state = self.state();
        let genes = mat_vec(&self.personality, &state);
        let learned = mat_vec(culture, &state);
        let mut scores = [0.0; N_ACTIONS];
        for i in 0..N_ACTIONS {
            scores[i] = genes[i] + learned[i];
        }
        self.last_scores = scores;

        // best and runner-up among the possible actions
        let (mut best, mut second): (Option<usize>, Option<usize>) = (None, None);
        for i in 0..N_ACTIONS {
            if !possible[i] {
                continue;
            }
            if best.is_none_or(|b| scores[i] > scores[b]) {
                second = best;
                best = Some(i);
            } else if second.is_none_or(|s| scores[i] > scores[s]) {
                second = Some(i);
            }
        }
        let Some(best) = best else {
            self.action = Rest;
            return;
        };

        // closeness: 1 = perfect tie, 0 = clear winner
        let closeness = match second {
            Some(s) if scores[best] > 0.0 => (scores[s] / scores[best]).clamp(0.0, 1.0),
            _ => 0.0,
        };

        // cautious: stand still a bit, then do the right thing
        self.hesitation = MAX_HESITATION * self.caution * closeness;
        // rash: no delay, but may grab the runner-up
        let mistake_chance = MAX_MISTAKE * (1.0 - self.caution) * closeness;
        self.mistake = second.is_some() && roll < mistake_chance;

        let chosen = if self.mistake { second.unwrap() } else { best };
        self.action = ACTIONS[chosen];

        // play: once in a while, just try something
        self.playing = play_roll < PLAY_CHANCE;
        if self.playing {
            let options: Vec<usize> = (0..N_ACTIONS).filter(|&i| possible[i]).collect();
            self.action = ACTIONS[options[fastrand::usize(..options.len())]];
            self.hesitation = 0.0;
        }

        if self.memory.len() == MEMORY_LEN {
            self.memory.remove(0);
        }
        self.memory.push((state, self.action));
    }

    pub fn decide(&mut self, possible: &[bool; N_ACTIONS], culture: &Personality) {
        self.decide_full(possible, culture, fastrand::f32(), fastrand::f32());
    }

    /// Spend REPRODUCE_COST of food, water and boredom to make one child next
    /// to us. The child starts with the food and water that was spent, and
    /// shares our family and home.
    pub fn reproduce(&mut self, child_id: u64) -> Creature {
        self.food -= REPRODUCE_COST;
        self.water -= REPRODUCE_COST;
        self.boredom = (self.boredom - REPRODUCE_COST).max(0.0);
        self.cooldown = REPRODUCE_COOLDOWN;

        let caution = (self.caution + (fastrand::f32() * 2.0 - 1.0) * CAUTION_MUTATION).clamp(0.0, 1.0);
        let mut child = Creature::new(
            child_id,
            self.x + fastrand::f32() * 16.0 - 8.0,
            self.y + fastrand::f32() * 16.0 - 8.0,
            self.family,
            mutate(&self.personality),
            caution,
        );
        child.food = REPRODUCE_COST;
        child.water = REPRODUCE_COST;
        child.boredom = 0.0;
        child.age = 0.0;
        child.home = self.home;
        child.generation = self.generation + 1;
        child
    }

    /// Color from personality, using three groups of action rows:
    ///   red   = wild    (explore + steal + flee)
    ///   green = social  (follow + share)
    ///   blue  = builder (pick up food + pick up wood + drop + go home)
    ///   share_i = group average_i / (wild + social + builder)
    ///   channel_i = 255 * clamp(3 * share_i - 0.5, 0, 1)
    pub fn color(&self) -> u32 {
        let row = |a: Action| -> f32 { self.personality[a.index()].iter().sum() };
        let wild = (row(Explore) + row(Steal) + row(Flee)) / 3.0;
        let social = (row(Follow) + row(Share)) / 2.0;
        let builder = (row(PickFood) + row(PickWood) + row(Drop) + row(GoHome)) / 4.0;
        let sums = [wild, social, builder];
        let total: f32 = sums.iter().sum::<f32>().max(1e-6);
        let ch = |i: usize| ((3.0 * sums[i] / total - 0.5).clamp(0.0, 1.0) * 255.0) as u32;
        (ch(0) << 16) | (ch(1) << 8) | ch(2)
    }
}

/// Matrix * vector, written out the long way so it's easy to follow.
pub fn mat_vec(m: &Personality, v: &StateVec) -> ActionVec {
    let mut out = [0.0; N_ACTIONS];
    for row in 0..N_ACTIONS {
        for col in 0..N_STATES {
            out[row] += m[row][col] * v[col];
        }
    }
    out
}

/// Random personality with instinct bias (see INSTINCTS).
pub fn random_personality() -> Personality {
    let mut p = [[0.0; N_STATES]; N_ACTIONS];
    for row in p.iter_mut() {
        for v in row.iter_mut() {
            *v = fastrand::f32() * RANDOM_MAX;
        }
    }
    for (action, state, strength) in INSTINCTS {
        p[action.index()][state] += strength;
    }
    p
}

/// Copy of `p` with every entry nudged by up to +-MUTATION (relative).
pub fn mutate(p: &Personality) -> Personality {
    let mut out = *p;
    for row in out.iter_mut() {
        for v in row.iter_mut() {
            let factor = 1.0 + (fastrand::f32() * 2.0 - 1.0) * MUTATION;
            *v = (*v * factor).max(0.0);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Personality = [[0.0; N_STATES]; N_ACTIONS];

    /// Personality where only the main instinct pairs matter (no curiosity).
    fn instincts_only() -> Personality {
        let mut p = [[0.0; N_STATES]; N_ACTIONS];
        for (action, state, strength) in INSTINCTS.iter().take(9) {
            p[action.index()][*state] = *strength;
        }
        p
    }

    fn calm(caution: f32) -> Creature {
        let mut c = Creature::new(0, 0.0, 0.0, 0, instincts_only(), caution);
        c.food = 1.0;
        c.water = 1.0;
        c.boredom = 0.0;
        c.health = 0.0;
        c.perception = Perception::default();
        c
    }

    #[test]
    fn mat_vec_matches_hand_calculation() {
        let mut m = [[0.0; N_STATES]; N_ACTIONS];
        m[0][0] = 2.0;
        m[0][1] = 3.0;
        m[1][BIAS] = 0.5;
        let mut v = [0.0; N_STATES];
        v[0] = 1.0;
        v[1] = 2.0;
        v[BIAS] = 1.0;
        let out = mat_vec(&m, &v);
        assert_eq!(out[0], 8.0); // 2*1 + 3*2
        assert_eq!(out[1], 0.5);
    }

    #[test]
    fn hungry_creature_eats() {
        let mut c = calm(1.0);
        c.food = 0.0;
        c.decide_with(&[true; N_ACTIONS], &NONE, 1.0);
        assert_eq!(c.action, Eat);
    }

    #[test]
    fn falls_back_when_best_action_is_impossible() {
        let mut c = calm(1.0);
        c.food = 0.0; // hunger 1 -> Eat
        c.water = 0.5; // thirst 0.5 -> Drink second
        let mut possible = [true; N_ACTIONS];
        possible[Eat.index()] = false;
        c.decide_with(&possible, &NONE, 1.0);
        assert_eq!(c.action, Drink);
    }

    #[test]
    fn cautious_creature_hesitates_on_a_close_call() {
        let mut c = calm(1.0);
        c.food = 0.0; // hunger 1.0
        c.water = 0.1; // thirst 0.9: close second
        c.decide_with(&[true; N_ACTIONS], &NONE, 0.0);
        assert_eq!(c.action, Eat); // never a mistake when fully cautious
        assert!((c.hesitation - 0.9).abs() < 1e-5); // 1.0 * 1.0 * 0.9
    }

    #[test]
    fn rash_creature_acts_fast_but_can_pick_the_runner_up() {
        let mut c = calm(0.0);
        c.food = 0.0;
        c.water = 0.1;
        c.decide_with(&[true; N_ACTIONS], &NONE, 0.0); // roll 0 < 0.45 chance
        assert_eq!(c.hesitation, 0.0);
        assert_eq!(c.action, Drink);
        c.decide_with(&[true; N_ACTIONS], &NONE, 0.99); // roll above chance
        assert_eq!(c.action, Eat);
    }

    #[test]
    fn clear_winner_means_no_hesitation_and_no_mistake() {
        let mut c = calm(0.5);
        c.food = 0.0; // hunger 1, everything else 0
        c.decide_with(&[true; N_ACTIONS], &NONE, 0.0);
        assert_eq!(c.action, Eat);
        assert_eq!(c.hesitation, 0.0);
    }

    #[test]
    fn reproducing_costs_thirty_percent_and_passes_things_on() {
        let mut parent = Creature::new(1, 0.0, 0.0, 7, instincts_only(), 0.5);
        parent.food = 1.0;
        parent.water = 0.8;
        parent.boredom = 0.5;
        parent.home = (50.0, 60.0);
        parent.generation = 3;
        let child = parent.reproduce(2);
        assert!((parent.food - 0.7).abs() < 1e-5);
        assert!((parent.water - 0.5).abs() < 1e-5);
        assert!((parent.boredom - 0.2).abs() < 1e-5);
        assert_eq!(child.food, REPRODUCE_COST);
        assert_eq!(child.family, 7);
        assert_eq!(child.home, (50.0, 60.0));
        assert_eq!(child.generation, 4);
        assert!((child.caution - 0.5).abs() <= CAUTION_MUTATION + 1e-6);
        assert!(!child.is_adult());
    }

    #[test]
    fn mutation_stays_within_five_percent() {
        let p = [[1.0; N_STATES]; N_ACTIONS];
        for _ in 0..200 {
            for row in mutate(&p) {
                for v in row {
                    assert!((0.95..=1.05).contains(&v), "{v}");
                }
            }
        }
    }

    #[test]
    fn dies_from_zero_health_or_old_age() {
        let mut c = calm(0.5);
        c.health = 1.0;
        assert!(!c.is_dead());
        c.age = c.lifespan;
        assert!(c.is_dead());
        c.age = 0.0;
        c.health = 0.0;
        assert!(c.is_dead());
    }

    #[test]
    fn nothing_tells_them_to_bring_things_home() {
        let p = random_personality();
        assert!(p[GoHome.index()].iter().all(|&v| v < RANDOM_MAX));
        assert!(p[Drop.index()][NEAR_HOME] < RANDOM_MAX);
        assert!(p[Eat.index()][HUNGER] >= 1.0);
    }

    #[test]
    fn culture_bonus_changes_the_choice() {
        let mut c = calm(1.0);
        c.carry = Some((Item::Food, 1.0));
        let mut culture = NONE;
        culture[GoHome.index()][CARRY_FOOD] = 5.0;
        c.decide_with(&[true; N_ACTIONS], &culture, 1.0);
        assert_eq!(c.action, GoHome);
    }
}
