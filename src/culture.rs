// Culture: what each family has learned from real outcomes, passed down to
// every descendant.
//
// How a family learns (reward-modulated Hebbian learning with an eligibility
// trace, the textbook way to credit a delayed reward):
//
// 1. Every creature remembers its last MEMORY_LEN decisions: what it felt
//    (state vector) and what it did (action).
// 2. When it drops something, that memory is stored on the pile.
// 3. If the pile later pays off (sprouts into a bush, keeps a family member
//    warm at night, feeds a family member), the family's culture matrix is
//    strengthened along that memory:
//
//      culture[action_k][s] += RATE * reward * FADE^k * state_k[s]
//
//    k = how many decisions ago (k = 0 is the drop itself), so recent steps
//    get the most credit.
//
// Nobody writes down what a discovery "means". If the dropper walked home
// before dropping, the family learns "carry food -> go home"; if it dropped
// next to a bush, it learns that instead. Superstitions are possible too.
//
// The culture matrix has the same shape as the genes and is added on top:
//   scores = (genes + family culture) * state
// Culture slowly fades unless it keeps paying off, and families teach each
// other when they share food.

use crate::creature::{Action, Personality, StateVec, N_ACTIONS, N_STATES};

pub const N_DISCOVERIES: usize = 3;

/// Decisions a creature remembers.
pub const MEMORY_LEN: usize = 10;
/// Learning rate and how fast credit fades for older memories.
const RATE: f32 = 0.08;
const FADE: f32 = 0.7;
/// No learned weight goes above this.
const MAX_WEIGHT: f32 = 1.5;
/// Fraction of every learned weight forgotten per tick (half-life ~700 ticks).
const FORGET: f32 = 0.001;
/// Teaching moves the student family's culture this far toward the teacher's.
const TEACH_RATE: f32 = 0.1;

pub type Memory = Vec<(StateVec, Action)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Discovery {
    /// A dropped food pile sprouted into a bush.
    Seeds,
    /// A wood pile kept a family member warm at night.
    Warmth,
    /// A family member ate from a food pile the family had dropped.
    Pantry,
}

pub const DISCOVERIES: [Discovery; N_DISCOVERIES] = [Discovery::Seeds, Discovery::Warmth, Discovery::Pantry];

impl Discovery {
    pub fn name(self) -> &'static str {
        match self {
            Discovery::Seeds => "SEEDS",
            Discovery::Warmth => "WARMTH",
            Discovery::Pantry => "PANTRY",
        }
    }

    /// What objectively happened, in words.
    pub fn fact(self) -> &'static str {
        match self {
            Discovery::Seeds => "DROPPED FOOD GREW INTO A BUSH",
            Discovery::Warmth => "A WOOD PILE KEPT US WARM",
            Discovery::Pantry => "WE ATE FOOD WE HAD STORED",
        }
    }

    pub fn reward(self) -> f32 {
        match self {
            Discovery::Seeds => 1.0,
            Discovery::Warmth => 1.0,
            Discovery::Pantry => 0.5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum How {
    /// Found by this creature (id, generation).
    Found { by: u64, generation: u32 },
    /// Taught by another family.
    TaughtBy(u32),
}

/// One weight that a lesson changed: action <- state, by how much.
pub type Change = (Action, usize, f32);

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub tick: u64,
    pub family: u32,
    pub discovery: Discovery,
    pub how: How,
    pub biggest: Vec<Change>, // the two weights this lesson strengthened most
}

pub struct Culture {
    known: Vec<[bool; N_DISCOVERIES]>, // per family: has it ever happened to us?
    learned: Vec<Personality>,         // per family: the learned matrix
    pub rewards: Vec<[u32; N_DISCOVERIES]>, // per family: how many times each paid off
    pub log: Vec<LogEntry>,
}

impl Culture {
    pub fn new(n_families: usize) -> Self {
        Self {
            known: vec![[false; N_DISCOVERIES]; n_families],
            learned: vec![[[0.0; N_STATES]; N_ACTIONS]; n_families],
            rewards: vec![[0; N_DISCOVERIES]; n_families],
            log: Vec::new(),
        }
    }

    pub fn knows(&self, family: u32, d: Discovery) -> bool {
        self.known[family as usize][d as usize]
    }

    pub fn known_by(&self, family: u32) -> Vec<Discovery> {
        DISCOVERIES.iter().copied().filter(|&d| self.knows(family, d)).collect()
    }

    /// The learned matrix this family adds on top of its genes.
    pub fn learned(&self, family: u32) -> &Personality {
        &self.learned[family as usize]
    }

    /// How many families know `d`.
    pub fn spread(&self, d: Discovery) -> usize {
        self.known.iter().filter(|k| k[d as usize]).count()
    }

    /// Something a family member did paid off. Strengthen the family's culture
    /// along `memory` (newest last), and log it the first time.
    pub fn reward(&mut self, family: u32, d: Discovery, memory: &Memory, tick: u64, by: (u64, u32)) {
        let f = family as usize;
        let mut changes = [[0.0f32; N_STATES]; N_ACTIONS];
        for (k, (state, action)) in memory.iter().rev().enumerate() {
            let strength = RATE * d.reward() * FADE.powi(k as i32);
            for s in 0..N_STATES {
                changes[action.index()][s] += strength * state[s];
            }
        }

        let m = &mut self.learned[f];
        for a in 0..N_ACTIONS {
            for s in 0..N_STATES {
                m[a][s] = (m[a][s] + changes[a][s]).min(MAX_WEIGHT);
            }
        }
        self.rewards[f][d as usize] += 1;

        if !self.known[f][d as usize] {
            self.known[f][d as usize] = true;
            self.log.push(LogEntry {
                tick,
                family,
                discovery: d,
                how: How::Found { by: by.0, generation: by.1 },
                biggest: biggest(&changes, 2),
            });
        }
    }

    /// Family `from` teaches family `to`: `to`'s culture moves a bit toward
    /// `from`'s, and `to` hears about every discovery `from` has made.
    pub fn teach(&mut self, from: u32, to: u32, tick: u64) {
        if from == to {
            return;
        }
        let (fi, ti) = (from as usize, to as usize);
        let teacher = self.learned[fi];
        let student = &mut self.learned[ti];
        for a in 0..N_ACTIONS {
            for s in 0..N_STATES {
                if teacher[a][s] > student[a][s] {
                    student[a][s] += TEACH_RATE * (teacher[a][s] - student[a][s]);
                }
            }
        }
        for d in DISCOVERIES {
            if self.known[fi][d as usize] && !self.known[ti][d as usize] {
                self.known[ti][d as usize] = true;
                self.log.push(LogEntry { tick, family: to, discovery: d, how: How::TaughtBy(from), biggest: Vec::new() });
            }
        }
    }

    /// Called once per tick: unused lessons slowly fade.
    pub fn forget(&mut self) {
        for m in &mut self.learned {
            for row in m.iter_mut() {
                for v in row.iter_mut() {
                    *v *= 1.0 - FORGET;
                }
            }
        }
    }
}

/// The `n` biggest entries of a change matrix.
fn biggest(changes: &Personality, n: usize) -> Vec<Change> {
    let mut all: Vec<Change> = Vec::new();
    for a in 0..N_ACTIONS {
        for s in 0..N_STATES {
            if changes[a][s] > 0.0 {
                all.push((crate::creature::ACTIONS[a], s, changes[a][s]));
            }
        }
    }
    all.sort_by(|x, y| y.2.total_cmp(&x.2));
    all.truncate(n);
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creature::{CARRY_FOOD, NEAR_HOME};

    fn state(pairs: &[(usize, f32)]) -> StateVec {
        let mut s = [0.0; N_STATES];
        for &(i, v) in pairs {
            s[i] = v;
        }
        s
    }

    #[test]
    fn reward_strengthens_the_remembered_steps_most_recent_first() {
        let mut c = Culture::new(2);
        let memory: Memory = vec![
            (state(&[(CARRY_FOOD, 1.0)]), Action::GoHome), // older
            (state(&[(CARRY_FOOD, 1.0), (NEAR_HOME, 1.0)]), Action::Drop), // the drop
        ];
        c.reward(1, Discovery::Seeds, &memory, 100, (7, 3));

        let m = c.learned(1);
        let drop = m[Action::Drop.index()][NEAR_HOME];
        let go_home = m[Action::GoHome.index()][CARRY_FOOD];
        assert!((drop - RATE).abs() < 1e-6);
        assert!((go_home - RATE * FADE).abs() < 1e-6);
        assert!(drop > go_home);
        assert_eq!(c.learned(0)[Action::Drop.index()][NEAR_HOME], 0.0); // other family untouched

        assert_eq!(c.log.len(), 1);
        assert_eq!(c.log[0].how, How::Found { by: 7, generation: 3 });
        c.reward(1, Discovery::Seeds, &memory, 200, (8, 4));
        assert_eq!(c.log.len(), 1); // only logged the first time
        assert_eq!(c.rewards[1][Discovery::Seeds as usize], 2);
    }

    #[test]
    fn teaching_and_forgetting() {
        let mut c = Culture::new(2);
        let memory: Memory = vec![(state(&[(CARRY_FOOD, 1.0)]), Action::Drop)];
        c.reward(0, Discovery::Warmth, &memory, 10, (1, 0));
        c.teach(0, 1, 50);
        assert!(c.knows(1, Discovery::Warmth));
        assert!(c.learned(1)[Action::Drop.index()][CARRY_FOOD] > 0.0);
        assert_eq!(c.log.last().unwrap().how, How::TaughtBy(0));

        let before = c.learned(0)[Action::Drop.index()][CARRY_FOOD];
        c.forget();
        assert!(c.learned(0)[Action::Drop.index()][CARRY_FOOD] < before);
    }
}
