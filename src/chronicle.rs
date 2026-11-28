// The chronicle and legends.
//
// The CHRONICLE is the world's objective record of notable events: rescues,
// raids, killings, wolf attacks, famines, discoveries, first shelters,
// leaders dying, migrations.
//
// A LEGEND (belief) is what one NPC thinks about one of those events. Every
// belief points back to its chronicle entry, so truth and belief can always
// be compared. Beliefs are:
//   - given to witnesses and to the affected family when the event happens
//   - inherited by children from both parents, and spread by talking
//   - distorted every time they're passed on (magnitude drifts, details get
//     lost); stories about someone who isn't around drift upward unchecked
//   - strengthened when retold in hard times, and slowly forgotten otherwise
//   - trusted less when told by strangers, and not at all from enemies

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EventKind {
    Rescue,
    Raid,
    Killing,
    WolfAttack,
    WolfSlain,
    Famine,
    Discovery,
    FirstShelter,
    LeaderDied,
    Migration,
    Feud,
    Peace,
}

impl EventKind {
    pub fn name(self) -> &'static str {
        match self {
            EventKind::Rescue => "RESCUE",
            EventKind::Raid => "RAID",
            EventKind::Killing => "KILLING",
            EventKind::WolfAttack => "WOLF ATTACK",
            EventKind::WolfSlain => "WOLF SLAIN",
            EventKind::Famine => "FAMINE",
            EventKind::Discovery => "DISCOVERY",
            EventKind::FirstShelter => "FIRST SHELTER",
            EventKind::LeaderDied => "LEADER DIED",
            EventKind::Migration => "MIGRATION",
            EventKind::Feud => "FEUD",
            EventKind::Peace => "PEACE",
        }
    }
}

/// Who or what a story is about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Subject {
    Being(u64),   // a specific NPC
    Family(u32),  // a whole family
    Place(f32, f32),
    Wolves,
    Stranger,     // a being whose identity was lost in the retelling
}

#[derive(Clone, Debug)]
#[allow(dead_code)] // id / subject / place are kept for tools and future views
pub struct Event {
    pub id: usize,
    pub tick: u64,
    pub kind: EventKind,
    pub subject: Subject,        // who did it / what it's about
    pub actor_family: Option<u32>, // family of whoever did it
    pub affected: Option<u32>,   // the family it happened to
    pub place: (f32, f32),
    pub magnitude: f32,          // how big it was (deaths, people saved...)
    pub text: String,            // objective description
}

#[derive(Clone, Copy, Debug)]
pub struct Belief {
    pub event: usize,
    pub subject: Subject,
    pub valence: f32,   // +1 good for us, -1 bad for us
    pub magnitude: f32, // how big the story is now (starts near the truth)
    pub hops: u8,       // how many retellings from a witness
    pub place: (f32, f32),
}

/// Beliefs an NPC can hold at once (the weakest are forgotten first).
pub const MAX_BELIEFS: usize = 8;
const DRIFT: f32 = 0.2; //          random spread per retelling
const ABSENT_BIAS: f32 = 0.04; //   upward drift when the subject isn't around
const PRESENT_BIAS: f32 = -0.05; // stories about someone you can see stay grounded
const DETAIL_LOSS: f32 = 0.08; //   chance per retelling to forget who it was
const FORGET_BELOW: f32 = 0.15;
const FADE: f32 = 0.998; //         per tick (half-life ~350 ticks)
const MAX_MAGNITUDE: f32 = 6.0;

pub struct Chronicle {
    pub events: Vec<Event>,
}

impl Chronicle {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn record(
        &mut self,
        tick: u64,
        kind: EventKind,
        subject: Subject,
        actor_family: Option<u32>,
        affected: Option<u32>,
        place: (f32, f32),
        magnitude: f32,
        text: String,
    ) -> usize {
        let id = self.events.len();
        self.events.push(Event { id, tick, kind, subject, actor_family, affected, place, magnitude, text });
        id
    }

    pub fn get(&self, id: usize) -> &Event {
        &self.events[id]
    }

    /// Last event of `kind` matching `pred`, if it's recent enough to merge into.
    pub fn recent_mut(&mut self, kind: EventKind, since: u64, pred: impl Fn(&Event) -> bool) -> Option<&mut Event> {
        self.events.iter_mut().rev().take_while(|e| e.tick >= since).find(|e| e.kind == kind && pred(e))
    }
}

/// A belief passed from one NPC to another (or to a child). Returns None if
/// the story was too weak to stick. `subject_present`: is the subject around?
pub fn retell(b: &Belief, trust: f32, subject_present: bool, rng: &mut fastrand::Rng) -> Option<Belief> {
    // only stories about a person grow into myths; stories about families,
    // places or wolves just drift randomly
    let about_someone = matches!(b.subject, Subject::Being(_) | Subject::Stranger);
    let bias = match (about_someone, subject_present) {
        (true, false) => ABSENT_BIAS,
        (true, true) => PRESENT_BIAS,
        (false, _) => -0.02,
    };
    let noise = (rng.f32() * 2.0 - 1.0) * DRIFT;
    let mut out = *b;
    out.magnitude = (b.magnitude * (bias + noise).exp() * (0.5 + 0.5 * trust)).min(MAX_MAGNITUDE);
    out.hops = b.hops.saturating_add(1);
    if matches!(out.subject, Subject::Being(_)) && rng.f32() < DETAIL_LOSS {
        out.subject = Subject::Stranger;
    }
    (out.magnitude >= FORGET_BELOW).then_some(out)
}

/// Add or merge a belief into a list. Two versions of the same event meet in
/// the middle; the list keeps the MAX_BELIEFS strongest.
pub fn absorb(beliefs: &mut Vec<Belief>, b: Belief) {
    if let Some(old) = beliefs.iter_mut().find(|o| o.event == b.event) {
        old.magnitude = (old.magnitude + b.magnitude) / 2.0;
        old.hops = old.hops.min(b.hops);
        return;
    }
    beliefs.push(b);
    if beliefs.len() > MAX_BELIEFS {
        beliefs.sort_by(|x, y| y.magnitude.total_cmp(&x.magnitude));
        beliefs.truncate(MAX_BELIEFS);
    }
}

/// Once per tick: stories fade, and the weakest are forgotten.
pub fn fade(beliefs: &mut Vec<Belief>) {
    for b in beliefs.iter_mut() {
        b.magnitude *= FADE;
    }
    beliefs.retain(|b| b.magnitude >= FORGET_BELOW);
}

/// In hard times an NPC sometimes recalls its biggest story, which keeps it
/// alive (roughly cancels out a few days of fading).
pub fn recall_in_hardship(beliefs: &mut [Belief], roll: f32) {
    if roll > 0.02 {
        return;
    }
    if let Some(b) = beliefs.iter_mut().max_by(|x, y| x.magnitude.total_cmp(&y.magnitude)) {
        b.magnitude = (b.magnitude * 1.03).min(MAX_MAGNITUDE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn belief(m: f32) -> Belief {
        Belief { event: 0, subject: Subject::Being(9), valence: 1.0, magnitude: m, hops: 0, place: (0.0, 0.0) }
    }

    #[test]
    fn stories_about_absent_beings_grow_on_average() {
        let mut rng = fastrand::Rng::with_seed(1);
        let (mut absent, mut present) = (0.0, 0.0);
        for _ in 0..2000 {
            absent += retell(&belief(1.0), 1.0, false, &mut rng).map_or(0.0, |b| b.magnitude);
            present += retell(&belief(1.0), 1.0, true, &mut rng).map_or(0.0, |b| b.magnitude);
        }
        assert!(absent > present);
        assert!(absent / 2000.0 > 1.0);
    }

    #[test]
    fn distrust_shrinks_stories_and_hops_count_up() {
        let mut rng = fastrand::Rng::with_seed(2);
        let b = retell(&belief(1.0), 0.0, true, &mut rng).unwrap();
        assert!(b.magnitude < 0.75);
        assert_eq!(b.hops, 1);
    }

    #[test]
    fn absorbing_merges_versions_of_the_same_event() {
        let mut list = vec![belief(1.0)];
        absorb(&mut list, belief(3.0));
        assert_eq!(list.len(), 1);
        assert!((list[0].magnitude - 2.0).abs() < 1e-5);
        for i in 1..20 {
            let mut b = belief(i as f32 * 0.1 + 0.2);
            b.event = i;
            absorb(&mut list, b);
        }
        assert_eq!(list.len(), MAX_BELIEFS);
    }
}
