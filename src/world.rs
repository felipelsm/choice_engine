use crate::creature::{random_personality, Action, Creature, Personality, N_ACTIONS, N_STATES};
use crate::render::Renderer;

/// Seconds between decisions.
pub const DECISION_INTERVAL: f32 = 1.0;

/// Every this many ticks, every creature loses METABOLISM of food and water.
const METABOLISM_EVERY: u64 = 5;
const METABOLISM: f32 = 0.10; // 10% of a full stomach

const EAT_RATE: f32 = 0.25; // food gained per second at a food source
const DRINK_RATE: f32 = 0.25; // water gained per second at a water source
const BOREDOM_RATE: f32 = 0.10; // boredom gained per second
const REST_RATE: f32 = 0.15; // boredom lost per second while resting

const STARVE_DAMAGE: f32 = 0.05; // health lost per second while food OR water is empty
const HEAL_RATE: f32 = 0.02; // health gained per second while food AND water are above HEAL_ABOVE
const HEAL_ABOVE: f32 = 0.5;

/// Births stop at this many creatures. Food and water never run out yet, so
/// without a cap the population doubles roughly every 10 ticks.
pub const MAX_POPULATION: usize = 500;

const SPEED: f32 = 60.0; // pixels per second
const REACH: f32 = 12.0; // close enough to eat/drink

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Food,
    Water,
}

pub struct Resource {
    pub kind: ResourceKind,
    pub x: f32,
    pub y: f32,
}

pub struct World {
    pub width: f32,
    pub height: f32,
    pub creatures: Vec<Creature>,
    pub resources: Vec<Resource>,
    pub ticks: u64,
    pub births: usize,
    pub deaths: usize,
    decision_timer: f32,
}

impl World {
    pub fn new(width: f32, height: f32, n_creatures: usize) -> Self {
        let mut resources = Vec::new();
        for _ in 0..4 {
            resources.push(Resource { kind: ResourceKind::Food, x: rand_in(width), y: rand_in(height) });
            resources.push(Resource { kind: ResourceKind::Water, x: rand_in(width), y: rand_in(height) });
        }

        let creatures = (0..n_creatures)
            .map(|_| Creature::new(rand_in(width), rand_in(height), random_personality()))
            .collect();

        Self {
            width,
            height,
            creatures,
            resources,
            ticks: 0,
            births: 0,
            deaths: 0,
            decision_timer: 0.0,
        }
    }

    /// Advance the world by `dt` seconds. Returns true on frames where a
    /// decision tick happened.
    pub fn update(&mut self, dt: f32) -> bool {
        // --- decision tick: once every DECISION_INTERVAL seconds ---
        self.decision_timer += dt;
        let mut ticked = false;
        while self.decision_timer >= DECISION_INTERVAL {
            self.decision_timer -= DECISION_INTERVAL;
            self.ticks += 1;
            ticked = true;

            if self.ticks % METABOLISM_EVERY == 0 {
                for c in &mut self.creatures {
                    c.food -= METABOLISM;
                    c.water -= METABOLISM;
                }
            }
            // decide, and reproduce right away if that's the choice
            let population = self.creatures.len();
            let mut babies = Vec::new();
            for c in &mut self.creatures {
                c.decide();
                if c.action == Action::Reproduce
                    && c.can_reproduce()
                    && population + babies.len() < MAX_POPULATION
                {
                    babies.push(c.reproduce());
                }
            }
            self.births += babies.len();
            self.creatures.extend(babies);
        }

        // --- continuous stuff: act on the current choice ---
        for c in &mut self.creatures {
            c.boredom += BOREDOM_RATE * dt;

            match c.action {
                Action::WalkToFood => {
                    if walk_to(c, &self.resources, ResourceKind::Food, dt) {
                        c.food += EAT_RATE * dt;
                    }
                }
                Action::WalkToWater => {
                    if walk_to(c, &self.resources, ResourceKind::Water, dt) {
                        c.water += DRINK_RATE * dt;
                    }
                }
                Action::Rest => c.boredom -= REST_RATE * dt,
                Action::Reproduce => {} // happens once, at the decision tick
            }

            c.food = c.food.clamp(0.0, 1.0);
            c.water = c.water.clamp(0.0, 1.0);
            c.boredom = c.boredom.clamp(0.0, 1.0);

            // health: hurt while starving or dehydrated, heal while well fed
            if c.food <= 0.0 || c.water <= 0.0 {
                c.health -= STARVE_DAMAGE * dt;
            } else if c.food > HEAL_ABOVE && c.water > HEAL_ABOVE {
                c.health += HEAL_RATE * dt;
            }
            c.health = c.health.min(1.0);
        }

        // --- deaths ---
        let before = self.creatures.len();
        self.creatures.retain(|c| !c.is_dead());
        self.deaths += before - self.creatures.len();

        ticked
    }

    /// Mean of every living creature's personality matrix.
    pub fn average_personality(&self) -> Option<Personality> {
        if self.creatures.is_empty() {
            return None;
        }
        let mut avg = [[0.0; N_STATES]; N_ACTIONS];
        for c in &self.creatures {
            for row in 0..N_ACTIONS {
                for col in 0..N_STATES {
                    avg[row][col] += c.personality[row][col];
                }
            }
        }
        let n = self.creatures.len() as f32;
        for row in avg.iter_mut() {
            for v in row.iter_mut() {
                *v /= n;
            }
        }
        Some(avg)
    }

    pub fn draw(&self, r: &mut dyn Renderer) {
        r.fill_rect(0, 0, self.width as i32, self.height as i32, 0x2b2b2b);

        for res in &self.resources {
            let color = match res.kind {
                ResourceKind::Food => 0x4caf50,
                ResourceKind::Water => 0x2196f3,
            };
            r.fill_rect(res.x as i32 - 8, res.y as i32 - 8, 16, 16, color);
        }

        for c in &self.creatures {
            let (x, y) = (c.x as i32, c.y as i32);

            // body = personality color, center dot = current action
            r.fill_circle(x, y, 6, c.color());
            r.fill_circle(x, y, 2, action_color(c.action));

            // bars above the head: food, water, health (full = good), boredom (full = bad)
            let bars = [(c.food, 0x4caf50), (c.water, 0x2196f3), (c.health, 0xf44336), (c.boredom, 0xe040fb)];
            for (i, &(value, color)) in bars.iter().enumerate() {
                let bx = x - 13 + i as i32 * 7;
                let full = 10;
                let h = (value * full as f32) as i32;
                r.fill_rect(bx, y - 10 - full, 5, full, 0x111111);
                r.fill_rect(bx, y - 10 - h, 5, h, color);
            }
        }
    }
}

pub fn action_color(a: Action) -> u32 {
    match a {
        Action::WalkToFood => 0x4caf50,
        Action::WalkToWater => 0x2196f3,
        Action::Rest => 0xffffff,
        Action::Reproduce => 0xffeb3b,
    }
}

/// Walk toward the nearest resource of `kind`. Returns true once we're there.
fn walk_to(c: &mut Creature, resources: &[Resource], kind: ResourceKind, dt: f32) -> bool {
    let target = resources
        .iter()
        .filter(|r| r.kind == kind)
        .min_by(|a, b| dist2(c, a).total_cmp(&dist2(c, b)));

    let Some(t) = target else { return false };
    let (dx, dy) = (t.x - c.x, t.y - c.y);
    let d = (dx * dx + dy * dy).sqrt();

    if d <= REACH {
        true
    } else {
        let step = (SPEED * dt).min(d);
        c.x += dx / d * step;
        c.y += dy / d * step;
        false
    }
}

fn dist2(c: &Creature, r: &Resource) -> f32 {
    (r.x - c.x).powi(2) + (r.y - c.y).powi(2)
}

fn rand_in(max: f32) -> f32 {
    20.0 + fastrand::f32() * (max - 40.0)
}
