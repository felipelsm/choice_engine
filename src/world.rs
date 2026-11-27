use crate::creature::{Action, Creature, BOREDOM, HUNGER, THIRST};
use crate::render::{Color, Renderer};

/// Seconds between decisions.
pub const DECISION_INTERVAL: f32 = 1.0;

// How fast needs grow (per second) and how fast they get satisfied.
const HUNGER_RATE: f32 = 0.04;
const THIRST_RATE: f32 = 0.06;
const BOREDOM_RATE: f32 = 0.05;
const EAT_RATE: f32 = 0.5;
const DRINK_RATE: f32 = 0.6;
const DANCE_RATE: f32 = 0.3;

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
    pub creatures: Vec<Creature>,
    pub resources: Vec<Resource>,
    decision_timer: f32,
    pub ticks: u64,
}

impl World {
    pub fn new(width: f32, height: f32, n_creatures: usize) -> Self {
        let mut resources = Vec::new();
        for _ in 0..4 {
            resources.push(Resource { kind: ResourceKind::Food, x: rand_in(width), y: rand_in(height) });
            resources.push(Resource { kind: ResourceKind::Water, x: rand_in(width), y: rand_in(height) });
        }

        let creatures = (0..n_creatures)
            .map(|id| Creature::new(id, rand_in(width), rand_in(height)))
            .collect();

        Self { creatures, resources, decision_timer: 0.0, ticks: 0 }
    }

    /// Advance the world by `dt` seconds. Returns true on frames where a
    /// decision tick happened.
    pub fn update(&mut self, dt: f32) -> bool {
        // --- decision tick: once every DECISION_INTERVAL seconds ---
        self.decision_timer += dt;
        let mut decided = false;
        while self.decision_timer >= DECISION_INTERVAL {
            self.decision_timer -= DECISION_INTERVAL;
            for c in &mut self.creatures {
                c.decide();
            }
            self.ticks += 1;
            decided = true;
        }

        // --- continuous stuff: needs grow, creatures act on their choice ---
        for c in &mut self.creatures {
            c.state[HUNGER] += HUNGER_RATE * dt;
            c.state[THIRST] += THIRST_RATE * dt;
            c.state[BOREDOM] += BOREDOM_RATE * dt;

            match c.action {
                Action::WalkToFood => seek(c, &self.resources, ResourceKind::Food, HUNGER, EAT_RATE, dt),
                Action::WalkToWater => seek(c, &self.resources, ResourceKind::Water, THIRST, DRINK_RATE, dt),
                Action::Dance => {
                    c.dance_phase += dt * 10.0;
                    c.state[BOREDOM] -= DANCE_RATE * dt;
                }
            }

            for s in &mut c.state {
                *s = s.clamp(0.0, 1.0);
            }
        }
        decided
    }

    pub fn draw(&self, r: &mut dyn Renderer) {
        r.clear(0x2b2b2b);

        for res in &self.resources {
            let color = match res.kind {
                ResourceKind::Food => 0x4caf50,
                ResourceKind::Water => 0x2196f3,
            };
            r.fill_rect(res.x as i32 - 8, res.y as i32 - 8, 16, 16, color);
        }

        for c in &self.creatures {
            let (mut x, y) = (c.x, c.y);
            if c.action == Action::Dance {
                x += c.dance_phase.sin() * 3.0; // wiggle
            }
            r.fill_circle(x as i32, y as i32, 6, action_color(c.action));

            // tiny need bars above the head: hunger, thirst, boredom
            let bar_colors: [Color; 3] = [0x4caf50, 0x2196f3, 0xe040fb];
            for (i, &value) in c.state.iter().enumerate() {
                let bx = x as i32 - 9 + i as i32 * 7;
                let full = 10;
                let h = (value * full as f32) as i32;
                r.fill_rect(bx, y as i32 - 10 - full, 5, full, 0x111111);
                r.fill_rect(bx, y as i32 - 10 - h, 5, h, bar_colors[i]);
            }
        }
    }
}

fn action_color(a: Action) -> Color {
    match a {
        Action::WalkToFood => 0xa5d6a7,
        Action::WalkToWater => 0x90caf9,
        Action::Dance => 0xf48fb1,
    }
}

/// Walk toward the nearest resource of `kind`; when there, reduce `need`.
fn seek(c: &mut Creature, resources: &[Resource], kind: ResourceKind, need: usize, rate: f32, dt: f32) {
    let target = resources
        .iter()
        .filter(|r| r.kind == kind)
        .min_by(|a, b| dist2(c, a).total_cmp(&dist2(c, b)));

    let Some(t) = target else { return };
    let (dx, dy) = (t.x - c.x, t.y - c.y);
    let d = (dx * dx + dy * dy).sqrt();

    if d <= REACH {
        c.state[need] -= rate * dt;
    } else {
        let step = (SPEED * dt).min(d);
        c.x += dx / d * step;
        c.y += dy / d * step;
    }
}

fn dist2(c: &Creature, r: &Resource) -> f32 {
    (r.x - c.x).powi(2) + (r.y - c.y).powi(2)
}

fn rand_in(max: f32) -> f32 {
    20.0 + fastrand::f32() * (max - 40.0)
}
