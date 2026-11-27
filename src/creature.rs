// A creature = position + food/water/boredom/health + personality matrix.
//
// Each decision tick:
//
//   action_vector = personality * state_vector
//
//   [ walk_to_food  ]   [ p00 p01 p02 p03 ]   [ hunger  ]
//   [ walk_to_water ] = [ p10 p11 p12 p13 ] * [ thirst  ]
//   [ rest          ]   [ p20 p21 p22 p23 ]   [ boredom ]
//   [ reproduce     ]   [ p30 p31 p32 p33 ]   [ health  ]
//
// where hunger = 1 - food and thirst = 1 - water.
// Row i of the matrix = "how much does each state push me toward action i".

pub const N_STATES: usize = 4;
pub const N_ACTIONS: usize = 4;

pub type StateVec = [f32; N_STATES];
pub type ActionVec = [f32; N_ACTIONS];
pub type Personality = [[f32; N_STATES]; N_ACTIONS];

/// Children get every personality entry scaled by a random factor in [1 - this, 1 + this].
pub const MUTATION: f32 = 0.05;

/// Reproducing costs this much food, water and boredom (30% of full).
pub const REPRODUCE_COST: f32 = 0.30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    WalkToFood,
    WalkToWater,
    Rest,
    Reproduce,
}

impl Action {
    pub fn from_index(i: usize) -> Action {
        match i {
            0 => Action::WalkToFood,
            1 => Action::WalkToWater,
            2 => Action::Rest,
            _ => Action::Reproduce,
        }
    }
}

pub struct Creature {
    pub x: f32,
    pub y: f32,
    pub food: f32,    // 0 = empty, 1 = full
    pub water: f32,   // 0 = empty, 1 = full
    pub boredom: f32, // 0 = fine, 1 = very bored
    pub health: f32,  // 0 = dead, 1 = healthy
    pub personality: Personality,
    pub action: Action,
    pub last_scores: ActionVec, // likelihoods from the last decision (sum to 1)
}

impl Creature {
    pub fn new(x: f32, y: f32, personality: Personality) -> Self {
        Self {
            x,
            y,
            food: 0.5 + fastrand::f32() * 0.5,
            water: 0.5 + fastrand::f32() * 0.5,
            boredom: fastrand::f32() * 0.5,
            health: 1.0,
            personality,
            action: Action::Rest,
            last_scores: [0.0; N_ACTIONS],
        }
    }

    pub fn state(&self) -> StateVec {
        [1.0 - self.food, 1.0 - self.water, self.boredom, self.health]
    }

    pub fn is_dead(&self) -> bool {
        self.health <= 0.0
    }

    pub fn can_reproduce(&self) -> bool {
        self.food >= REPRODUCE_COST && self.water >= REPRODUCE_COST
    }

    /// Runs once per decision tick.
    pub fn decide(&mut self) {
        let scores = mat_vec(&self.personality, &self.state());

        // Turn raw scores into likelihoods (normalize so they sum to 1).
        let total: f32 = scores.iter().sum();
        self.last_scores = if total > 0.0 {
            scores.map(|s| s / total)
        } else {
            [1.0 / N_ACTIONS as f32; N_ACTIONS]
        };

        self.action = Action::from_index(argmax(&self.last_scores));
    }

    /// Spend REPRODUCE_COST of food, water and boredom to make one child next
    /// to us. The child starts with the food and water that was spent.
    pub fn reproduce(&mut self) -> Creature {
        self.food -= REPRODUCE_COST;
        self.water -= REPRODUCE_COST;
        self.boredom = (self.boredom - REPRODUCE_COST).max(0.0);

        let mut child = Creature::new(
            self.x + fastrand::f32() * 16.0 - 8.0,
            self.y + fastrand::f32() * 16.0 - 8.0,
            mutate(&self.personality),
        );
        child.food = REPRODUCE_COST;
        child.water = REPRODUCE_COST;
        child.boredom = 0.0;
        child
    }

    /// Color from personality.
    ///
    /// Row sum = how strongly the creature is driven toward that action.
    /// Using only the food, water and rest rows:
    ///   share_i = rowsum_i / (rowsum_food + rowsum_water + rowsum_rest)
    ///   channel_i = 255 * clamp(3 * share_i - 0.5, 0, 1)
    /// An even split gives 127 in every channel (grey). Mostly food = green,
    /// mostly water = blue, mostly rest = red.
    pub fn color(&self) -> u32 {
        let sums: Vec<f32> = self.personality[..3].iter().map(|row| row.iter().sum()).collect();
        let total: f32 = sums.iter().sum::<f32>().max(1e-6);
        let ch = |i: usize| ((3.0 * sums[i] / total - 0.5).clamp(0.0, 1.0) * 255.0) as u32;
        let (g, b, r) = (ch(0), ch(1), ch(2));
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

fn argmax(v: &ActionVec) -> usize {
    let mut best = 0;
    for i in 1..v.len() {
        if v[i] > v[best] {
            best = i;
        }
    }
    best
}

/// Extra weight added to the main diagonal of new random personalities
/// (hunger -> food, thirst -> water, boredom -> rest, health -> reproduce).
/// 0.0 = fully random. Bigger = more "sensible" starting creatures.
pub const DIAGONAL_BIAS: f32 = 0.5;

/// Random personality: every entry uniform in 0..1, plus DIAGONAL_BIAS on
/// the main diagonal.
pub fn random_personality() -> Personality {
    let mut p = [[0.0; N_STATES]; N_ACTIONS];
    for (row, values) in p.iter_mut().enumerate() {
        for (col, v) in values.iter_mut().enumerate() {
            *v = fastrand::f32();
            if row == col {
                *v += DIAGONAL_BIAS;
            }
        }
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

    const IDENTITY: Personality = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];

    #[test]
    fn mat_vec_matches_hand_calculation() {
        let m = [
            [1.0, 2.0, 3.0, 4.0],
            [0.0, 1.0, 0.0, 1.0],
            [2.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.5],
        ];
        let v = [1.0, 0.0, 2.0, 1.0];
        // row 0: 1*1 + 2*0 + 3*2 + 4*1 = 11
        assert_eq!(mat_vec(&m, &v), [11.0, 1.0, 2.0, 0.5]);
    }

    #[test]
    fn hungry_creature_goes_to_food() {
        let mut c = Creature::new(0.0, 0.0, IDENTITY);
        c.food = 0.0; // hunger 1.0
        c.water = 0.8;
        c.boredom = 0.1;
        c.health = 0.5;
        c.decide();
        assert_eq!(c.action, Action::WalkToFood);
        assert!((c.last_scores.iter().sum::<f32>() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn healthy_fed_creature_reproduces() {
        let mut c = Creature::new(0.0, 0.0, IDENTITY);
        c.food = 1.0;
        c.water = 1.0;
        c.boredom = 0.2;
        c.health = 1.0;
        c.decide();
        assert_eq!(c.action, Action::Reproduce);
    }

    #[test]
    fn reproducing_costs_thirty_percent() {
        let mut parent = Creature::new(0.0, 0.0, IDENTITY);
        parent.food = 1.0;
        parent.water = 0.8;
        parent.boredom = 0.5;
        let child = parent.reproduce();
        assert!((parent.food - 0.7).abs() < 1e-5);
        assert!((parent.water - 0.5).abs() < 1e-5);
        assert!((parent.boredom - 0.2).abs() < 1e-5);
        assert_eq!(child.food, REPRODUCE_COST);
        assert_eq!(child.water, REPRODUCE_COST);
    }

    #[test]
    fn mutation_stays_within_five_percent() {
        let p = [[1.0; N_STATES]; N_ACTIONS];
        for _ in 0..1000 {
            for row in mutate(&p) {
                for v in row {
                    assert!((0.95..=1.05).contains(&v), "{v}");
                }
            }
        }
    }

    #[test]
    fn zero_health_is_dead() {
        let mut c = Creature::new(0.0, 0.0, IDENTITY);
        assert!(!c.is_dead());
        c.health = 0.0;
        assert!(c.is_dead());
    }
}
