// Animals: simple creatures with no culture and no goal stack.
//   DEER graze bushes, run from wolves and people, breed when fed.
//   WOLVES hunt deer, and lone people away from shelters (more at night).
// Killing a deer drops meat (a food pile) for the hunter's family; wolves can
// be fought off and killed. Populations rise and fall against each other
// (predator-prey cycles); a few animals wander in from the edges if a species
// dies out, so the ecology doesn't end for good.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Species {
    Deer,
    Wolf,
}

pub struct Animal {
    pub id: u64,
    pub species: Species,
    pub x: f32,
    pub y: f32,
    pub health: f32,
    pub food: f32,
    pub age: f32,
    pub lifespan: f32,
    pub cooldown: f32,
    pub stamina: f32,
    pub target: Option<(f32, f32)>,
    pub killed_by: Option<u64>, // npc id that dealt the last blow
}

pub const DEER_MAX: usize = 50;
pub const WOLF_MAX: usize = 8;
const DEER_SPEED: f32 = 70.0;
const DEER_TIRED_SPEED: f32 = 35.0;
const WOLF_SPEED: f32 = 74.0;
const WANDER_SPEED: f32 = 25.0;
pub const WOLF_BITE: f32 = 0.12; // health per second to a person
const WOLF_BITE_DEER: f32 = 0.5;
const DEER_SIGHT: f32 = 80.0;
const WOLF_SIGHT: f32 = 220.0;
const REACH: f32 = 10.0;

/// What the animals need to know about people.
pub struct PersonView {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub alone: bool,     // nobody else within 50 px
    pub sheltered: bool, // near a shelter
}

pub enum AnimalEvent {
    Bite { npc: u64, wolf: u64, dmg: f32 },
}

impl Animal {
    pub fn new(id: u64, species: Species, x: f32, y: f32, rng: &mut fastrand::Rng) -> Self {
        let (life_min, life_span) = match species {
            Species::Deer => (300.0, 100.0),
            Species::Wolf => (400.0, 200.0),
        };
        Self {
            id,
            species,
            x,
            y,
            health: 1.0,
            food: 0.7,
            age: 0.0,
            lifespan: life_min + rng.f32() * life_span,
            cooldown: 20.0,
            stamina: 1.0,
            target: None,
            killed_by: None,
        }
    }

    pub fn is_dead(&self) -> bool {
        self.health <= 0.0 || self.age >= self.lifespan
    }
}

fn dist(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt()
}

fn walk(a: &mut Animal, tx: f32, ty: f32, speed: f32, dt: f32, w: f32, h: f32) -> bool {
    let (dx, dy) = (tx - a.x, ty - a.y);
    let d = (dx * dx + dy * dy).sqrt();
    if d <= REACH {
        return true;
    }
    let step = (speed * dt).min(d);
    a.x = (a.x + dx / d * step).clamp(5.0, w - 5.0);
    a.y = (a.y + dy / d * step).clamp(5.0, h - 5.0);
    false
}

/// Move and act for `dt` seconds. Bushes are (x, y, amount) and get grazed.
#[allow(clippy::too_many_arguments)]
pub fn step(
    animals: &mut [Animal],
    bushes: &mut [(f32, f32, f32)],
    people: &[PersonView],
    night: bool,
    dt: f32,
    w: f32,
    h: f32,
    rng: &mut fastrand::Rng,
    events: &mut Vec<AnimalEvent>,
) {
    let wolves: Vec<(u64, f32, f32)> =
        animals.iter().filter(|a| a.species == Species::Wolf).map(|a| (a.id, a.x, a.y)).collect();
    let deer: Vec<(usize, f32, f32)> = animals
        .iter()
        .enumerate()
        .filter(|(_, a)| a.species == Species::Deer)
        .map(|(i, a)| (i, a.x, a.y))
        .collect();
    let mut deer_bites: Vec<(usize, f32)> = Vec::new();

    for (idx, a) in animals.iter_mut().enumerate() {
        a.stamina = (a.stamina + 0.1 * dt).min(1.0);
        match a.species {
            Species::Deer => {
                a.food -= 0.02 * dt;
                // nearest danger
                let danger = wolves
                    .iter()
                    .map(|&(_, x, y)| (x, y, dist(a.x, a.y, x, y)))
                    .chain(people.iter().map(|p| (p.x, p.y, dist(a.x, a.y, p.x, p.y) * 1.6)))
                    .filter(|&(_, _, d)| d < DEER_SIGHT)
                    .min_by(|p, q| p.2.total_cmp(&q.2));
                if let Some((tx, ty, _)) = danger {
                    let (dx, dy) = (a.x - tx, a.y - ty);
                    let d = (dx * dx + dy * dy).sqrt().max(1.0);
                    let speed = if a.stamina > 0.0 { DEER_SPEED } else { DEER_TIRED_SPEED };
                    a.stamina -= 0.3 * dt;
                    walk(a, a.x + dx / d * 50.0, a.y + dy / d * 50.0, speed, dt, w, h);
                } else if a.food < 0.7 {
                    // grass is everywhere; bushes are a treat
                    a.food += 0.03 * dt;
                    let near = bushes
                        .iter_mut()
                        .filter(|b| b.2 > 0.2)
                        .min_by(|p, q| dist(a.x, a.y, p.0, p.1).total_cmp(&dist(a.x, a.y, q.0, q.1)));
                    if let Some(b) = near {
                        if walk(a, b.0, b.1, WANDER_SPEED * 1.5, dt, w, h) {
                            let bite = (0.1 * dt).min(b.2);
                            b.2 -= bite;
                            a.food += 2.0 * bite;
                        }
                    }
                } else {
                    a.food += 0.03 * dt;
                    wander(a, dt, w, h, rng);
                }
            }
            Species::Wolf => {
                a.food -= 0.012 * dt;
                // keep hunting until full enough to breed (threshold 0.8)
                if a.food < 0.95 {
                    let range = if night { WOLF_SIGHT * 1.5 } else { WOLF_SIGHT };
                    let prey_deer = deer
                        .iter()
                        .map(|&(i, x, y)| (Some(i), None, x, y, dist(a.x, a.y, x, y)))
                        .filter(|p| p.4 < range);
                    let prey_people = people
                        .iter()
                        .filter(|p| p.alone && !p.sheltered && a.food < 0.5)
                        .map(|p| (None, Some(p.id), p.x, p.y, dist(a.x, a.y, p.x, p.y) * if night { 0.8 } else { 1.3 }))
                        .filter(|p| p.4 < range);
                    let prey = prey_deer.chain(prey_people).min_by(|p, q| p.4.total_cmp(&q.4));
                    if let Some((deer_i, person, tx, ty, _)) = prey {
                        if walk(a, tx, ty, WOLF_SPEED, dt, w, h) {
                            if let Some(i) = deer_i {
                                deer_bites.push((i, WOLF_BITE_DEER * dt));
                                a.food = (a.food + 0.4 * dt).min(1.0);
                            }
                            if let Some(id) = person {
                                events.push(AnimalEvent::Bite { npc: id, wolf: a.id, dmg: WOLF_BITE * dt });
                                a.food = (a.food + 0.1 * dt).min(1.0);
                            }
                        }
                    } else {
                        wander(a, dt, w, h, rng);
                    }
                } else {
                    wander(a, dt, w, h, rng);
                }
            }
        }
        if a.food <= 0.0 {
            a.health -= 0.05 * dt;
        }
        a.food = a.food.clamp(0.0, 1.2);
        let _ = idx;
    }
    for (i, dmg) in deer_bites {
        animals[i].health -= dmg;
    }
}

fn wander(a: &mut Animal, dt: f32, w: f32, h: f32, rng: &mut fastrand::Rng) {
    let t = *a.target.get_or_insert_with(|| (rng.f32() * w, rng.f32() * h));
    if walk(a, t.0, t.1, WANDER_SPEED, dt, w, h) {
        a.target = None;
    }
}

/// Once per tick: aging, breeding, and animals wandering in if a species is gone.
pub fn tick(animals: &mut Vec<Animal>, next_id: &mut u64, ticks: u64, w: f32, h: f32, rng: &mut fastrand::Rng) {
    let n_deer = animals.iter().filter(|a| a.species == Species::Deer).count();
    let n_wolves = animals.len() - n_deer;
    let (mut deer_room, mut wolf_room) = (DEER_MAX.saturating_sub(n_deer), WOLF_MAX.saturating_sub(n_wolves));
    let mut born = Vec::new();
    for a in animals.iter_mut() {
        a.age += 1.0;
        a.cooldown -= 1.0;
        let (room, threshold) = match a.species {
            Species::Deer => (&mut deer_room, 0.75),
            Species::Wolf => (&mut wolf_room, 0.8),
        };
        if *room > 0 && a.age > 30.0 && a.cooldown <= 0.0 && a.food > threshold {
            a.food -= 0.4;
            *room -= 1;
            a.cooldown = if a.species == Species::Deer { 40.0 } else { 80.0 };
            born.push(Animal::new(*next_id, a.species, a.x + 5.0, a.y + 5.0, rng));
            *next_id += 1;
        }
    }
    animals.extend(born);

    // migrants from the edges keep the ecology going
    let edge = |rng: &mut fastrand::Rng| if rng.bool() { (5.0, rng.f32() * h) } else { (rng.f32() * w, 5.0) };
    if n_deer < 3 && ticks % 100 == 0 {
        for _ in 0..2 {
            let (x, y) = edge(rng);
            animals.push(Animal::new(*next_id, Species::Deer, x, y, rng));
            *next_id += 1;
        }
    }
    if n_wolves < 2 && ticks % 300 == 0 {
        // a pair, so they can breed
        let (x, y) = edge(rng);
        for k in 0..2 {
            animals.push(Animal::new(*next_id, Species::Wolf, x + k as f32 * 6.0, y, rng));
            *next_id += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wolves_bite_lone_unsheltered_people_but_not_sheltered_ones() {
        let mut rng = fastrand::Rng::with_seed(1);
        for (sheltered, expect_bite) in [(false, true), (true, false)] {
            let mut animals = vec![Animal::new(1, Species::Wolf, 100.0, 100.0, &mut rng)];
            animals[0].food = 0.3;
            let people = [PersonView { id: 7, x: 105.0, y: 100.0, alone: true, sheltered }];
            let mut events = Vec::new();
            step(&mut animals, &mut [], &people, true, 0.1, 400.0, 400.0, &mut rng, &mut events);
            assert_eq!(!events.is_empty(), expect_bite);
        }
    }

    #[test]
    fn deer_run_from_wolves() {
        let mut rng = fastrand::Rng::with_seed(2);
        let mut animals = vec![
            Animal::new(1, Species::Deer, 100.0, 100.0, &mut rng),
            Animal::new(2, Species::Wolf, 140.0, 100.0, &mut rng),
        ];
        animals[1].food = 1.0; // not hunting, just standing there
        let x0 = animals[0].x;
        step(&mut animals, &mut [], &[], false, 0.5, 400.0, 400.0, &mut rng, &mut Vec::new());
        assert!(animals[0].x < x0);
    }
}
