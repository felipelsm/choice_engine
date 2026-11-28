// Culture: a family's know-how, passed down to every descendant.
//
// A discovery is only recorded when it really happens (a food pile a family
// member dropped actually sprouted, a wood pile they built actually kept one
// of them warm, they actually ate food they had stored). Piles remember what
// the dropper had been doing; when a pile pays off, that sequence of
// primitives becomes a named, reusable METHOD for the family (e.g.
// PLANT: pick up food > go home > drop). Each further payoff makes the method
// more valued. If the dropper's sequence was odd ("drop" right next to a bush),
// the family learns that odd method instead: superstitions are possible.
//
// Methods are taught to other families by talking, and slowly lose value if
// they stop paying off. Authored methods (story NPCs) never fade.

use crate::goals::Goal;
use crate::methods::{Method, Prim, Source};

pub const N_DISCOVERIES: usize = 3;
const MAX_METHODS: usize = 6;
const FIRST_VALUE: f32 = 0.4;
const REWARD_VALUE: f32 = 0.25;
const MAX_VALUE: f32 = 1.5;
const FORGET: f32 = 0.9995; // per tick, learned methods only

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Discovery {
    Seeds,
    Warmth,
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

    pub fn fact(self) -> &'static str {
        match self {
            Discovery::Seeds => "DROPPED FOOD GREW INTO A BUSH",
            Discovery::Warmth => "A WOOD PILE KEPT US WARM",
            Discovery::Pantry => "WE ATE FOOD WE HAD STORED",
        }
    }

    pub fn from_name(s: &str) -> Option<Discovery> {
        DISCOVERIES.iter().copied().find(|d| d.name().eq_ignore_ascii_case(s))
    }

    fn method_name(self) -> &'static str {
        match self {
            Discovery::Seeds => "PLANT",
            Discovery::Warmth => "BUILD UP WOOD",
            Discovery::Pantry => "STORE FOOD",
        }
    }

    fn goals(self) -> Vec<Goal> {
        match self {
            Discovery::Seeds => vec![Goal::Work, Goal::FixHunger],
            Discovery::Warmth => vec![Goal::KeepWarm, Goal::Work],
            Discovery::Pantry => vec![Goal::Work],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum How {
    Found { by: u64, generation: u32 },
    TaughtBy(u32),
    Authored(String),
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub tick: u64,
    pub family: u32,
    pub discovery: Discovery,
    pub how: How,
    pub method: Option<String>, // the steps the family took from it
}

#[derive(Clone, Debug)]
pub struct KnownMethod {
    pub method: Method,
    pub value: f32,
    pub locked: bool, // authored: never fades
}

#[derive(Clone, Debug, Default)]
pub struct FamilyCulture {
    pub methods: Vec<KnownMethod>,
    pub known: [bool; N_DISCOVERIES],
    pub rewards: [u32; N_DISCOVERIES],
}

pub struct Culture {
    pub families: Vec<FamilyCulture>,
    pub log: Vec<LogEntry>,
}

impl Culture {
    pub fn new(n_families: usize) -> Self {
        Self { families: vec![FamilyCulture::default(); n_families], log: Vec::new() }
    }

    pub fn knows(&self, family: u32, d: Discovery) -> bool {
        self.families[family as usize].known[d as usize]
    }

    pub fn known_by(&self, family: u32) -> Vec<Discovery> {
        DISCOVERIES.iter().copied().filter(|&d| self.knows(family, d)).collect()
    }

    pub fn methods(&self, family: u32) -> &[KnownMethod] {
        &self.families[family as usize].methods
    }

    pub fn spread(&self, d: Discovery) -> usize {
        self.families.iter().filter(|f| f.known[d as usize]).count()
    }

    /// A pile paid off. `memory` is what the dropper had been doing (newest
    /// last). Returns true the first time the family makes this discovery.
    pub fn reward(&mut self, family: u32, d: Discovery, memory: &[Prim], tick: u64, by: (u64, u32)) -> bool {
        let steps = method_steps(memory);
        let fam = &mut self.families[family as usize];
        fam.rewards[d as usize] += 1;
        let mut method_text = None;
        if !steps.is_empty() {
            match fam.methods.iter_mut().find(|m| m.method.steps == steps) {
                Some(m) => m.value = (m.value + REWARD_VALUE).min(MAX_VALUE),
                None => {
                    let method = Method {
                        name: d.method_name().to_string(),
                        steps: steps.clone(),
                        goals: d.goals(),
                        source: Source::Learned { tick, by: by.0 },
                    };
                    method_text = Some(method.steps_text());
                    fam.methods.push(KnownMethod { method, value: FIRST_VALUE, locked: false });
                    trim(&mut fam.methods);
                }
            }
        }
        let first = !fam.known[d as usize];
        if first {
            fam.known[d as usize] = true;
            self.log.push(LogEntry {
                tick,
                family,
                discovery: d,
                how: How::Found { by: by.0, generation: by.1 },
                method: method_text,
            });
        }
        first
    }

    /// Story knowledge: known from the start, never fades.
    pub fn author(&mut self, family: u32, d: Option<Discovery>, method: Option<Method>, note: &str) {
        let fam = &mut self.families[family as usize];
        if let Some(m) = &method {
            fam.methods.push(KnownMethod { method: m.clone(), value: 1.0, locked: true });
        }
        if let Some(d) = d {
            fam.known[d as usize] = true;
            self.log.push(LogEntry {
                tick: 0,
                family,
                discovery: d,
                how: How::Authored(note.to_string()),
                method: method.map(|m| m.steps_text()),
            });
        }
    }

    /// Family `from` teaches family `to` its discoveries and its best method.
    pub fn teach(&mut self, from: u32, to: u32, tick: u64) {
        if from == to {
            return;
        }
        let teacher = self.families[from as usize].clone();
        let student = &mut self.families[to as usize];
        for d in DISCOVERIES {
            if teacher.known[d as usize] && !student.known[d as usize] {
                student.known[d as usize] = true;
                self.log.push(LogEntry { tick, family: to, discovery: d, how: How::TaughtBy(from), method: None });
            }
        }
        let best = teacher
            .methods
            .iter()
            .filter(|m| !student.methods.iter().any(|s| s.method.steps == m.method.steps))
            .max_by(|a, b| a.value.total_cmp(&b.value));
        if let Some(m) = best {
            student.methods.push(KnownMethod { method: m.method.clone(), value: m.value * 0.6, locked: false });
            trim(&mut student.methods);
        }
    }

    pub fn forget(&mut self) {
        for fam in &mut self.families {
            for m in &mut fam.methods {
                if !m.locked {
                    m.value *= FORGET;
                }
            }
            fam.methods.retain(|m| m.locked || m.value > 0.05);
        }
    }
}

/// Turn a memory of primitives into a method: keep only the steps that move
/// things around, drop repeats, and keep the last few ending with the drop.
pub fn method_steps(memory: &[Prim]) -> Vec<Prim> {
    let mut steps: Vec<Prim> = Vec::new();
    for &p in memory.iter().filter(|p| p.is_constructive()) {
        if steps.last() != Some(&p) {
            steps.push(p);
        }
    }
    if let Some(last_drop) = steps.iter().rposition(|&p| p == Prim::Drop) {
        steps.truncate(last_drop + 1);
    }
    let start = steps.len().saturating_sub(4);
    steps.split_off(start)
}

fn trim(methods: &mut Vec<KnownMethod>) {
    while methods.len() > MAX_METHODS {
        let weakest = methods
            .iter()
            .enumerate()
            .filter(|(_, m)| !m.locked)
            .min_by(|a, b| a.1.value.total_cmp(&b.1.value))
            .map(|(i, _)| i);
        match weakest {
            Some(i) => {
                methods.remove(i);
            }
            None => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_payoff_turns_the_droppers_steps_into_a_method() {
        let mut c = Culture::new(2);
        let memory = [Prim::Drink, Prim::PickFood, Prim::PickFood, Prim::GoHome, Prim::Rest, Prim::Drop];
        assert!(c.reward(1, Discovery::Seeds, &memory, 50, (7, 2)));
        let m = &c.methods(1)[0];
        assert_eq!(m.method.steps, vec![Prim::PickFood, Prim::GoHome, Prim::Drop]);
        assert_eq!(m.method.name, "PLANT");
        assert!(!c.reward(1, Discovery::Seeds, &memory, 80, (8, 3)));
        assert!(c.methods(1)[0].value > FIRST_VALUE);
        assert_eq!(c.log.len(), 1);
    }

    #[test]
    fn teaching_and_forgetting() {
        let mut c = Culture::new(3);
        c.reward(0, Discovery::Warmth, &[Prim::PickWood, Prim::Drop], 10, (1, 0));
        c.teach(0, 2, 20);
        assert!(c.knows(2, Discovery::Warmth));
        assert_eq!(c.methods(2).len(), 1);
        assert!(c.methods(2)[0].value < c.methods(0)[0].value);
        for _ in 0..20000 {
            c.forget();
        }
        assert!(c.methods(0).is_empty());
    }

    #[test]
    fn authored_methods_never_fade() {
        let mut c = Culture::new(1);
        c.author(0, Some(Discovery::Warmth), crate::methods::authored("bring_wood_home"), "story");
        for _ in 0..20000 {
            c.forget();
        }
        assert_eq!(c.methods(0).len(), 1);
        assert!(matches!(c.log[0].how, How::Authored(_)));
    }
}
