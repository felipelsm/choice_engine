use crate::creature::{
    random_personality, Action, Creature, Perception, Personality, CARRY_CAP, N_ACTIONS, N_STATES,
};
use crate::render::Renderer;

/// Seconds between decisions.
pub const DECISION_INTERVAL: f32 = 1.0;

// --- body ---
const METABOLISM_EVERY: u64 = 5; // every this many ticks...
const METABOLISM: f32 = 0.10; //    ...lose this much food and water
const EAT_RATE: f32 = 0.25; //      per second
const DRINK_RATE: f32 = 0.25;
const BOREDOM_RATE: f32 = 0.05;
const REST_RATE: f32 = 0.15;
const SHELTER_REST_BONUS: f32 = 2.0; // resting under a shelter is this much faster

// --- health ---
const STARVE_DAMAGE: f32 = 0.05; //     per second while food OR water is empty
const EXHAUSTION_DAMAGE: f32 = 0.03; // per second while boredom is maxed
const COLD_DAMAGE: f32 = 0.02; //       per second at night without shelter
const HEAL_RATE: f32 = 0.02; //         per second while none of the above
const HEAL_ABOVE: f32 = 0.5; //         ...and food and water are above this

// --- food sources ---
const WILD_SOURCES: usize = 6;
const WILD_MAX: f32 = 8.0; //     a full wild bush feeds 8 full stomachs
const WILD_REGROW: f32 = 0.2; //  per second
const FARM_MAX: f32 = 4.0;
const FARM_REGROW: f32 = 0.1;
const FARM_START: f32 = 0.5;
const MAX_FOOD_SOURCES: usize = 40;
const WATER_SOURCES: usize = 4; // water never runs out

// --- settlement ---
const GATHER_RATE: f32 = 0.4; //    food per second into your hands
const BUILD_RATE: f32 = 0.2; //     food per second spent building
const SHELTER_COST: f32 = 2.0; //   food to finish a shelter
const SHELTER_RADIUS: f32 = 50.0;
const FARM_COST: f32 = 0.2; //      carried food spent per farm planted
const JOIN_RADIUS: f32 = 100.0; //  a new home this close to a village joins it

// --- social ---
const NEAR: f32 = 40.0; //   "next to me" radius
const VIEW: f32 = 150.0; //  how far creatures can see family
const SHARE_AMOUNT: f32 = 0.15;
const STEAL_AMOUNT: f32 = 0.2;
const STEAL_SELF_DAMAGE: f32 = 0.03;
const STEAL_VICTIM_DAMAGE: f32 = 0.06;
const LONELY_RATE: f32 = 0.03; //  per second alone
const UNLONELY_RATE: f32 = 0.10; // per second near others
const FEAR_DECAY: f32 = 0.05; //   per second

// --- time ---
pub const DAY_LENGTH: u64 = 60; // ticks per day
const NIGHT_START: u64 = 36; //    ticks into the day when night begins

/// Safety cap. Food is what should limit population now.
pub const MAX_POPULATION: usize = 1000;

const SPEED: f32 = 60.0; // pixels per second
const REACH: f32 = 12.0; // close enough to use something
const HOME_REACH: f32 = 20.0;
const FAR: f32 = 400.0; //  distance that counts as "1.0 far" in perception

pub struct FoodSource {
    pub x: f32,
    pub y: f32,
    pub amount: f32,
    pub max: f32,
    pub regrow: f32,
    pub farm: bool,
}

pub struct Water {
    pub x: f32,
    pub y: f32,
}

pub struct Settlement {
    pub x: f32,
    pub y: f32,
    pub stock: f32,   // stored food
    pub shelter: f32, // build progress, 1.0 = finished
}

impl Settlement {
    pub fn has_shelter(&self) -> bool {
        self.shelter >= 1.0
    }
}

/// Per-creature info gathered at the start of a tick, used for feasibility
/// and for share / steal / follow targets.
#[derive(Default)]
struct Senses {
    share_target: Option<usize>,
    steal_target: Option<usize>,
    kin_center: Option<(f32, f32)>,
}

enum Interaction {
    Share { from: usize, to: usize },
    Steal { thief: usize, victim: usize },
}

/// Why creatures died, for stats.
#[derive(Clone, Copy)]
pub enum DeathCause {
    OldAge,
    Starved,
    Exhausted,
    Cold,
    Fight,
}

pub const DEATH_CAUSES: [(DeathCause, &str); 5] = [
    (DeathCause::OldAge, "AGE"),
    (DeathCause::Starved, "STARVE"),
    (DeathCause::Exhausted, "TIRED"),
    (DeathCause::Cold, "COLD"),
    (DeathCause::Fight, "FIGHT"),
];

pub struct World {
    pub width: f32,
    pub height: f32,
    pub creatures: Vec<Creature>,
    pub food: Vec<FoodSource>,
    pub water: Vec<Water>,
    pub settlements: Vec<Settlement>,
    pub ticks: u64,
    pub births: usize,
    pub deaths: usize,
    pub deaths_by: [usize; 5], // indexed like DEATH_CAUSES
    pub history: Vec<usize>, // population at every tick
    decision_timer: f32,
}

impl World {
    pub fn new(width: f32, height: f32, n_creatures: usize) -> Self {
        let food = (0..WILD_SOURCES)
            .map(|_| FoodSource {
                x: rand_in(width),
                y: rand_in(height),
                amount: WILD_MAX,
                max: WILD_MAX,
                regrow: WILD_REGROW,
                farm: false,
            })
            .collect();
        let water = (0..WATER_SOURCES).map(|_| Water { x: rand_in(width), y: rand_in(height) }).collect();

        // every founder starts its own family
        let creatures = (0..n_creatures)
            .map(|i| Creature::new(rand_in(width), rand_in(height), i as u32, random_personality()))
            .collect();

        Self {
            width,
            height,
            creatures,
            food,
            water,
            settlements: Vec::new(),
            ticks: 0,
            births: 0,
            deaths: 0,
            deaths_by: [0; 5],
            history: vec![n_creatures],
            decision_timer: 0.0,
        }
    }

    pub fn is_night(&self) -> bool {
        self.ticks % DAY_LENGTH >= NIGHT_START
    }

    pub fn day(&self) -> u64 {
        self.ticks / DAY_LENGTH + 1
    }

    /// Advance the world by `dt` seconds. Returns true if a decision tick happened.
    pub fn update(&mut self, dt: f32) -> bool {
        self.decision_timer += dt;
        let mut ticked = false;
        while self.decision_timer >= DECISION_INTERVAL {
            self.decision_timer -= DECISION_INTERVAL;
            self.tick();
            ticked = true;
        }
        self.step(dt);
        ticked
    }

    // -----------------------------------------------------------------------
    // Decision tick: perceive, decide, do instant actions
    // -----------------------------------------------------------------------

    fn tick(&mut self) {
        self.ticks += 1;

        for c in &mut self.creatures {
            c.age += 1.0;
            c.cooldown -= 1.0;
            if self.ticks % METABOLISM_EVERY == 0 {
                c.food = (c.food - METABOLISM).max(0.0);
                c.water = (c.water - METABOLISM).max(0.0);
            }
        }

        let senses = self.perceive();

        let population = self.creatures.len();
        let mut babies = Vec::new();
        let mut interactions = Vec::new();

        for i in 0..self.creatures.len() {
            let possible = self.possible_actions(i, &senses[i]);
            self.creatures[i].decide(&possible);

            match self.creatures[i].action {
                Action::Reproduce => {
                    if population + babies.len() < MAX_POPULATION {
                        babies.push(self.creatures[i].reproduce());
                    }
                }
                Action::Share => {
                    if let Some(to) = senses[i].share_target {
                        interactions.push(Interaction::Share { from: i, to });
                    }
                }
                Action::Steal => {
                    if let Some(victim) = senses[i].steal_target {
                        interactions.push(Interaction::Steal { thief: i, victim });
                    }
                }
                Action::Explore => {
                    let c = &self.creatures[i];
                    let tx = (c.x + fastrand::f32() * 300.0 - 150.0).clamp(10.0, self.width - 10.0);
                    let ty = (c.y + fastrand::f32() * 300.0 - 150.0).clamp(10.0, self.height - 10.0);
                    self.creatures[i].target = Some((tx, ty));
                }
                Action::Follow => self.creatures[i].target = senses[i].kin_center,
                Action::Flee => {
                    let c = &self.creatures[i];
                    if let Some((tx, ty)) = c.threat {
                        let (dx, dy) = (c.x - tx, c.y - ty);
                        let d = (dx * dx + dy * dy).sqrt().max(1.0);
                        let fx = (c.x + dx / d * 120.0).clamp(10.0, self.width - 10.0);
                        let fy = (c.y + dy / d * 120.0).clamp(10.0, self.height - 10.0);
                        self.creatures[i].target = Some((fx, fy));
                    }
                }
                Action::Store | Action::Build | Action::Farm => self.ensure_home(i),
                _ => {}
            }
        }

        for it in interactions {
            match it {
                Interaction::Share { from, to } => self.share(from, to),
                Interaction::Steal { thief, victim } => self.steal(thief, victim),
            }
        }

        self.births += babies.len();
        self.creatures.extend(babies);
        self.history.push(self.creatures.len());
    }

    /// Fill in each creature's Perception and collect targets. O(n^2), which
    /// is fine for a few hundred creatures; use a grid if it gets slow.
    fn perceive(&mut self) -> Vec<Senses> {
        let n = self.creatures.len();
        let mut senses: Vec<Senses> = (0..n).map(|_| Senses::default()).collect();
        let night = self.is_night();

        for i in 0..n {
            let c = &self.creatures[i];
            let (mut near, mut kin_near) = (0usize, 0usize);
            let (mut kin_sum_x, mut kin_sum_y, mut kin_seen) = (0.0, 0.0, 0usize);
            let mut hungriest: Option<(usize, f32)> = None;
            let mut nearest_stranger: Option<(usize, f32)> = None;

            for j in 0..n {
                if i == j {
                    continue;
                }
                let o = &self.creatures[j];
                let d = dist(c.x, c.y, o.x, o.y);
                let kin = o.family == c.family;
                if d <= NEAR {
                    near += 1;
                    if kin {
                        kin_near += 1;
                    }
                    if hungriest.is_none_or(|(_, f)| o.food < f) {
                        hungriest = Some((j, o.food));
                    }
                    let has_food = o.carry > 0.05 || o.food > 0.2;
                    if !kin && has_food && nearest_stranger.is_none_or(|(_, nd)| d < nd) {
                        nearest_stranger = Some((j, d));
                    }
                }
                if kin && d <= VIEW {
                    kin_sum_x += o.x;
                    kin_sum_y += o.y;
                    kin_seen += 1;
                }
            }

            let kin_center = (kin_seen > 0)
                .then(|| (kin_sum_x / kin_seen as f32, kin_sum_y / kin_seen as f32))
                .filter(|&(kx, ky)| dist(c.x, c.y, kx, ky) > 15.0);

            let food_far = self
                .food
                .iter()
                .filter(|f| f.amount > 0.5)
                .map(|f| dist(c.x, c.y, f.x, f.y))
                .fold(f32::INFINITY, f32::min);

            let home_far = c
                .home
                .map(|h| dist(c.x, c.y, self.settlements[h].x, self.settlements[h].y) / FAR)
                .unwrap_or(1.0);

            let cold = night && !self.sheltered(c.x, c.y);

            senses[i] = Senses {
                share_target: hungriest.map(|(j, _)| j),
                steal_target: nearest_stranger.map(|(j, _)| j),
                kin_center,
            };

            let c = &mut self.creatures[i];
            c.perception = Perception {
                food_far: (food_far / FAR).min(1.0),
                crowding: (near as f32 / 8.0).min(1.0),
                kin_near: (kin_near as f32 / 5.0).min(1.0),
                home_far: home_far.min(1.0),
                cold: if cold { 1.0 } else { 0.0 },
            };
        }
        senses
    }

    fn possible_actions(&self, i: usize, s: &Senses) -> [bool; N_ACTIONS] {
        let c = &self.creatures[i];
        let any_food = self.food.iter().any(|f| f.amount > 0.05);
        let home_stock = c.home.is_some_and(|h| self.settlements[h].stock > 0.05);
        let home_shelter_done = c.home.is_some_and(|h| self.settlements[h].has_shelter());

        let mut p = [false; N_ACTIONS];
        p[Action::Eat.index()] = c.carry > 0.01 || any_food || home_stock;
        p[Action::Drink.index()] = true;
        p[Action::Rest.index()] = true;
        p[Action::Reproduce.index()] = c.can_reproduce();
        p[Action::Explore.index()] = true;
        p[Action::Follow.index()] = s.kin_center.is_some();
        p[Action::Share.index()] = s.share_target.is_some() && (c.carry >= SHARE_AMOUNT || c.food > 0.6);
        p[Action::Steal.index()] = s.steal_target.is_some();
        p[Action::Flee.index()] = c.threat.is_some() && c.fear > 0.05;
        p[Action::Gather.index()] = any_food && c.carry < CARRY_CAP - 0.05;
        p[Action::Store.index()] = c.carry > 0.05;
        p[Action::Build.index()] = c.carry > 0.05 && !home_shelter_done;
        p[Action::Farm.index()] = c.carry >= FARM_COST && self.food.len() < MAX_FOOD_SOURCES;
        p
    }

    /// Give creature `i` a home: join a village within JOIN_RADIUS, or found one here.
    fn ensure_home(&mut self, i: usize) {
        if self.creatures[i].home.is_some() {
            return;
        }
        let (x, y) = (self.creatures[i].x, self.creatures[i].y);
        let nearby = self
            .settlements
            .iter()
            .enumerate()
            .map(|(k, s)| (k, dist(x, y, s.x, s.y)))
            .filter(|&(_, d)| d <= JOIN_RADIUS)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let home = match nearby {
            Some((k, _)) => k,
            None => {
                self.settlements.push(Settlement { x, y, stock: 0.0, shelter: 0.0 });
                self.settlements.len() - 1
            }
        };
        self.creatures[i].home = Some(home);
    }

    fn share(&mut self, from: usize, to: usize) {
        let giver = &mut self.creatures[from];
        let amount = if giver.carry >= SHARE_AMOUNT {
            giver.carry -= SHARE_AMOUNT;
            SHARE_AMOUNT
        } else if giver.food > 0.6 {
            giver.food -= SHARE_AMOUNT;
            SHARE_AMOUNT
        } else {
            return;
        };
        feed(&mut self.creatures[to], amount);
    }

    fn steal(&mut self, thief: usize, victim: usize) {
        let (tx, ty) = (self.creatures[thief].x, self.creatures[thief].y);
        let v = &mut self.creatures[victim];
        let taken = if v.carry > 0.05 {
            let t = v.carry.min(STEAL_AMOUNT);
            v.carry -= t;
            t
        } else {
            let t = v.food.min(STEAL_AMOUNT);
            v.food -= t;
            t
        };
        v.health -= STEAL_VICTIM_DAMAGE;
        v.fear = 1.0;
        v.threat = Some((tx, ty));

        let t = &mut self.creatures[thief];
        t.health -= STEAL_SELF_DAMAGE;
        feed(t, taken);
    }

    fn sheltered(&self, x: f32, y: f32) -> bool {
        self.settlements
            .iter()
            .any(|s| s.has_shelter() && dist(x, y, s.x, s.y) <= SHELTER_RADIUS)
    }

    // -----------------------------------------------------------------------
    // Continuous step: move, eat, drink, carry, build, heal, die
    // -----------------------------------------------------------------------

    fn step(&mut self, dt: f32) {
        let night = self.is_night();

        for f in &mut self.food {
            f.amount = (f.amount + f.regrow * dt).min(f.max);
        }

        // shelter check needs settlements while creatures are borrowed mutably
        let shelters: Vec<(f32, f32)> = self
            .settlements
            .iter()
            .filter(|s| s.has_shelter())
            .map(|s| (s.x, s.y))
            .collect();

        let World { creatures, food, water, settlements, .. } = self;

        for c in creatures.iter_mut() {
            let sheltered = shelters.iter().any(|&(sx, sy)| dist(c.x, c.y, sx, sy) <= SHELTER_RADIUS);
            c.boredom += BOREDOM_RATE * dt;

            match c.action {
                Action::Eat => {
                    if c.carry > 0.01 {
                        let bite = (EAT_RATE * dt).min(c.carry).min(1.0 - c.food);
                        c.carry -= bite;
                        c.food += bite;
                    } else {
                        eat_from_world(c, food, settlements, dt);
                    }
                }
                Action::Drink => {
                    if let Some(w) = water.iter().min_by(|a, b| {
                        dist(c.x, c.y, a.x, a.y).total_cmp(&dist(c.x, c.y, b.x, b.y))
                    }) {
                        if walk(c, w.x, w.y, REACH, dt) {
                            c.water += DRINK_RATE * dt;
                        }
                    }
                }
                Action::Rest => {
                    let bonus = if sheltered { SHELTER_REST_BONUS } else { 1.0 };
                    c.boredom -= REST_RATE * bonus * dt;
                }
                Action::Explore | Action::Follow | Action::Flee => {
                    if let Some((tx, ty)) = c.target {
                        walk(c, tx, ty, 4.0, dt);
                    }
                }
                Action::Gather => {
                    if let Some(k) = nearest_food(c, food) {
                        let f = &mut food[k];
                        if walk(c, f.x, f.y, REACH, dt) {
                            let take = (GATHER_RATE * dt).min(f.amount).min(CARRY_CAP - c.carry);
                            f.amount -= take;
                            c.carry += take;
                        }
                    }
                }
                Action::Store => {
                    if let Some(h) = c.home {
                        let s = &mut settlements[h];
                        if walk(c, s.x, s.y, HOME_REACH, dt) {
                            s.stock += c.carry;
                            c.carry = 0.0;
                        }
                    }
                }
                Action::Build => {
                    if let Some(h) = c.home {
                        let s = &mut settlements[h];
                        if walk(c, s.x, s.y, HOME_REACH, dt) && !s.has_shelter() {
                            let spend = (BUILD_RATE * dt).min(c.carry);
                            c.carry -= spend;
                            s.shelter = (s.shelter + spend / SHELTER_COST).min(1.0);
                        }
                    }
                }
                Action::Farm => {
                    if let Some(h) = c.home {
                        let (hx, hy) = (settlements[h].x, settlements[h].y);
                        if walk(c, hx, hy, HOME_REACH, dt) && c.carry >= FARM_COST && food.len() < MAX_FOOD_SOURCES {
                            c.carry -= FARM_COST;
                            let angle = fastrand::f32() * std::f32::consts::TAU;
                            let r = 25.0 + fastrand::f32() * 30.0;
                            food.push(FoodSource {
                                x: hx + angle.cos() * r,
                                y: hy + angle.sin() * r,
                                amount: FARM_START,
                                max: FARM_MAX,
                                regrow: FARM_REGROW,
                                farm: true,
                            });
                        }
                    }
                }
                Action::Reproduce | Action::Share | Action::Steal => {} // instant, done at the tick
            }

            // social feelings
            if c.perception.crowding > 0.0 {
                c.loneliness -= UNLONELY_RATE * dt;
            } else {
                c.loneliness += LONELY_RATE * dt;
            }
            c.fear -= FEAR_DECAY * dt;
            if c.fear <= 0.0 {
                c.threat = None;
            }

            c.food = c.food.clamp(0.0, 1.0);
            c.water = c.water.clamp(0.0, 1.0);
            c.boredom = c.boredom.clamp(0.0, 1.0);
            c.loneliness = c.loneliness.clamp(0.0, 1.0);
            c.fear = c.fear.clamp(0.0, 1.0);

            // health
            let starving = c.food <= 0.0 || c.water <= 0.0;
            let exhausted = c.boredom >= 1.0;
            let cold = night && !sheltered;
            if starving {
                c.health -= STARVE_DAMAGE * dt;
            }
            if exhausted {
                c.health -= EXHAUSTION_DAMAGE * dt;
            }
            if cold {
                c.health -= COLD_DAMAGE * dt;
            }
            if !starving && !exhausted && !cold && c.food > HEAL_ABOVE && c.water > HEAL_ABOVE {
                c.health += HEAL_RATE * dt;
            }
            c.health = c.health.min(1.0);
        }

        let night = self.is_night();
        let mut by = self.deaths_by;
        self.creatures.retain(|c| {
            if !c.is_dead() {
                return true;
            }
            let cause = if c.age >= c.lifespan {
                DeathCause::OldAge
            } else if c.food <= 0.0 || c.water <= 0.0 {
                DeathCause::Starved
            } else if c.boredom >= 1.0 {
                DeathCause::Exhausted
            } else if night && c.perception.cold > 0.0 {
                DeathCause::Cold
            } else {
                DeathCause::Fight
            };
            by[cause as usize] += 1;
            false
        });
        self.deaths = by.iter().sum();
        self.deaths_by = by;
    }

    // -----------------------------------------------------------------------
    // Stats
    // -----------------------------------------------------------------------

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

    /// How many creatures are doing each action right now.
    pub fn action_counts(&self) -> [usize; N_ACTIONS] {
        let mut counts = [0; N_ACTIONS];
        for c in &self.creatures {
            counts[c.action.index()] += 1;
        }
        counts
    }

    pub fn total_stock(&self) -> f32 {
        self.settlements.iter().map(|s| s.stock).sum()
    }

    pub fn shelters_built(&self) -> usize {
        self.settlements.iter().filter(|s| s.has_shelter()).count()
    }

    pub fn farms(&self) -> usize {
        self.food.iter().filter(|f| f.farm).count()
    }

    /// Settlements that at least one living creature calls home.
    pub fn active_villages(&self) -> usize {
        let mut used = vec![false; self.settlements.len()];
        for c in &self.creatures {
            if let Some(h) = c.home {
                used[h] = true;
            }
        }
        used.iter().filter(|&&u| u).count()
    }

    // -----------------------------------------------------------------------
    // Drawing
    // -----------------------------------------------------------------------

    pub fn draw(&self, r: &mut dyn Renderer) {
        let bg = if self.is_night() { 0x15151f } else { 0x2b2b2b };
        r.fill_rect(0, 0, self.width as i32, self.height as i32, bg);

        for s in &self.settlements {
            let (x, y) = (s.x as i32, s.y as i32);
            if s.has_shelter() {
                r.fill_rect(x - 14, y - 6, 28, 16, 0x8d6e63); // walls
                r.fill_rect(x - 17, y - 12, 34, 7, 0x5d4037); // roof
            } else {
                // outline + build progress
                r.fill_rect(x - 14, y - 6, 28, 1, 0x8d6e63);
                r.fill_rect(x - 14, y + 9, 28, 1, 0x8d6e63);
                r.fill_rect(x - 14, y - 6, 1, 16, 0x8d6e63);
                r.fill_rect(x + 13, y - 6, 1, 16, 0x8d6e63);
                r.fill_rect(x - 14, y + 12, (28.0 * s.shelter) as i32, 3, 0x8d6e63);
            }
            // stockpile: grows with stored food
            let size = 4 + (s.stock.min(10.0) * 1.2) as i32;
            r.fill_rect(x + 16, y + 10 - size, size, size, 0xffb74d);
        }

        for w in &self.water {
            r.fill_rect(w.x as i32 - 9, w.y as i32 - 9, 18, 18, 0x2196f3);
        }

        for f in &self.food {
            let half = 2 + (6.0 * f.amount / f.max) as i32;
            let color = if f.farm { 0xc0ca33 } else { 0x4caf50 };
            r.fill_rect(f.x as i32 - half, f.y as i32 - half, half * 2, half * 2, color);
        }

        for c in &self.creatures {
            let (x, y) = (c.x as i32, c.y as i32);
            let radius = if c.is_adult() { 6 } else { 4 };

            // body = personality color, center dot = current action
            r.fill_circle(x, y, radius, c.color());
            r.fill_circle(x, y, 2, action_color(c.action));
            if c.carry > 0.05 {
                r.fill_rect(x + radius, y - 1, 3, 3, 0xffb74d);
            }

            // bars above the head: food, water, health (full = good), boredom (full = bad)
            let bars = [(c.food, 0x4caf50), (c.water, 0x2196f3), (c.health, 0xf44336), (c.boredom, 0xe040fb)];
            for (i, &(value, color)) in bars.iter().enumerate() {
                let bx = x - 9 + i as i32 * 5;
                let full = 8;
                let h = (value * full as f32) as i32;
                r.fill_rect(bx, y - 9 - full, 3, full, 0x111111);
                r.fill_rect(bx, y - 9 - h, 3, h, color);
            }
        }
    }
}

pub fn action_color(a: Action) -> u32 {
    match a {
        Action::Eat => 0x4caf50,
        Action::Drink => 0x2196f3,
        Action::Rest => 0xffffff,
        Action::Reproduce => 0xffeb3b,
        Action::Explore => 0x9e9e9e,
        Action::Follow => 0x00bcd4,
        Action::Share => 0x8bc34a,
        Action::Steal => 0xf44336,
        Action::Flee => 0xff9800,
        Action::Gather => 0xcddc39,
        Action::Store => 0xffb74d,
        Action::Build => 0xa1887f,
        Action::Farm => 0x009688,
    }
}

/// Eat from the nearest food source or the home stockpile, whichever is closer.
fn eat_from_world(c: &mut Creature, food: &mut [FoodSource], settlements: &mut [Settlement], dt: f32) {
    let src = nearest_food(c, food).map(|k| (k, dist(c.x, c.y, food[k].x, food[k].y)));
    let home = c
        .home
        .filter(|&h| settlements[h].stock > 0.05)
        .map(|h| (h, dist(c.x, c.y, settlements[h].x, settlements[h].y)));

    let use_home = match (src, home) {
        (Some((_, ds)), Some((_, dh))) => dh <= ds,
        (None, Some(_)) => true,
        _ => false,
    };

    if use_home {
        let s = &mut settlements[home.unwrap().0];
        if walk(c, s.x, s.y, HOME_REACH, dt) {
            let bite = (EAT_RATE * dt).min(s.stock).min(1.0 - c.food);
            s.stock -= bite;
            c.food += bite;
        }
    } else if let Some((k, _)) = src {
        let f = &mut food[k];
        if walk(c, f.x, f.y, REACH, dt) {
            let bite = (EAT_RATE * dt).min(f.amount).min(1.0 - c.food);
            f.amount -= bite;
            c.food += bite;
        }
    }
}

fn nearest_food(c: &Creature, food: &[FoodSource]) -> Option<usize> {
    food.iter()
        .enumerate()
        .filter(|(_, f)| f.amount > 0.05)
        .min_by(|(_, a), (_, b)| dist(c.x, c.y, a.x, a.y).total_cmp(&dist(c.x, c.y, b.x, b.y)))
        .map(|(k, _)| k)
}

/// Food goes into the stomach; whatever doesn't fit goes into the hands.
fn feed(c: &mut Creature, amount: f32) {
    let room = 1.0 - c.food;
    let eaten = amount.min(room);
    c.food += eaten;
    c.carry = (c.carry + amount - eaten).min(CARRY_CAP);
}

/// Move toward (tx, ty). Returns true once within `reach`.
fn walk(c: &mut Creature, tx: f32, ty: f32, reach: f32, dt: f32) -> bool {
    let (dx, dy) = (tx - c.x, ty - c.y);
    let d = (dx * dx + dy * dy).sqrt();
    if d <= reach {
        return true;
    }
    let step = (SPEED * dt).min(d);
    c.x += dx / d * step;
    c.y += dy / d * step;
    false
}

fn dist(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt()
}

fn rand_in(max: f32) -> f32 {
    30.0 + fastrand::f32() * (max - 60.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn food_sources_run_out_and_regrow() {
        let mut w = World::new(400.0, 400.0, 0);
        w.food.truncate(1);
        w.food[0].amount = 0.0;
        for _ in 0..60 {
            w.step(1.0 / 60.0);
        }
        assert!((w.food[0].amount - WILD_REGROW).abs() < 0.01);
    }

    #[test]
    fn storing_founds_a_village_and_fills_the_stockpile() {
        let mut w = World::new(400.0, 400.0, 1);
        w.creatures[0].carry = 0.5;
        w.ensure_home(0);
        assert_eq!(w.settlements.len(), 1);
        w.creatures[0].action = Action::Store;
        w.step(0.1);
        assert!((w.settlements[0].stock - 0.5).abs() < 1e-5);
        assert_eq!(w.creatures[0].carry, 0.0);
    }

    #[test]
    fn night_is_cold_without_shelter() {
        let mut w = World::new(400.0, 400.0, 1);
        w.ticks = NIGHT_START;
        assert!(w.is_night());
        let c = &mut w.creatures[0];
        c.food = 1.0;
        c.water = 1.0;
        c.boredom = 0.0;
        c.health = 1.0;
        c.action = Action::Rest;
        w.step(1.0);
        assert!(w.creatures[0].health < 1.0);
    }

    #[test]
    fn stealing_moves_food_and_hurts_both() {
        let mut w = World::new(400.0, 400.0, 2);
        w.creatures[0].food = 0.2;
        w.creatures[1].carry = 0.5;
        w.steal(0, 1);
        assert!((w.creatures[1].carry - 0.3).abs() < 1e-5);
        assert!(w.creatures[0].food > 0.2);
        assert!(w.creatures[0].health < 1.0 && w.creatures[1].health < 1.0);
        assert!(w.creatures[1].fear > 0.9);
    }
}
