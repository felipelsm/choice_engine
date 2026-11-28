// Conversations, begging and trading. Pure decision logic (no world access)
// so it can be tested; world.rs applies the results.
//
// Every exchange is evaluated by the listener using:
//   attitude = family bond + personal affinity + feeling about the speaker's family
// and its own needs and mood (annoyance). Prejudice isn't hardcoded: feelings
// about whole families come from raids, killings and gossip (legends).

use crate::methods::Item;
use crate::npc::*;

/// How `listener` feels about someone (roughly -1.5 .. 1.5).
pub fn attitude(listener: &Npc, speaker_id: u64, speaker_family: u32) -> f32 {
    let kin = if speaker_family == listener.family { 0.5 } else { -0.15 * listener.traits[LOYALTY] };
    kin + listener.affinity(speaker_id) + 0.6 * listener.family_feeling(speaker_family)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TalkMove {
    Chat,   // small talk / gossip: share a story
    Praise, // compliments: both like each other more
    Insult, // the speaker lets its dislike or bad mood out
}

/// What the speaker says, given how it feels about the listener.
pub fn talk_move(speaker: &Npc, attitude_to_listener: f32, roll: f32) -> TalkMove {
    let t = &speaker.traits;
    let insult_chance = (0.5 * t[AGGRESSION] + 0.6 * speaker.annoyance) * (0.2 - attitude_to_listener).max(0.0) * 1.5;
    if roll < insult_chance {
        TalkMove::Insult
    } else if attitude_to_listener > 0.3 && roll > 1.0 - 0.4 * t[SOCIABILITY] {
        TalkMove::Praise
    } else {
        TalkMove::Chat
    }
}

/// How the listener takes it: change in its affinity toward the speaker and
/// in its annoyance, and whether it snaps (insults back and holds a grudge).
pub struct Reaction {
    pub affinity: f32,
    pub annoyance: f32,
    pub snaps: bool,
}

pub fn react(listener: &Npc, m: TalkMove) -> Reaction {
    let t = &listener.traits;
    match m {
        TalkMove::Chat => Reaction { affinity: 0.05 + 0.05 * t[SOCIABILITY], annoyance: -0.05, snaps: false },
        TalkMove::Praise => Reaction { affinity: 0.15, annoyance: -0.1, snaps: false },
        TalkMove::Insult => {
            let annoyance = 0.2 + 0.3 * (1.0 - t[PATIENCE]);
            let snaps = listener.annoyance + annoyance > 0.7 && t[COURAGE] > 0.3;
            Reaction { affinity: -0.25 - 0.2 * t[VENGEFULNESS], annoyance, snaps }
        }
    }
}

/// Someone asks `giver` for food. Returns how much it gives (0 = refuses).
/// Friends and kin give easily, hungry askers move people, thrifty, hungry
/// or annoyed givers refuse.
pub fn answer_ask(giver: &Npc, attitude_to_asker: f32, asker_hunger: f32) -> f32 {
    let t = &giver.traits;
    let has = giver.carrying(Item::Food) >= 0.15 || giver.food > 0.6;
    if !has {
        return 0.0;
    }
    let willing = attitude_to_asker + 0.5 * (1.0 - t[THRIFT]) + 0.6 * asker_hunger
        - 0.6 * (1.0 - giver.food)
        - 0.5 * giver.annoyance;
    if willing > 0.6 { 0.15 } else { 0.0 }
}

/// How much `npc` values an item right now (per unit).
pub fn value(npc: &Npc, item: Item) -> f32 {
    match item {
        Item::Food => 0.2 + (1.0 - npc.food),
        Item::Wood => 0.2 + npc.perception.cold + 0.2 * npc.traits[THRIFT],
    }
}

/// `seller` is offered `give` units of `offered` in exchange for its `wanted`.
/// Returns how many units of `wanted` it hands over, or None to refuse.
/// Friends trade 1:1; strangers want a better rate; enemies refuse.
pub fn answer_trade(seller: &Npc, attitude_to_buyer: f32, offered: Item, give: f32, wanted: Item) -> Option<f32> {
    if attitude_to_buyer < -0.4 {
        return None;
    }
    let have = seller.carrying(wanted);
    if have < 0.05 || offered == wanted {
        return None;
    }
    // how much better than 1:1 it wants the deal to be
    let markup = 1.0 + (0.3 - attitude_to_buyer).max(0.0) * 1.5;
    let gets = value(seller, offered) * give;
    // largest amount it'll hand over at that markup
    let max_out = gets / (value(seller, wanted) * markup);
    let out = max_out.min(have);
    (out >= 0.05).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn npc(family: u32) -> Npc {
        let mut rng = fastrand::Rng::with_seed(family as u64 + 10);
        let mut n = Npc::new(family as u64, 0.0, 0.0, family, Mind::Villager, &mut rng);
        n.traits = [0.5; N_TRAITS];
        n
    }

    #[test]
    fn prejudice_comes_from_feelings_about_the_family() {
        let mut n = npc(0);
        let neutral = attitude(&n, 99, 3);
        n.adjust_family_feeling(3, -0.8); // e.g. after hearing about raids
        assert!(attitude(&n, 99, 3) < neutral - 0.4);
        assert!(attitude(&n, 98, 0) > neutral); // own family is liked
    }

    #[test]
    fn friends_give_food_enemies_do_not() {
        let mut giver = npc(0);
        giver.food = 0.9;
        assert!(answer_ask(&giver, 0.8, 0.5) > 0.0);
        assert_eq!(answer_ask(&giver, -0.8, 0.5), 0.0);
    }

    #[test]
    fn strangers_get_a_worse_rate_and_enemies_none() {
        let mut seller = npc(0);
        seller.carry = Some((Item::Food, 1.0));
        seller.food = 0.5;
        let friend = answer_trade(&seller, 0.5, Item::Wood, 0.5, Item::Food).unwrap();
        let stranger = answer_trade(&seller, -0.2, Item::Wood, 0.5, Item::Food).unwrap();
        assert!(friend > stranger);
        assert!(answer_trade(&seller, -0.9, Item::Wood, 0.5, Item::Food).is_none());
    }

    #[test]
    fn insults_annoy_and_can_make_a_listener_snap() {
        let mut listener = npc(0);
        listener.traits[PATIENCE] = 0.0;
        listener.traits[COURAGE] = 0.9;
        listener.annoyance = 0.4;
        let r = react(&listener, TalkMove::Insult);
        assert!(r.affinity < 0.0 && r.annoyance > 0.0);
        assert!(r.snaps);
    }
}
