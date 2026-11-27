// A creature = position + state vector + personality matrix.
//
//   action_vector = personality * state_vector
//
//   [ walk_to_food  ]   [ p00 p01 p02 ]   [ hunger  ]
//   [ walk_to_water ] = [ p10 p11 p12 ] * [ thirst  ]
//   [ dance         ]   [ p20 p21 p22 ]   [ boredom ]
//
// Row i of the matrix = "how much does each need push me toward action i".

pub const N_STATES: usize = 3;
pub const N_ACTIONS: usize = 3;

pub const HUNGER: usize = 0;
pub const THIRST: usize = 1;
pub const BOREDOM: usize = 2;

pub type StateVec = [f32; N_STATES];
pub type ActionVec = [f32; N_ACTIONS];
pub type Personality = [[f32; N_STATES]; N_ACTIONS];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    WalkToFood,
    WalkToWater,
    Dance,
}

impl Action {
    pub fn from_index(i: usize) -> Action {
        match i {
            0 => Action::WalkToFood,
            1 => Action::WalkToWater,
            _ => Action::Dance,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Action::WalkToFood => "food",
            Action::WalkToWater => "water",
            Action::Dance => "dance",
        }
    }
}

pub struct Creature {
    pub id: usize,
    pub x: f32,
    pub y: f32,
    pub state: StateVec, // each value in 0..1, 1 = desperate
    pub personality: Personality,
    pub action: Action,
    pub last_scores: ActionVec, // likelihoods from the last decision (sum to 1)
    pub dance_phase: f32,
}

impl Creature {
    pub fn new(id: usize, x: f32, y: f32) -> Self {
        Self {
            id,
            x,
            y,
            state: [
                fastrand::f32() * 0.5,
                fastrand::f32() * 0.5,
                fastrand::f32() * 0.5,
            ],
            personality: random_personality(),
            action: Action::Dance,
            last_scores: [0.0; N_ACTIONS],
            dance_phase: fastrand::f32() * std::f32::consts::TAU,
        }
    }

    /// Runs once per decision tick.
    pub fn decide(&mut self) {
        let scores = mat_vec(&self.personality, &self.state);

        // Turn raw scores into likelihoods (normalize so they sum to 1).
        let total: f32 = scores.iter().sum();
        self.last_scores = if total > 0.0 {
            scores.map(|s| s / total)
        } else {
            [1.0 / N_ACTIONS as f32; N_ACTIONS]
        };

        self.action = Action::from_index(argmax(&self.last_scores));
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

/// Random personality biased toward "sensible": the diagonal (hunger -> food,
/// thirst -> water, boredom -> dance) is strong, everything else is weak noise.
/// Remove the bias to get weirder creatures.
fn random_personality() -> Personality {
    let mut p = [[0.0; N_STATES]; N_ACTIONS];
    for row in 0..N_ACTIONS {
        for col in 0..N_STATES {
            p[row][col] = if row == col {
                0.5 + fastrand::f32() * 0.5 // 0.5 .. 1.0
            } else {
                fastrand::f32() * 0.3 // 0.0 .. 0.3
            };
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hungry_creature_with_identity_personality_goes_to_food() {
        let mut c = Creature::new(0, 0.0, 0.0);
        c.personality = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        c.state = [0.9, 0.2, 0.1];
        c.decide();
        assert_eq!(c.action, Action::WalkToFood);
        assert!((c.last_scores.iter().sum::<f32>() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn mat_vec_matches_hand_calculation() {
        let m = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]];
        let v = [1.0, 0.0, 2.0];
        assert_eq!(mat_vec(&m, &v), [7.0, 16.0, 25.0]);
    }
}
