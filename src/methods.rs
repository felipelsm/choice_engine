// Methods: HOW an NPC goes about its goal, decided on the fly from its
// surroundings. Every method is a list of primitive steps:
//   - innate methods are single primitives (walk to a bush and eat, flee...)
//   - learned methods are sequences a family discovered (see culture.rs)
//   - authored methods are sequences written for story NPCs (story.toml)
// They all work the same way; only their `source` differs.

use crate::goals::Goal;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    Food,
    Wood,
}

/// Primitive micro actions: the only things an NPC's body can actually do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Prim {
    EatHand,     // eat carried food
    EatBush,     // walk to a bush and eat
    EatPile,     // walk to an own-family food pile and eat
    Drink,       // walk to water and drink
    Rest,        // sit and recover
    GoShelter,   // walk to the nearest shelter
    Wander,      // walk somewhere new
    PickFood,    // walk to food and carry it
    PickWood,    // walk to a tree and carry wood
    Drop,        // drop what you carry, right here
    GoHome,      // walk home
    FollowKin,   // walk toward family
    FollowLeader, // walk toward the leader (or a revered being)
    Talk,        // chat: swap stories, praise, or insult (depends on feelings)
    Ask,         // ask / beg someone for food
    Trade,       // offer what you carry in exchange for what you need
    Share,       // give food to a hungry neighbor
    Steal,       // grab food from a non-family neighbor
    StealPile,   // take food from another family's pile
    Attack,      // fight a creature or animal
    Flee,        // run from a threat
    Mate,        // have a child with a partner
    Bud,         // have a child alone (costlier)
    Offer,       // carry food to a revered being or sacred place and leave it
    Rally,       // call others to join what you're doing
    Guard,       // stand watch at home
}

impl Prim {
    pub fn name(self) -> &'static str {
        match self {
            Prim::EatHand => "EAT FROM HANDS",
            Prim::EatBush => "EAT AT BUSH",
            Prim::EatPile => "EAT STORED FOOD",
            Prim::Drink => "DRINK",
            Prim::Rest => "REST",
            Prim::GoShelter => "GO TO SHELTER",
            Prim::Wander => "WANDER",
            Prim::PickFood => "PICK UP FOOD",
            Prim::PickWood => "PICK UP WOOD",
            Prim::Drop => "DROP",
            Prim::GoHome => "GO HOME",
            Prim::FollowKin => "FOLLOW FAMILY",
            Prim::FollowLeader => "FOLLOW LEADER",
            Prim::Talk => "TALK",
            Prim::Ask => "ASK FOR FOOD",
            Prim::Trade => "TRADE",
            Prim::Share => "SHARE FOOD",
            Prim::Steal => "STEAL",
            Prim::StealPile => "RAID A PILE",
            Prim::Attack => "ATTACK",
            Prim::Flee => "FLEE",
            Prim::Mate => "MATE",
            Prim::Bud => "BUD",
            Prim::Offer => "MAKE AN OFFERING",
            Prim::Rally => "RALLY OTHERS",
            Prim::Guard => "GUARD HOME",
        }
    }

    /// Every primitive, for random play / exploration.
    pub const ALL: [Prim; 26] = [
        Prim::EatHand, Prim::EatBush, Prim::EatPile, Prim::Drink, Prim::Rest, Prim::GoShelter,
        Prim::Wander, Prim::PickFood, Prim::PickWood, Prim::Drop, Prim::GoHome, Prim::FollowKin,
        Prim::FollowLeader, Prim::Talk, Prim::Ask, Prim::Trade, Prim::Share, Prim::Steal,
        Prim::StealPile, Prim::Attack, Prim::Flee, Prim::Mate, Prim::Bud, Prim::Offer, Prim::Rally,
        Prim::Guard,
    ];

    /// Primitives that change the world in a way that can lead to a discovery.
    pub fn is_constructive(self) -> bool {
        matches!(self, Prim::PickFood | Prim::PickWood | Prim::GoHome | Prim::Drop)
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Source {
    Innate,
    Learned { tick: u64, by: u64 },
    Authored,
}

#[derive(Clone, Debug)]
pub struct Method {
    pub name: String,
    pub steps: Vec<Prim>,
    pub goals: Vec<Goal>,
    pub source: Source,
}

impl Method {
    pub fn innate(p: Prim) -> Self {
        Method { name: p.name().to_string(), steps: vec![p], goals: Vec::new(), source: Source::Innate }
    }

    pub fn steps_text(&self) -> String {
        self.steps.iter().map(|p| p.name()).collect::<Vec<_>>().join(" > ")
    }
}

/// Innate methods available for each goal (single primitives).
pub fn innate_for(goal: Goal) -> &'static [Prim] {
    match goal {
        Goal::FixHunger => &[Prim::EatHand, Prim::EatBush, Prim::EatPile, Prim::Ask, Prim::Trade, Prim::Steal],
        Goal::FixThirst => &[Prim::Drink],
        Goal::Rest => &[Prim::Rest, Prim::GoShelter],
        Goal::KeepWarm => &[Prim::GoShelter, Prim::FollowKin, Prim::Trade, Prim::Rest],
        Goal::Reproduce => &[Prim::Mate, Prim::Bud],
        Goal::StaySafe => &[Prim::Flee, Prim::GoShelter, Prim::FollowKin, Prim::Attack],
        Goal::Socialize => &[Prim::Talk, Prim::Share, Prim::Trade, Prim::FollowKin, Prim::FollowLeader],
        Goal::Work => &[Prim::PickFood, Prim::PickWood, Prim::Drop, Prim::GoHome, Prim::Guard],
        Goal::Explore => &[Prim::Wander],
        Goal::Hunt => &[Prim::Attack, Prim::Rally],
        Goal::Raid => &[Prim::StealPile, Prim::Steal, Prim::Rally],
        Goal::Avenge => &[Prim::Attack],
        Goal::Defend => &[Prim::Attack, Prim::Guard, Prim::Rally],
        Goal::Revere => &[Prim::Offer, Prim::FollowLeader],
    }
}

/// Library of methods a story file can give an NPC by name.
pub fn authored(name: &str) -> Option<Method> {
    let m = |n: &str, steps: Vec<Prim>, goals: Vec<Goal>| Method {
        name: n.to_string(),
        steps,
        goals,
        source: Source::Authored,
    };
    match name {
        "plant" => Some(m("PLANT", vec![Prim::PickFood, Prim::GoHome, Prim::Drop], vec![Goal::Work, Goal::FixHunger])),
        "store_food" => Some(m("STORE FOOD", vec![Prim::PickFood, Prim::GoHome, Prim::Drop], vec![Goal::Work])),
        "bring_wood_home" => Some(m(
            "BRING WOOD HOME",
            vec![Prim::PickWood, Prim::GoHome, Prim::Drop],
            vec![Goal::KeepWarm, Goal::Work],
        )),
        "group_hunt" => Some(m("GROUP HUNT", vec![Prim::Rally, Prim::Attack], vec![Goal::Hunt])),
        _ => None,
    }
}

/// What a primitive is aimed at, resolved at the decision tick.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Target {
    Here,
    Pos(f32, f32),
    Npc(u64),
    Animal(u64),
    Bush(usize),
    Pile(usize),
    Water(usize),
    Tree(usize),
}

/// The method an NPC is currently running.
#[derive(Clone, Debug)]
pub struct Active {
    pub goal: Goal,
    pub method: Method,
    pub step: usize,
    pub target: Target,
    pub timer: f32, // seconds on the current step
}

impl Active {
    pub fn prim(&self) -> Prim {
        self.method.steps[self.step]
    }
}
