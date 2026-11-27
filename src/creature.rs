// A creature = position + body + memory + personality matrix.
//
// Each decision tick:
//
//   action_vector = personality * state_vector
//
// The personality matrix has one row per action and one column per state.
// Row i = "how much does each state push me toward action i".
// The creature picks the highest-scoring action it can actually do right now
// (e.g. it can't share food it doesn't have); if the top one is impossible it
// falls back to the next best.

use Action::*;

// ---------------------------------------------------------------------------
// States (inputs). Every state is 0..1.
// ---------------------------------------------------------------------------

pub const N_STATES: usize = 14;

pub const HUNGER: usize = 0; //     1 - food in stomach
pub const THIRST: usize = 1; //     1 - water in stomach
pub const BOREDOM: usize = 2; //    rises over time, rest lowers it
pub const HEALTH: usize = 3; //     0 = dead
pub const AGE: usize = 4; //        age / lifespan
pub const FOOD_FAR: usize = 5; //   distance to nearest food that has food left
pub const CROWDING: usize = 6; //   how many others are close
pub const LONELINESS: usize = 7; // rises when alone, drops near others
pub const KIN_NEAR: usize = 8; //   how many family members are close
pub const FEAR: usize = 9; //       spikes when attacked, fades over time
pub const CARRYING: usize = 10; //  food carried / carry capacity
pub const HOME_FAR: usize = 11; //  distance to home village (1 = no home)
pub const COLD: usize = 12; //      1 at night when not under a shelter
pub const BIAS: usize = 13; //      always 1: lets each action have a default pull

pub const STATE_NAMES: [&str; N_STATES] = [
    "HUNGER", "THIRST", "BOREDOM", "HEALTH", "AGE", "FOOD FAR", "CROWDING",
    "LONELINESS", "KIN NEAR", "FEAR", "CARRYING", "HOME FAR", "COLD", "BIAS (ALWAYS 1)",
];
pub const STATE_SHORT: [&str; N_STATES] = [
    "HU", "TH", "BO", "HE", "AG", "FD", "CR", "LO", "KI", "FE", "CA", "HO", "CO", "1",
];

// ---------------------------------------------------------------------------
// Actions (outputs).
// ---------------------------------------------------------------------------

pub const N_ACTIONS: usize = 13;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Eat,       // eat carried food, or walk to food / the village stockpile
    Drink,     // walk to water
    Rest,      // lower boredom (faster under a shelter)
    Reproduce, // make a child
    Explore,   // wander somewhere new
    Follow,    // move toward family members
    Share,     // give food to the hungriest neighbor
    Steal,     // take food from a nearby non-family creature (both get hurt)
    Flee,      // run away from whoever attacked us
    Gather,    // pick up food to carry instead of eating it
    Store,     // drop carried food in the home stockpile
    Build,     // spend carried food building the home shelter
    Farm,      // spend carried food planting a new food source near home
}

pub const ACTIONS: [Action; N_ACTIONS] = [
    Eat, Drink, Rest, Reproduce, Explore, Follow, Share, Steal, Flee, Gather, Store, Build, Farm,
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
            Gather => "GATHER",
            Store => "STORE",
            Build => "BUILD",
            Farm => "FARM",
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
            Gather => "GATH",
            Store => "STOR",
            Build => "BULD",
            Farm => "FARM",
        }
    }
}

pub type StateVec = [f32; N_STATES];
pub type ActionVec = [f32; N_ACTIONS];
pub type Personality = [[f32; N_STATES]; N_ACTIONS];

// ---------------------------------------------------------------------------
// Tuning
// ---------------------------------------------------------------------------

/// Children get every personality entry scaled by a random factor in [1 - this, 1 + this].
pub const MUTATION: f32 = 0.05;

/// Reproducing costs this much food, water and boredom (30% of full).
pub const REPRODUCE_COST: f32 = 0.30;

/// Ticks before a creature can reproduce.
pub const MATURITY: f32 = 20.0;

/// Ticks a creature must wait between children.
pub const REPRODUCE_COOLDOWN: f32 = 15.0;

/// Lifespan in ticks is picked between these.
pub const MIN_LIFESPAN: f32 = 200.0;
pub const MAX_LIFESPAN: f32 = 300.0;

/// Max food a creature can carry (1.0 = one full stomach).
pub const CARRY_CAP: f32 = 1.0;

/// Random personalities: every entry is uniform in 0..RANDOM_MAX, and each
/// "instinct" (action, state) pair below gets its strength added on top.
/// This is the non-square version of the old diagonal bias.
pub const RANDOM_MAX: f32 = 0.3;
pub const INSTINCTS: [(Action, usize, f32); N_ACTIONS] = [
    (Eat, HUNGER, 1.0),
    (Drink, THIRST, 1.0),
    (Rest, BOREDOM, 1.0),
    (Reproduce, HEALTH, 0.6), // health is usually ~1, so keep this modest
    (Explore, FOOD_FAR, 1.0),
    (Follow, LONELINESS, 1.0),
    (Share, KIN_NEAR, 1.0),
    (Steal, CROWDING, 0.5),
    (Flee, FEAR, 1.0),
    // BIAS is always 1, so this is a constant pull: "work when nothing else
    // is urgent". Kept weak, or creatures hoard food while starving.
    (Gather, BIAS, 0.5),
    (Store, CARRYING, 1.0),
    (Build, COLD, 1.0),
    (Farm, FOOD_FAR, 1.0),
];

/// What the world tells a creature about its surroundings each tick.
#[derive(Clone, Copy, Default)]
pub struct Perception {
    pub food_far: f32,
    pub crowding: f32,
    pub kin_near: f32,
    pub home_far: f32,
    pub cold: f32,
}

pub struct Creature {
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

    // social / settlement
    pub family: u32,
    pub carry: f32,
    pub home: Option<usize>, // index into World::settlements
    pub loneliness: f32,
    pub fear: f32,
    pub threat: Option<(f32, f32)>, // where the last attacker was

    // decision
    pub personality: Personality,
    pub action: Action,
    pub target: Option<(f32, f32)>, // for explore / follow / flee
    pub perception: Perception,
    pub last_scores: ActionVec,
}

impl Creature {
    pub fn new(x: f32, y: f32, family: u32, personality: Personality) -> Self {
        Self {
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
            carry: 0.0,
            home: None,
            loneliness: 0.0,
            fear: 0.0,
            threat: None,
            personality,
            action: Rest,
            target: None,
            perception: Perception { food_far: 0.5, home_far: 1.0, ..Default::default() },
            last_scores: [0.0; N_ACTIONS],
        }
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
        s[CARRYING] = self.carry / CARRY_CAP;
        s[HOME_FAR] = p.home_far;
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
    /// done right now; the best possible action wins.
    pub fn decide(&mut self, possible: &[bool; N_ACTIONS]) {
        let scores = mat_vec(&self.personality, &self.state());

        let total: f32 = scores.iter().sum();
        self.last_scores = if total > 0.0 {
            scores.map(|s| s / total)
        } else {
            [1.0 / N_ACTIONS as f32; N_ACTIONS]
        };

        let mut best: Option<usize> = None;
        for i in 0..N_ACTIONS {
            if possible[i] && best.is_none_or(|b| self.last_scores[i] > self.last_scores[b]) {
                best = Some(i);
            }
        }
        self.action = ACTIONS[best.unwrap_or(Rest.index())];
    }

    /// Spend REPRODUCE_COST of food, water and boredom to make one child next
    /// to us. The child starts with the food and water that was spent and
    /// belongs to the same family and village.
    pub fn reproduce(&mut self) -> Creature {
        self.food -= REPRODUCE_COST;
        self.water -= REPRODUCE_COST;
        self.boredom = (self.boredom - REPRODUCE_COST).max(0.0);
        self.cooldown = REPRODUCE_COOLDOWN;

        let mut child = Creature::new(
            self.x + fastrand::f32() * 16.0 - 8.0,
            self.y + fastrand::f32() * 16.0 - 8.0,
            self.family,
            mutate(&self.personality),
        );
        child.food = REPRODUCE_COST;
        child.water = REPRODUCE_COST;
        child.boredom = 0.0;
        child.age = 0.0;
        child.home = self.home;
        child
    }

    /// Color from personality, using three groups of action rows:
    ///   red   = wild    (explore + steal + flee)
    ///   green = social  (follow + share)
    ///   blue  = settler (gather + store + build + farm)
    ///   share_i = groupsum_i / (wild + social + settler)
    ///   channel_i = 255 * clamp(3 * share_i - 0.5, 0, 1)
    pub fn color(&self) -> u32 {
        let row = |a: Action| -> f32 { self.personality[a.index()].iter().sum() };
        let wild = row(Explore) + row(Steal) + row(Flee);
        let social = row(Follow) + row(Share);
        let settler = row(Gather) + row(Store) + row(Build) + row(Farm);
        // groups have different row counts, so compare per-row averages
        let sums = [wild / 3.0, social / 2.0, settler / 4.0];
        let total: f32 = sums.iter().sum::<f32>().max(1e-6);
        let ch = |i: usize| ((3.0 * sums[i] / total - 0.5).clamp(0.0, 1.0) * 255.0) as u32;
        let (r, g, b) = (ch(0), ch(1), ch(2));
        (r << 16) | (g << 8) | b
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

    /// Personality where only the instinct pairs matter.
    fn instincts_only() -> Personality {
        let mut p = [[0.0; N_STATES]; N_ACTIONS];
        for (action, state, strength) in INSTINCTS {
            p[action.index()][state] = strength;
        }
        p
    }

    fn calm(c: &mut Creature) {
        c.food = 1.0;
        c.water = 1.0;
        c.boredom = 0.0;
        c.health = 0.0;
        c.perception = Perception::default();
    }

    #[test]
    fn mat_vec_matches_hand_calculation() {
        let mut m = [[0.0; N_STATES]; N_ACTIONS];
        m[0][0] = 2.0;
        m[0][1] = 3.0;
        m[1][13] = 0.5;
        let mut v = [0.0; N_STATES];
        v[0] = 1.0;
        v[1] = 2.0;
        v[13] = 1.0;
        let out = mat_vec(&m, &v);
        assert_eq!(out[0], 8.0); // 2*1 + 3*2
        assert_eq!(out[1], 0.5); // 0.5*1
    }

    #[test]
    fn hungry_creature_eats() {
        let mut c = Creature::new(0.0, 0.0, 0, instincts_only());
        calm(&mut c);
        c.food = 0.0;
        c.decide(&[true; N_ACTIONS]);
        assert_eq!(c.action, Eat);
    }

    #[test]
    fn falls_back_when_best_action_is_impossible() {
        let mut c = Creature::new(0.0, 0.0, 0, instincts_only());
        calm(&mut c);
        c.food = 0.0; // hunger 1 -> Eat
        c.water = 0.5; // thirst 0.5 -> Drink second
        let mut possible = [true; N_ACTIONS];
        possible[Eat.index()] = false;
        c.decide(&possible);
        assert_eq!(c.action, Drink);
    }

    #[test]
    fn reproducing_costs_thirty_percent_and_keeps_family() {
        let mut parent = Creature::new(0.0, 0.0, 7, instincts_only());
        parent.food = 1.0;
        parent.water = 0.8;
        parent.boredom = 0.5;
        parent.home = Some(3);
        let child = parent.reproduce();
        assert!((parent.food - 0.7).abs() < 1e-5);
        assert!((parent.water - 0.5).abs() < 1e-5);
        assert!((parent.boredom - 0.2).abs() < 1e-5);
        assert_eq!(child.food, REPRODUCE_COST);
        assert_eq!(child.family, 7);
        assert_eq!(child.home, Some(3));
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
        let mut c = Creature::new(0.0, 0.0, 0, instincts_only());
        assert!(!c.is_dead());
        c.age = c.lifespan;
        assert!(c.is_dead());
        c.age = 0.0;
        c.health = 0.0;
        assert!(c.is_dead());
    }

    #[test]
    fn instinct_bias_lands_on_the_right_cells() {
        let p = random_personality();
        assert!(p[Eat.index()][HUNGER] >= 1.0);
        assert!(p[Build.index()][COLD] >= 1.0);
        assert!(p[Eat.index()][THIRST] < RANDOM_MAX);
    }
}
