// Goals: WHAT an NPC is trying to do. Chosen from its state (needs, feelings
// and a few salient events), kept until done, given up, or beaten by a clearly
// better goal. Emergencies interrupt without erasing what was going on.
//
// Picking the goal:
//   score[g] = (genes[g] . state) * trait multiplier(g) + leader nudge
//   - a new goal only takes over if it beats the current one by a margin
//     (STUBBORNNESS), so close calls can't make the NPC flip-flop
//   - when a challenger gets close to that margin the NPC hesitates
//     (DELIBERATION) and, if it follows a leader, may just copy the leader
//   - emergency goals (STAY SAFE, DEFEND) are pushed on top of the stack;
//     when they're done, the NPC goes back to what it was doing
//   - every goal has a done condition and a give-up timer (PATIENCE)

use crate::npc::*;

pub const N_GOALS: usize = 14;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goal {
    FixHunger,
    FixThirst,
    Rest,
    KeepWarm,
    Reproduce,
    StaySafe,
    Socialize,
    Work,
    Explore,
    Hunt,
    Raid,
    Avenge,
    Defend,
    Revere,
}

pub const GOALS: [Goal; N_GOALS] = [
    Goal::FixHunger,
    Goal::FixThirst,
    Goal::Rest,
    Goal::KeepWarm,
    Goal::Reproduce,
    Goal::StaySafe,
    Goal::Socialize,
    Goal::Work,
    Goal::Explore,
    Goal::Hunt,
    Goal::Raid,
    Goal::Avenge,
    Goal::Defend,
    Goal::Revere,
];

impl Goal {
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Goal::FixHunger => "FIX HUNGER",
            Goal::FixThirst => "FIX THIRST",
            Goal::Rest => "REST",
            Goal::KeepWarm => "KEEP WARM",
            Goal::Reproduce => "REPRODUCE",
            Goal::StaySafe => "STAY SAFE",
            Goal::Socialize => "SOCIALIZE",
            Goal::Work => "WORK",
            Goal::Explore => "EXPLORE",
            Goal::Hunt => "HUNT",
            Goal::Raid => "RAID",
            Goal::Avenge => "AVENGE",
            Goal::Defend => "DEFEND",
            Goal::Revere => "REVERE",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Goal::FixHunger => "HUNG",
            Goal::FixThirst => "THRS",
            Goal::Rest => "REST",
            Goal::KeepWarm => "WARM",
            Goal::Reproduce => "REPR",
            Goal::StaySafe => "SAFE",
            Goal::Socialize => "SOCL",
            Goal::Work => "WORK",
            Goal::Explore => "EXPL",
            Goal::Hunt => "HUNT",
            Goal::Raid => "RAID",
            Goal::Avenge => "AVNG",
            Goal::Defend => "DEFN",
            Goal::Revere => "REVR",
        }
    }

    pub fn color(self) -> u32 {
        match self {
            Goal::FixHunger => 0x4caf50,
            Goal::FixThirst => 0x2196f3,
            Goal::Rest => 0xffffff,
            Goal::KeepWarm => 0xff7043,
            Goal::Reproduce => 0xffeb3b,
            Goal::StaySafe => 0xff9800,
            Goal::Socialize => 0x00bcd4,
            Goal::Work => 0xa1887f,
            Goal::Explore => 0x9e9e9e,
            Goal::Hunt => 0x8d6e63,
            Goal::Raid => 0xe91e63,
            Goal::Avenge => 0xd50000,
            Goal::Defend => 0x7e57c2,
            Goal::Revere => 0xffd54f,
        }
    }

    /// Emergencies interrupt the current goal instead of replacing it.
    pub fn is_emergency(self) -> bool {
        matches!(self, Goal::StaySafe | Goal::Defend)
    }

    /// How style traits scale this goal's score. This is where "aggressive"
    /// or "sociable" changes WHAT an NPC wants without touching its needs.
    pub fn trait_multiplier(self, t: &Traits) -> f32 {
        match self {
            Goal::StaySafe => 1.3 - 0.6 * t[COURAGE],
            Goal::Socialize => 0.3 + t[SOCIABILITY],
            Goal::Explore => 0.4 + t[CURIOSITY],
            Goal::Hunt => 0.5 + 0.5 * t[COURAGE] + 0.3 * t[AGGRESSION],
            Goal::Raid => 0.1 + 1.2 * t[AGGRESSION],
            Goal::Avenge => 0.1 + 1.2 * t[VENGEFULNESS],
            Goal::Defend => 0.3 + t[LOYALTY],
            Goal::Work => 0.6 + 0.6 * t[THRIFT],
            _ => 1.0,
        }
    }
}

/// Starting "instinct" weights for random goal genes: (goal, state, strength).
/// Everything else starts at 0..RANDOM_MAX.
pub const RANDOM_MAX: f32 = 0.2;
pub const INSTINCTS: [(Goal, usize, f32); 17] = [
    (Goal::FixHunger, HUNGER, 1.0),
    (Goal::FixThirst, THIRST, 1.0),
    (Goal::Rest, FATIGUE, 1.0),
    (Goal::KeepWarm, COLD, 1.0),
    (Goal::Reproduce, HEALTH, 0.5),
    (Goal::StaySafe, THREAT, 1.2),
    (Goal::StaySafe, FEAR, 0.5),
    (Goal::Socialize, LONELINESS, 0.8),
    (Goal::Work, BIAS, 0.25),
    (Goal::Explore, BIAS, 0.1),
    (Goal::Hunt, PREY_NEAR, 0.4),
    (Goal::Hunt, HUNGER, 0.3),
    (Goal::Raid, HUNGER, 0.3),
    (Goal::Avenge, GRUDGE, 0.9),
    (Goal::Defend, KIN_THREAT, 1.1),
    (Goal::Revere, REVERENCE, 1.0),
    (Goal::Raid, GRUDGE, 0.3),
];

pub fn random_goal_genes(rng: &mut fastrand::Rng) -> GoalGenes {
    let mut g = [[0.0; N_STATES]; N_GOALS];
    for row in g.iter_mut() {
        for v in row.iter_mut() {
            *v = rng.f32() * RANDOM_MAX;
        }
    }
    for (goal, state, strength) in INSTINCTS {
        g[goal.index()][state] += strength;
    }
    g
}

/// One entry on an NPC's goal stack.
#[derive(Clone, Copy, Debug)]
pub struct GoalFrame {
    pub goal: Goal,
    pub ticks: f32,      // how long it has been pursued
    pub interrupt: bool, // pushed on top as an emergency
    pub pondered: bool,  // already hesitated over a close call for this goal
}

/// Emergency goals push onto the stack once their score passes this.
pub const EMERGENCY_LEVEL: f32 = 0.6;
/// The stack never grows past this; the oldest suspended goal is dropped.
pub const MAX_STACK: usize = 4;
/// After giving up on a goal, its score is cut for this many ticks.
pub const GIVE_UP_COOLDOWN: f32 = 15.0;

/// What happened when the goal was (re)considered this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GoalChange {
    Kept,
    Started,
    Switched,
    Interrupted,
    Finished,
    GaveUp,
    Hesitating,
    CopiedLeader,
}

/// Decide what to do with the goal stack. Pure logic so it can be tested.
/// `scores` = 0 means the goal is impossible right now. `done` says whether
/// the current goal's done condition holds. `leader_goal` is what the leader
/// this NPC follows is doing, if any.
pub fn choose_goal(
    stack: &mut Vec<GoalFrame>,
    scores: &[f32; N_GOALS],
    done: bool,
    traits: &Traits,
    leader_goal: Option<Goal>,
    hesitation: &mut f32,
    roll: f32,
) -> GoalChange {
    // finished or impossible: pop and resume whatever is underneath
    let mut change = GoalChange::Kept;
    let mut gave_up_on = None;
    if let Some(top) = stack.last().copied() {
        let patience_ticks = 10.0 + traits[PATIENCE] * 50.0;
        if done || scores[top.goal.index()] <= 0.0 {
            stack.pop();
            change = GoalChange::Finished;
        } else if top.ticks > patience_ticks {
            stack.pop();
            change = GoalChange::GaveUp;
            gave_up_on = Some(top.goal);
        }
    }
    // the best goal, not counting one just given up on
    let best = (0..N_GOALS)
        .filter(|&i| Some(GOALS[i]) != gave_up_on)
        .max_by(|&a, &b| scores[a].total_cmp(&scores[b]))
        .map(|i| GOALS[i])
        .filter(|g| scores[g.index()] > 0.0);
    // suspended goals that no longer matter are dropped
    stack.retain(|f| scores[f.goal.index()] > 0.05);

    let Some(best) = best else { return change };
    let Some(top) = stack.last().copied() else {
        stack.push(GoalFrame { goal: best, ticks: 0.0, interrupt: false, pondered: false });
        return if change == GoalChange::Kept { GoalChange::Started } else { change };
    };
    if best == top.goal {
        return change;
    }

    let current = scores[top.goal.index()];
    let challenger = scores[best.index()];

    // emergencies interrupt
    if best.is_emergency() && challenger > EMERGENCY_LEVEL && challenger > current {
        if !stack.iter().any(|f| f.goal == best) {
            stack.push(GoalFrame { goal: best, ticks: 0.0, interrupt: true, pondered: false });
            if stack.len() > MAX_STACK {
                stack.remove(0);
            }
            return GoalChange::Interrupted;
        }
    }

    // ordinary switch only past the stubbornness margin
    let threshold = current * (1.0 + 0.6 * traits[STUBBORNNESS]) + 0.02;
    if challenger > threshold {
        let interrupt = top.interrupt;
        *stack.last_mut().unwrap() = GoalFrame { goal: best, ticks: 0.0, interrupt, pondered: false };
        return GoalChange::Switched;
    }

    // close call: hesitate (once per goal: think it over, then commit), and
    // maybe just do what the leader does
    let closeness = (challenger / threshold).clamp(0.0, 1.0);
    if closeness > 0.85 && !top.pondered {
        stack.last_mut().unwrap().pondered = true;
        *hesitation = 1.0 * traits[DELIBERATION] * closeness;
        if let Some(lg) = leader_goal {
            if lg != top.goal && scores[lg.index()] > 0.0 && roll < traits[CONFORMITY] {
                let interrupt = top.interrupt;
                *stack.last_mut().unwrap() = GoalFrame { goal: lg, ticks: 0.0, interrupt, pondered: false };
                return GoalChange::CopiedLeader;
            }
        }
        return GoalChange::Hesitating;
    }
    change
}

#[cfg(test)]
mod tests {
    use super::*;

    fn traits(stubborn: f32) -> Traits {
        let mut t = [0.5; N_TRAITS];
        t[STUBBORNNESS] = stubborn;
        t[PATIENCE] = 1.0;
        t
    }

    fn scores(pairs: &[(Goal, f32)]) -> [f32; N_GOALS] {
        let mut s = [0.0; N_GOALS];
        for &(g, v) in pairs {
            s[g.index()] = v;
        }
        s
    }

    #[test]
    fn close_challenger_does_not_steal_the_goal() {
        let mut stack = vec![GoalFrame { goal: Goal::FixHunger, ticks: 0.0, interrupt: false, pondered: false }];
        let mut h = 0.0;
        // threshold = 0.8 * (1 + 0.6*0.5) + 0.02 = 1.06
        let s = scores(&[(Goal::FixHunger, 0.8), (Goal::FixThirst, 0.95)]);
        let c = choose_goal(&mut stack, &s, false, &traits(0.5), None, &mut h, 1.0);
        assert_eq!(stack.last().unwrap().goal, Goal::FixHunger);
        assert_eq!(c, GoalChange::Hesitating); // 0.95 / 1.06 = 0.9 -> close call
        assert!(h > 0.0);
        // same close call next tick: already thought it over, keeps going
        let c = choose_goal(&mut stack, &s, false, &traits(0.5), None, &mut h, 1.0);
        assert_eq!(c, GoalChange::Kept);
        let s = scores(&[(Goal::FixHunger, 0.8), (Goal::FixThirst, 1.1)]);
        let c = choose_goal(&mut stack, &s, false, &traits(0.5), None, &mut h, 1.0);
        assert_eq!(c, GoalChange::Switched);
        assert_eq!(stack.last().unwrap().goal, Goal::FixThirst);
    }

    #[test]
    fn emergency_interrupts_then_old_goal_resumes() {
        let mut stack = vec![GoalFrame { goal: Goal::FixHunger, ticks: 3.0, interrupt: false, pondered: false }];
        let mut h = 0.0;
        let s = scores(&[(Goal::FixHunger, 0.8), (Goal::StaySafe, 1.2)]);
        let c = choose_goal(&mut stack, &s, false, &traits(0.5), None, &mut h, 1.0);
        assert_eq!(c, GoalChange::Interrupted);
        assert_eq!(stack.len(), 2);
        // threat gone: STAY SAFE is done, hunger is back on top
        let s = scores(&[(Goal::FixHunger, 0.8), (Goal::StaySafe, 0.3)]);
        let c = choose_goal(&mut stack, &s, true, &traits(0.5), None, &mut h, 1.0);
        assert_eq!(c, GoalChange::Finished);
        assert_eq!(stack.last().unwrap().goal, Goal::FixHunger);
        assert_eq!(stack.last().unwrap().ticks, 3.0);
    }

    #[test]
    fn hesitating_follower_copies_the_leader() {
        let mut stack = vec![GoalFrame { goal: Goal::Rest, ticks: 0.0, interrupt: false, pondered: false }];
        let mut h = 0.0;
        let mut t = traits(0.5);
        t[CONFORMITY] = 1.0;
        let s = scores(&[(Goal::Rest, 0.8), (Goal::Work, 0.95), (Goal::Hunt, 0.4)]);
        let c = choose_goal(&mut stack, &s, false, &t, Some(Goal::Hunt), &mut h, 0.5);
        assert_eq!(c, GoalChange::CopiedLeader);
        assert_eq!(stack.last().unwrap().goal, Goal::Hunt);
    }

    #[test]
    fn impatient_npcs_give_up() {
        let mut stack = vec![GoalFrame { goal: Goal::Hunt, ticks: 100.0, interrupt: false, pondered: false }];
        let mut h = 0.0;
        let mut t = traits(0.5);
        t[PATIENCE] = 0.0; // gives up after 10 ticks
        let s = scores(&[(Goal::Hunt, 0.8), (Goal::Rest, 0.3)]);
        let c = choose_goal(&mut stack, &s, false, &t, None, &mut h, 1.0);
        assert_eq!(c, GoalChange::GaveUp);
        assert_eq!(stack.last().unwrap().goal, Goal::Rest);
    }
}
