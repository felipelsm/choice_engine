// Side panel, sim controls and drawing.
//
// Tabs (click or press Tab):
//   STATS      a clicked NPC explains itself (goal, method, why, traits,
//              feelings, legends), or population averages
//   EVOLUTION  families over time, trait drift, goal-gene change since tick 0
//   CULTURE    a family's know-how (methods) and the discovery log
//   HISTORY    leaders, the chronicle (what really happened), and what a
//              clicked NPC believes happened
// Click an NPC to inspect it; click empty ground to clear.

use crate::chronicle::{Belief, EventKind, Subject};
use crate::culture::{How, DISCOVERIES};
use crate::ecology::Species;
use crate::font::{draw_text, text_width, GLYPH_H};
use crate::goals::{Goal, GOALS, N_GOALS};
use crate::methods::{Item, Source};
use crate::npc::*;
use crate::render::{Color, Input, Key, Renderer};
use crate::story::StoryFile;
use crate::world::{World, DEATH_CAUSES};

pub const PANEL_W: i32 = 300;
pub const FOUNDERS: usize = 20;

const SPEEDS: [f32; 11] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0];
const NORMAL_SPEED: usize = 2;
const MAX_STEP: f32 = 1.0 / 60.0;
const MAX_SUBSTEPS: f32 = 64.0;

const BG: Color = 0x1a1a1a;
const TEXT: Color = 0xdddddd;
const DIM: Color = 0x777777;
const ACCENT: Color = 0xffb300;
const CW: i32 = 268; // content width

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Ready,
    Running,
    Paused,
    Ended,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Stats,
    Evolution,
    Culture,
    History,
}

pub struct App {
    pub world: World,
    pub phase: Phase,
    pub tab: Tab,
    pub selected: Option<u64>,
    speed: usize,
    world_w: f32,
    world_h: f32,
    story: Option<StoryFile>,
}

#[derive(Clone, Copy)]
enum ButtonId {
    Start,
    Pause,
    End,
    Slower,
    Faster,
    Tab(Tab),
}

struct Button {
    id: ButtonId,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    label: &'static str,
    enabled: bool,
    active: bool,
}

impl Button {
    fn contains(&self, (mx, my): (i32, i32)) -> bool {
        mx >= self.x && mx < self.x + self.w && my >= self.y && my < self.y + self.h
    }
}

impl App {
    pub fn new(world_w: f32, world_h: f32, seed: u64, story: Option<StoryFile>) -> Self {
        Self {
            world: World::new(world_w, world_h, FOUNDERS, seed, story.as_ref()),
            phase: Phase::Ready,
            tab: Tab::Stats,
            selected: None,
            speed: NORMAL_SPEED,
            world_w,
            world_h,
            story,
        }
    }

    pub fn start(&mut self) {
        match self.phase {
            Phase::Ready => self.phase = Phase::Running,
            Phase::Ended => {
                let seed = self.world.seed.wrapping_add(1);
                self.world = World::new(self.world_w, self.world_h, FOUNDERS, seed, self.story.as_ref());
                self.selected = None;
                self.phase = Phase::Running;
            }
            _ => {}
        }
    }

    pub fn toggle_pause(&mut self) {
        self.phase = match self.phase {
            Phase::Running => Phase::Paused,
            Phase::Paused => Phase::Running,
            p => p,
        };
    }

    pub fn end(&mut self) {
        if matches!(self.phase, Phase::Running | Phase::Paused) {
            self.phase = Phase::Ended;
        }
    }

    pub fn slower(&mut self) {
        self.speed = self.speed.saturating_sub(1);
    }

    pub fn faster(&mut self) {
        self.speed = (self.speed + 1).min(SPEEDS.len() - 1);
    }

    pub fn update(&mut self, dt: f32) -> bool {
        if self.phase != Phase::Running {
            return false;
        }
        let mut remaining = dt * SPEEDS[self.speed];
        let step_size = MAX_STEP.max(remaining / MAX_SUBSTEPS);
        let mut ticked = false;
        while remaining > 0.0 {
            let step = remaining.min(step_size);
            ticked |= self.world.update(step);
            remaining -= step;
        }
        if self.world.npcs.is_empty() {
            self.phase = Phase::Ended;
        }
        ticked
    }

    pub fn handle(&mut self, input: &Input) -> bool {
        for key in &input.pressed {
            match key {
                Key::Enter => self.start(),
                Key::Space => self.toggle_pause(),
                Key::E => self.end(),
                Key::Escape => return false,
                Key::Plus => self.faster(),
                Key::Minus => self.slower(),
                Key::Tab => {
                    self.tab = match self.tab {
                        Tab::Stats => Tab::Evolution,
                        Tab::Evolution => Tab::Culture,
                        Tab::Culture => Tab::History,
                        Tab::History => Tab::Stats,
                    }
                }
            }
        }
        if input.clicked {
            if let Some((mx, my)) = input.mouse {
                if (mx as f32) < self.world_w {
                    self.selected = self.world.npc_at(mx as f32, my as f32, 12.0);
                } else {
                    for b in self.buttons() {
                        if b.enabled && b.contains((mx, my)) {
                            match b.id {
                                ButtonId::Start => self.start(),
                                ButtonId::Pause => self.toggle_pause(),
                                ButtonId::End => self.end(),
                                ButtonId::Slower => self.slower(),
                                ButtonId::Faster => self.faster(),
                                ButtonId::Tab(t) => self.tab = t,
                            }
                        }
                    }
                }
            }
        }
        true
    }

    fn buttons(&self) -> Vec<Button> {
        let x0 = self.world_w as i32 + 16;
        let (y, w, h, gap) = (120, 86, 26, 5);
        let live = matches!(self.phase, Phase::Running | Phase::Paused);
        let b = |id, x, y, w, h, label, enabled, active| Button { id, x, y, w, h, label, enabled, active };
        let mut v = vec![
            b(
                ButtonId::Start, x0, y, w, h,
                if self.phase == Phase::Ended { "RESTART" } else { "START" },
                matches!(self.phase, Phase::Ready | Phase::Ended), false,
            ),
            b(ButtonId::Pause, x0 + w + gap, y, w, h, if self.phase == Phase::Paused { "RESUME" } else { "PAUSE" }, live, false),
            b(ButtonId::End, x0 + 2 * (w + gap), y, w, h, "END", live, false),
            b(ButtonId::Slower, x0, y + h + 8, 26, h, "-", self.speed > 0, false),
            b(ButtonId::Faster, x0 + CW - 26, y + h + 8, 26, h, "+", self.speed < SPEEDS.len() - 1, false),
        ];
        let tabs = [(Tab::Stats, "STATS"), (Tab::Evolution, "EVOLVE"), (Tab::Culture, "CULTURE"), (Tab::History, "HISTORY")];
        for (i, (t, label)) in tabs.iter().enumerate() {
            v.push(b(ButtonId::Tab(*t), x0 + i as i32 * 68, 188, 64, 20, label, true, self.tab == *t));
        }
        v
    }

    pub fn draw(&self, r: &mut dyn Renderer, mouse: Option<(i32, i32)>) {
        draw_world(r, &self.world, self.selected);

        let px = self.world_w as i32;
        let x0 = px + 16;
        r.fill_rect(px, 0, PANEL_W, self.world_h as i32, BG);
        draw_text(r, "CHOICE ENGINE", x0, 16, 2, TEXT);

        let w = &self.world;
        let phase = match self.phase {
            Phase::Ready => "READY",
            Phase::Running => "RUNNING",
            Phase::Paused => "PAUSED",
            Phase::Ended if w.npcs.is_empty() => "ENDED - EXTINCT",
            Phase::Ended => "ENDED",
        };
        draw_text(r, phase, x0, 44, 1, ACCENT);
        draw_text(r, &format!("SEED {}", w.seed), x0 + 150, 44, 1, DIM);
        let time = if w.is_night() { "NIGHT" } else { "DAY" };
        draw_text(r, &format!("TICK {}  DAY {} ({})", w.ticks, w.day(), time), x0, 58, 1, TEXT);
        draw_text(
            r,
            &format!(
                "POP {} FAM {} GEN {}  DEER {} WOLF {}",
                w.npcs.len(),
                w.families_alive(),
                w.max_generation(),
                w.animal_count(Species::Deer),
                w.animal_count(Species::Wolf)
            ),
            x0, 72, 1, TEXT,
        );
        draw_text(
            r,
            &format!("LEADERS {}  SHELTERS {}  PLANTED {}", w.leaders().len(), w.shelters(), w.planted_bushes()),
            x0, 86, 1, TEXT,
        );
        let st = &w.stats;
        draw_text(
            r,
            &format!("TRADE {} GIFT {} INSULT {} FIGHT {}", st.trades, st.gifts, st.insults, st.fights),
            x0, 100, 1, TEXT,
        );

        for b in self.buttons() {
            let hover = mouse.is_some_and(|m| b.contains(m));
            let fill = match (b.enabled, b.active, hover) {
                (false, _, _) => 0x2a2a2a,
                (true, true, _) => 0x5a4a1a,
                (true, false, true) => 0x4a4a4a,
                (true, false, false) => 0x333333,
            };
            r.fill_rect(b.x, b.y, b.w, b.h, fill);
            let tw = text_width(b.label, 1);
            let color = if !b.enabled { DIM } else if b.active { ACCENT } else { TEXT };
            draw_text(r, b.label, b.x + (b.w - tw) / 2, b.y + (b.h - GLYPH_H) / 2, 1, color);
        }
        let speed = SPEEDS[self.speed];
        let label = if speed < 1.0 { format!("SPEED {speed}X") } else { format!("SPEED {}X", speed as u32) };
        draw_text(r, &label, x0 + (CW - text_width(&label, 1)) / 2, 154 + (26 - GLYPH_H) / 2, 1, TEXT);

        match self.tab {
            Tab::Stats => self.stats_tab(r, x0, mouse),
            Tab::Evolution => self.evolution_tab(r, x0, mouse),
            Tab::Culture => self.culture_tab(r, x0),
            Tab::History => self.history_tab(r, x0),
        }

        let y = self.world_h as i32 - 26;
        draw_text(r, "ENTER START  SPACE PAUSE  E END  TAB VIEW", x0, y, 1, DIM);
        draw_text(r, "+/- SPEED  CLICK INSPECT  ESC QUIT", x0, y + 12, 1, DIM);

        if let Some((mx, my)) = mouse {
            draw_cursor(r, mx, my);
        }
    }

    // -----------------------------------------------------------------------

    fn stats_tab(&self, r: &mut dyn Renderer, x0: i32, mouse: Option<(i32, i32)>) {
        let w = &self.world;
        if let Some(n) = self.selected.and_then(|id| w.npc(id)) {
            explain(r, w, n, x0);
            return;
        }
        draw_text(r, "AVG GOAL GENES (HOVER)   N = NOW", x0, 216, 1, TEXT);
        let Some(avg) = w.average_genes() else {
            draw_text(r, "NO ONE LEFT", x0, 232, 1, DIM);
            return;
        };
        let counts = w.goal_counts();
        let marks: Vec<String> = counts.iter().map(|c| c.to_string()).collect();
        let hovered = heatmap(r, &avg, &marks, x0, 230, mouse, heat_color);
        let info_y = 230 + 22 + N_GOALS as i32 * CELL + 4;
        match hovered {
            Some((g, s)) => {
                draw_text(r, &format!("{} <- {}", GOALS[g].name(), STATE_NAMES[s]), x0, info_y, 1, TEXT);
                draw_text(r, &format!("WEIGHT {:.2}", avg[g][s]), x0, info_y + 12, 1, ACCENT);
            }
            None => draw_text(r, "CLICK AN NPC TO HEAR WHY IT ACTS", x0, info_y, 1, DIM),
        }

        let pop: Vec<f32> = w.pop_history.iter().map(|&p| p as f32).collect();
        let max = pop.iter().copied().fold(1.0, f32::max);
        draw_text(r, &format!("POPULATION (MAX {})", max as u32), x0, 438, 1, TEXT);
        line_chart(r, &pop, x0, 450, CW, 34, 0x4fc3f7, true);

        let causes: Vec<String> =
            DEATH_CAUSES.iter().zip(w.deaths_by).map(|((_, name), n)| format!("{name} {n}")).collect();
        draw_text(r, &format!("DIED: {}", causes[..3].join(" ")), x0, 494, 1, DIM);
        draw_text(r, &format!("      {}", causes[3..].join(" ")), x0, 506, 1, DIM);
        draw_text(
            r,
            &format!("RAIDS {} KILLS {} WOLVES SLAIN {}", st(w).raids, st(w).kills, st(w).wolves_slain),
            x0, 522, 1, DIM,
        );
        draw_text(
            r,
            &format!("DEER HUNTED {} MIGRATIONS {} OFFERINGS {}", st(w).deer_hunted, st(w).migrations, st(w).offerings),
            x0, 534, 1, DIM,
        );
        draw_text(
            r,
            &format!("HESITATED {}  COPIED A LEADER {}", st(w).hesitations, st(w).copied_leader),
            x0, 546, 1, DIM,
        );
        let legend = [
            "BODY = FAMILY  DOT = GOAL  ? = HESITATING",
            "GOLD RING = LEADER  LINE = FOLLOWING",
            "TAN = DEER  GREY = WOLF  HUT = SHELTER",
            "ORANGE = FOOD PILE  BROWN = WOOD PILE",
        ];
        for (i, l) in legend.iter().enumerate() {
            draw_text(r, l, x0, 566 + i as i32 * 12, 1, DIM);
        }
    }

    fn evolution_tab(&self, r: &mut dyn Renderer, x0: i32, mouse: Option<(i32, i32)>) {
        let w = &self.world;
        draw_text(r, &format!("FAMILIES ({} OF {} ALIVE)", w.families_alive(), w.n_families), x0, 216, 1, TEXT);
        families_chart(r, &w.family_history, x0, 228, CW, 64);

        draw_text(r, "TRAITS NOW (CHANGE SINCE START)", x0, 302, 1, TEXT);
        let now = w.average_traits();
        let first = w.trait_history.first().copied().unwrap_or(now);
        for t in 0..N_TRAITS {
            let (col, row) = (t / 8, t % 8);
            let d = now[t] - first[t];
            let color = if d > 0.03 { 0x66bb6a } else if d < -0.03 { 0xef5350 } else { TEXT };
            draw_text(
                r,
                &format!("{} {:.2} {:+.2}", TRAIT_SHORT[t], now[t], d),
                x0 + col as i32 * 136, 316 + row as i32 * 12, 1, color,
            );
        }

        let (Some(start), Some(now_g)) = (w.gene_history.first(), w.average_genes()) else { return };
        let mut diff = [[0.0; N_STATES]; N_GOALS];
        for g in 0..N_GOALS {
            for s in 0..N_STATES {
                diff[g][s] = now_g[g][s] - start[g][s];
            }
        }
        draw_text(r, "GOAL GENES: CHANGE SINCE START", x0, 418, 1, TEXT);
        let blank = vec![String::new(); N_GOALS];
        let hovered = heatmap(r, &diff, &blank, x0, 432, mouse, diff_color);
        let info_y = 432 + 22 + N_GOALS as i32 * CELL + 2;
        match hovered {
            Some((g, s)) => {
                draw_text(
                    r,
                    &format!("{} <- {}  {:.2} -> {:.2}", GOALS[g].short(), STATE_SHORT[s], start[g][s], now_g[g][s]),
                    x0, info_y, 1, ACCENT,
                );
                let series: Vec<f32> = w.gene_history.iter().map(|m| m[g][s]).collect();
                line_chart(r, &series, x0, info_y + 12, CW, 28, ACCENT, false);
            }
            None => draw_text(r, "GREEN = GREW  RED = SHRANK", x0, info_y, 1, DIM),
        }
    }

    fn culture_tab(&self, r: &mut dyn Renderer, x0: i32) {
        let w = &self.world;
        let culture = &w.culture;
        // the clicked NPC's family, or the biggest family
        let family = self.selected.and_then(|id| w.npc(id)).map(|n| n.family).or_else(|| {
            let mut counts = vec![0; w.n_families];
            for n in &w.npcs {
                counts[n.family as usize] += 1;
            }
            (0..w.n_families).max_by_key(|&f| counts[f]).map(|f| f as u32)
        });
        let mut y = 216;
        if let Some(f) = family {
            draw_text(r, &format!("FAMILY {f} KNOW-HOW"), x0, y, 1, family_color(f));
            y += 12;
            let known: Vec<&str> = culture.known_by(f).iter().map(|d| d.name()).collect();
            let knows = if known.is_empty() { "NOTHING YET".to_string() } else { known.join(" ") };
            draw_text(r, &format!("KNOWS: {knows}"), x0, y, 1, DIM);
            y += 14;
            let methods = culture.methods(f);
            if methods.is_empty() {
                draw_text(r, "NO METHODS BEYOND THE BASICS", x0, y, 1, DIM);
                y += 12;
            }
            for m in methods.iter().take(6) {
                let src = match m.method.source {
                    Source::Authored => "AUTHORED".to_string(),
                    Source::Learned { tick, .. } => format!("LEARNED T{tick}"),
                    Source::Innate => "INNATE".to_string(),
                };
                draw_text(r, &format!("{} ({src}, VALUE {:.2})", m.method.name, m.value), x0, y, 1, TEXT);
                draw_text(r, &format!(" {}", clip(&m.method.steps_text(), 43)), x0, y + 11, 1, DIM);
                y += 24;
            }
        }
        y += 4;
        let spread: Vec<String> = DISCOVERIES.iter().map(|d| format!("{} {}", d.name(), culture.spread(*d))).collect();
        draw_text(r, &format!("FAMILIES WHO KNOW: {}", spread.join(" ")), x0, y, 1, DIM);
        y += 18;
        draw_text(r, &format!("DISCOVERY LOG ({})", culture.log.len()), x0, y, 1, TEXT);
        y += 14;
        if culture.log.is_empty() {
            draw_text(r, "NOTHING DISCOVERED YET", x0, y, 1, DIM);
        }
        for e in culture.log.iter().rev() {
            if y > 670 {
                break;
            }
            let color = family_color(e.family);
            match &e.how {
                How::Found { by, generation } => {
                    draw_text(r, &format!("T{} FAM {} {} BY #{by} G{generation}", e.tick, e.family, e.discovery.name()), x0, y, 1, color);
                    y += 11;
                    if let Some(m) = &e.method {
                        draw_text(r, &format!(" -> {}", clip(m, 40)), x0, y, 1, DIM);
                        y += 11;
                    }
                }
                How::TaughtBy(from) => {
                    draw_text(r, &format!("T{} FAM {} GOT {} FROM FAM {from}", e.tick, e.family, e.discovery.name()), x0, y, 1, color);
                    y += 11;
                }
                How::Authored(note) => {
                    draw_text(r, &format!("FAM {} {} (AUTHORED)", e.family, e.discovery.name()), x0, y, 1, color);
                    y += 11;
                    if !note.is_empty() {
                        draw_text(r, &format!(" \"{}\"", clip(note, 40)), x0, y, 1, DIM);
                        y += 11;
                    }
                }
            }
            y += 3;
        }
    }

    fn history_tab(&self, r: &mut dyn Renderer, x0: i32) {
        let w = &self.world;
        draw_text(r, "LEADERS", x0, 216, 1, TEXT);
        let leaders = w.leaders();
        if leaders.is_empty() {
            draw_text(r, "NOBODY HAS A FOLLOWING YET", x0, 228, 1, DIM);
        }
        for (i, l) in leaders.iter().take(3).enumerate() {
            draw_text(
                r,
                &format!("{} FAM {} - {} FOLLOWERS, INFL {:.2}", l.label(), l.family, l.followers, l.influence()),
                x0, 228 + i as i32 * 12, 1, family_color(l.family),
            );
        }
        let feuds = w.active_feuds();
        let feud_text = if feuds.is_empty() {
            "FEUDS: NONE".to_string()
        } else {
            let list: Vec<String> = feuds.iter().take(4).map(|f| format!("{} VS {}", f.a, f.b)).collect();
            format!("FEUDS: {}", list.join(", "))
        };
        draw_text(r, &clip(&feud_text, 44), x0, 268, 1, if feuds.is_empty() { DIM } else { 0xef5350 });

        let mut y = 284;
        draw_text(r, &format!("CHRONICLE: WHAT HAPPENED ({})", w.chronicle.events.len()), x0, y, 1, TEXT);
        y += 14;
        for e in w.chronicle.events.iter().rev().take(9) {
            draw_text(r, &format!("T{} {}", e.tick, e.kind.name()), x0, y, 1, ACCENT);
            draw_text(r, &clip(&e.text, 44), x0, y + 11, 1, DIM);
            y += 25;
        }

        let y = 530;
        match self.selected.and_then(|id| w.npc(id)) {
            Some(n) => {
                draw_text(r, &format!("WHAT {} BELIEVES", n.label()), x0, y, 1, family_color(n.family));
                let mut beliefs = n.beliefs.clone();
                beliefs.sort_by(|a, b| b.magnitude.total_cmp(&a.magnitude));
                if beliefs.is_empty() {
                    draw_text(r, "NO STORIES YET", x0, y + 14, 1, DIM);
                }
                for (i, b) in beliefs.iter().take(5).enumerate() {
                    let yy = y + 14 + i as i32 * 26;
                    let truth = &w.chronicle.get(b.event);
                    let color = if b.valence > 0.0 { 0x66bb6a } else { 0xef5350 };
                    draw_text(r, &format!("{} X{:.1} ({} HOPS)", describe(w, b), b.magnitude, b.hops), x0, yy, 1, color);
                    draw_text(r, &format!("TRUTH: {}", clip(&truth.text, 37)), x0, yy + 11, 1, DIM);
                }
            }
            None => {
                draw_text(r, "CLICK AN NPC TO SEE WHAT IT BELIEVES", x0, y, 1, DIM);
                let held: usize = w.npcs.iter().map(|n| n.beliefs.len()).sum();
                let avg = held as f32 / w.npcs.len().max(1) as f32;
                draw_text(r, &format!("STORIES PER NPC: {avg:.1}"), x0, y + 14, 1, DIM);
                let biggest = w
                    .npcs
                    .iter()
                    .flat_map(|n| n.beliefs.iter().map(move |b| (n, b)))
                    .max_by(|a, b| a.1.magnitude.total_cmp(&b.1.magnitude));
                if let Some((n, b)) = biggest {
                    draw_text(r, "BIGGEST LEGEND ALIVE:", x0, y + 30, 1, TEXT);
                    draw_text(r, &format!("{} X{:.1}", describe(w, b), b.magnitude), x0, y + 42, 1, ACCENT);
                    draw_text(r, &format!("HELD BY {}, {} HOPS FROM A WITNESS", n.label(), b.hops), x0, y + 54, 1, DIM);
                    draw_text(r, &format!("TRUTH: {}", clip(&w.chronicle.get(b.event).text, 37)), x0, y + 66, 1, DIM);
                }
            }
        }
    }
}

fn st(w: &World) -> &crate::world::Stats {
    &w.stats
}

/// One NPC explains itself.
fn explain(r: &mut dyn Renderer, w: &World, n: &Npc, x0: i32) {
    let fc = family_color(n.family);
    draw_text(r, &format!("{} ({})", n.label(), n.mind.name()), x0, 216, 1, fc);
    draw_text(r, &format!("FAM {} GEN {} AGE {}", n.family, n.generation, n.age as u32), x0 + 150, 216, 1, DIM);
    let mut y = 232;
    match n.goal() {
        Some(g) => {
            draw_text(r, &format!("GOAL: {} ({:.2})", g.name(), n.goal_scores[g.index()]), x0, y, 1, g.color());
            y += 12;
            if n.stack.len() > 1 {
                let stack: Vec<&str> = n.stack.iter().rev().map(|f| f.goal.short()).collect();
                draw_text(r, &format!("STACK: {}", stack.join(" > ")), x0, y, 1, DIM);
                y += 12;
            }
        }
        None => {
            draw_text(r, "GOAL: NONE", x0, y, 1, DIM);
            y += 12;
        }
    }
    let doing = if n.hesitation > 0.0 {
        "HESITATING...".to_string()
    } else if let Some(a) = &n.active {
        let tag = if n.playing { " (PLAYING)" } else { "" };
        if a.method.steps.len() > 1 {
            format!("{}{tag}: {} ({}/{})", a.method.name, a.prim().name(), a.step + 1, a.method.steps.len())
        } else {
            format!("{}{tag}", a.prim().name())
        }
    } else {
        "NOTHING".to_string()
    };
    draw_text(r, &format!("DOING: {}", clip(&doing, 37)), x0, y, 1, TEXT);
    y += 12;
    let why: Vec<String> = n.why.iter().map(|(k, v)| format!("{k} +{v:.2}")).collect();
    if !why.is_empty() {
        draw_text(r, &format!("WHY: {}", clip(&why.join(", "), 39)), x0, y, 1, ACCENT);
        y += 12;
    }
    y += 4;
    draw_text(
        r,
        &format!("FOOD {:.2} WATER {:.2} HP {:.2} TIRED {:.2}", n.food, n.water, n.health, n.fatigue),
        x0, y, 1, DIM,
    );
    y += 12;
    let carry = match n.carry {
        Some((Item::Food, a)) => format!("FOOD {a:.1}"),
        Some((Item::Wood, a)) => format!("WOOD {a:.1}"),
        None => "-".to_string(),
    };
    draw_text(
        r,
        &format!("FEAR {:.1} LONELY {:.1} ANNOYED {:.1} HOLDS {carry}", n.fear, n.loneliness, n.annoyance),
        x0, y, 1, DIM,
    );
    y += 16;

    draw_text(r, "TRAITS", x0, y, 1, TEXT);
    y += 12;
    for row in 0..4 {
        let line: Vec<String> = (0..4).map(|c| format!("{} {:.2}", TRAIT_SHORT[row * 4 + c], n.traits[row * 4 + c])).collect();
        draw_text(r, &line.join(" "), x0, y, 1, DIM);
        y += 11;
    }
    y += 5;
    let follows = n.following.and_then(|id| w.npc(id)).map_or("NOBODY".to_string(), |l| l.label());
    draw_text(
        r,
        &format!("PRESTIGE {:.2} INFL {:.2} FOLLOWERS {}", n.prestige, n.influence(), n.followers),
        x0, y, 1, TEXT,
    );
    y += 12;
    draw_text(r, &format!("FOLLOWS {follows}  WINS {}", n.wins), x0, y, 1, DIM);
    y += 16;

    draw_text(r, "FEELINGS", x0, y, 1, TEXT);
    y += 12;
    let mut rel = n.relations.clone();
    rel.sort_by(|a, b| b.affinity.abs().total_cmp(&a.affinity.abs()));
    let mut lines = 0;
    for rr in rel.iter().take(3) {
        let (word, color) = if rr.affinity > 0.0 { ("LIKES", 0x66bb6a) } else { ("HATES", 0xef5350) };
        draw_text(r, &format!("{word} #{} (FAM {}) {:+.2}", rr.id, rr.family, rr.affinity), x0, y, 1, color);
        y += 11;
        lines += 1;
    }
    for &(f, v) in n.family_feelings.iter().filter(|f| f.1.abs() > 0.15).take(2) {
        let (word, color) = if v > 0.0 { ("LIKES", 0x66bb6a) } else { ("DISTRUSTS", 0xef5350) };
        draw_text(r, &format!("{word} ALL OF FAMILY {f} {v:+.2}"), x0, y, 1, color);
        y += 11;
        lines += 1;
    }
    if lines == 0 {
        draw_text(r, "NO STRONG FEELINGS", x0, y, 1, DIM);
        y += 11;
    }
    y += 5;
    draw_text(r, "LEGENDS IT BELIEVES", x0, y, 1, TEXT);
    y += 12;
    let mut beliefs = n.beliefs.clone();
    beliefs.sort_by(|a, b| b.magnitude.total_cmp(&a.magnitude));
    if beliefs.is_empty() {
        draw_text(r, "NONE YET", x0, y, 1, DIM);
    }
    for b in beliefs.iter().take(4) {
        let color = if b.valence > 0.0 { 0x66bb6a } else { 0xef5350 };
        draw_text(r, &format!("{} X{:.1}", describe(w, b), b.magnitude), x0, y, 1, color);
        y += 11;
    }
}

/// A belief in words, from the believer's side.
fn describe(w: &World, b: &Belief) -> String {
    let who = |s: Subject| -> String {
        match s {
            Subject::Being(id) => w.npc(id).map_or(format!("#{id}"), |n| n.label()),
            Subject::Family(f) => format!("FAMILY {f}"),
            Subject::Stranger => "A STRANGER".to_string(),
            Subject::Wolves => "WOLVES".to_string(),
            Subject::Place(..) => "THAT PLACE".to_string(),
        }
    };
    let e = w.chronicle.get(b.event);
    let s = b.subject;
    match e.kind {
        EventKind::Rescue => format!("{} SAVED US", who(s)),
        EventKind::Raid => format!("{} ROBBED US", who(s)),
        EventKind::Killing => format!("{} KILLED ONE OF US", who(s)),
        EventKind::WolfAttack => "WOLVES TOOK ONE OF US".to_string(),
        EventKind::WolfSlain => format!("{} SLEW A WOLF", who(s)),
        EventKind::Famine => "THE GREAT HUNGER".to_string(),
        EventKind::Discovery => format!("{}'S DISCOVERY", who(s)),
        EventKind::FirstShelter => format!("{} BUILT OUR SHELTER", who(s)),
        EventKind::LeaderDied => format!("OUR LEADER {}", who(s)),
        EventKind::Migration => "THE MIGRATION".to_string(),
        EventKind::Feud => format!("THE FEUD: {} STARTED IT", who(s)),
        EventKind::Peace => format!("PEACE WITH {}", who(s)),
    }
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(n.saturating_sub(2)).collect();
        t.push_str("..");
        t
    }
}

// ---------------------------------------------------------------------------
// World drawing
// ---------------------------------------------------------------------------

pub fn draw_world(r: &mut dyn Renderer, w: &World, selected: Option<u64>) {
    let bg = if w.is_night() { 0x15151f } else { 0x2b2b2b };
    r.fill_rect(0, 0, w.width as i32, w.height as i32, bg);

    for p in &w.piles {
        let (x, y) = (p.x as i32, p.y as i32);
        if p.is_shelter() {
            let c = family_color(p.family);
            r.fill_rect(x - 14, y - 6, 28, 16, 0x8d6e63);
            r.fill_rect(x - 17, y - 12, 34, 7, 0x5d4037);
            r.fill_rect(x - 3, y + 2, 6, 8, c); // door in the builders' color
        } else {
            let half = 2 + (p.amount.min(2.0) * 3.0) as i32;
            let color = match p.item {
                Item::Food => 0xffb74d,
                Item::Wood => 0x8d6e63,
            };
            r.fill_rect(x - half, y - half, half * 2, half * 2, color);
        }
    }
    for wt in &w.water {
        r.fill_rect(wt.x as i32 - 9, wt.y as i32 - 9, 18, 18, 0x2196f3);
    }
    for t in &w.trees {
        let (x, y) = (t.x as i32, t.y as i32);
        r.fill_rect(x - 2, y, 4, 9, 0x5d4037);
        r.fill_circle(x, y - 2, 3 + (5.0 * t.amount / 10.0) as i32, 0x1b5e20);
    }
    for b in &w.bushes {
        let half = 2 + (6.0 * b.amount / b.max) as i32;
        let color = if b.planted { 0xc0ca33 } else { 0x4caf50 };
        r.fill_rect(b.x as i32 - half, b.y as i32 - half, half * 2, half * 2, color);
    }
    for a in &w.animals {
        let (x, y) = (a.x as i32, a.y as i32);
        match a.species {
            Species::Deer => r.fill_circle(x, y, 4, 0xc8a165),
            Species::Wolf => {
                r.fill_circle(x, y, 5, 0x616161);
                r.fill_rect(x + 1, y - 2, 2, 2, 0xff1744);
            }
        }
    }

    // who follows whom
    for n in &w.npcs {
        if let Some(l) = n.following.and_then(|id| w.npc(id)) {
            dotted_line(r, n.x as i32, n.y as i32, l.x as i32, l.y as i32, 0x555555);
        }
    }

    for n in &w.npcs {
        let (x, y) = (n.x as i32, n.y as i32);
        let radius = if n.is_adult() { 6 } else { 4 };
        if selected == Some(n.id) {
            r.fill_circle(x, y, radius + 4, 0xffffff);
            r.fill_rect(n.home.0 as i32 - 3, n.home.1 as i32 - 3, 7, 7, 0xffffff);
        }
        if n.followers >= 3 {
            r.fill_circle(x, y, radius + 2, 0xffd54f);
        }
        r.fill_circle(x, y, radius, family_color(n.family));
        r.fill_circle(x, y, 2, n.goal().map_or(0x000000, |g: Goal| g.color()));
        match n.carry {
            Some((Item::Food, _)) => r.fill_rect(x + radius, y - 1, 3, 3, 0xffb74d),
            Some((Item::Wood, _)) => r.fill_rect(x + radius, y - 1, 3, 3, 0xa1887f),
            None => {}
        }
        let bars = [(n.food, 0x4caf50), (n.water, 0x2196f3), (n.health, 0xf44336), (n.fatigue, 0xe040fb)];
        for (i, &(v, c)) in bars.iter().enumerate() {
            let bx = x - 9 + i as i32 * 5;
            let full = 8;
            let h = (v * full as f32) as i32;
            r.fill_rect(bx, y - 9 - full, 3, full, 0x111111);
            r.fill_rect(bx, y - 9 - h, 3, h, c);
        }
        if n.hesitation > 0.0 {
            draw_text(r, "?", x + 8, y - 20, 1, 0xffffff);
        }
        if let Some(name) = &n.name {
            let up = name.to_ascii_uppercase();
            draw_text(r, &up, x - text_width(&up, 1) / 2, y - 28, 1, 0xffffff);
        }
    }
}

fn dotted_line(r: &mut dyn Renderer, x0: i32, y0: i32, x1: i32, y1: i32, c: Color) {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let steps = dx.abs().max(dy.abs()).max(1);
    for i in (0..steps).step_by(3) {
        r.fill_rect(x0 + dx * i / steps, y0 + dy * i / steps, 1, 1, c);
    }
}

/// A distinct, stable color per family (hue spaced by the golden angle).
pub fn family_color(family: u32) -> u32 {
    let h = (family as f32 * 0.618_034).fract() * 6.0;
    let (s, v) = (0.65, 0.95);
    let c = v * s;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let to = |f: f32| ((f + m) * 255.0) as u32;
    (to(r) << 16) | (to(g) << 8) | to(b)
}

// ---------------------------------------------------------------------------
// Charts
// ---------------------------------------------------------------------------

const CELL: i32 = 11;

fn heatmap(
    r: &mut dyn Renderer,
    m: &GoalGenes,
    marks: &[String],
    x0: i32,
    y0: i32,
    mouse: Option<(i32, i32)>,
    color: fn(f32) -> Color,
) -> Option<(usize, usize)> {
    let gx = x0 + 32;
    let gy = y0 + 22;
    for (c, label) in STATE_SHORT.iter().enumerate() {
        let ly = if c % 2 == 0 { y0 } else { y0 + 10 };
        draw_text(r, label, gx + c as i32 * CELL, ly, 1, DIM);
    }
    for (row, g) in GOALS.iter().enumerate() {
        let y = gy + row as i32 * CELL;
        r.fill_circle(x0 + 2, y + 5, 2, g.color());
        draw_text(r, g.short(), x0 + 7, y + 2, 1, DIM);
        for col in 0..N_STATES {
            r.fill_rect(gx + col as i32 * CELL, y, CELL - 1, CELL - 1, color(m[row][col]));
        }
        draw_text(r, &marks[row], gx + N_STATES as i32 * CELL + 3, y + 2, 1, TEXT);
    }
    let hovered = mouse.and_then(|(mx, my)| {
        let (col, row) = ((mx - gx) / CELL, (my - gy) / CELL);
        (mx >= gx && my >= gy && (col as usize) < N_STATES && (row as usize) < N_GOALS).then_some((row as usize, col as usize))
    });
    if let Some((row, col)) = hovered {
        let (cx, cy) = (gx + col as i32 * CELL, gy + row as i32 * CELL);
        r.fill_rect(cx - 1, cy - 1, CELL + 1, 1, 0xffffff);
        r.fill_rect(cx - 1, cy + CELL - 1, CELL + 1, 1, 0xffffff);
        r.fill_rect(cx - 1, cy - 1, 1, CELL + 1, 0xffffff);
        r.fill_rect(cx + CELL - 1, cy - 1, 1, CELL + 1, 0xffffff);
    }
    hovered
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let ch = |c: Color, s: u32| ((c >> s) & 0xff) as f32;
    let mix = |s: u32| (ch(a, s) + (ch(b, s) - ch(a, s)) * t) as u32;
    (mix(16) << 16) | (mix(8) << 8) | mix(0)
}

fn heat_color(v: f32) -> Color {
    lerp_color(0x1e1e1e, 0xffb300, (v / 1.2).clamp(0.0, 1.0))
}

fn diff_color(d: f32) -> Color {
    let t = (d / 0.3).clamp(-1.0, 1.0);
    if t >= 0.0 { lerp_color(0x1e1e1e, 0x66bb6a, t) } else { lerp_color(0x1e1e1e, 0xef5350, -t) }
}

fn line_chart(r: &mut dyn Renderer, values: &[f32], x: i32, y: i32, w: i32, h: i32, color: Color, filled: bool) {
    r.fill_rect(x, y, w, h, 0x111111);
    if values.is_empty() {
        return;
    }
    let lo = if filled { 0.0 } else { values.iter().copied().fold(f32::INFINITY, f32::min) };
    let hi = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let span = (hi - lo).max(1e-6);
    for px in 0..w {
        let i = ((px as usize * values.len()) / w as usize).min(values.len() - 1);
        let vy = ((values[i] - lo) / span * (h - 1) as f32) as i32;
        if filled {
            r.fill_rect(x + px, y + h - 1 - vy, 1, vy + 1, color);
        } else {
            r.fill_rect(x + px, y + h - 2 - vy, 1, 2, color);
        }
    }
}

fn families_chart(r: &mut dyn Renderer, history: &[Vec<u32>], x: i32, y: i32, w: i32, h: i32) {
    r.fill_rect(x, y, w, h, 0x111111);
    if history.is_empty() {
        return;
    }
    for px in 0..w {
        let counts = &history[((px as usize * history.len()) / w as usize).min(history.len() - 1)];
        let total: u32 = counts.iter().sum();
        if total == 0 {
            continue;
        }
        let mut acc = 0;
        for (f, &c) in counts.iter().enumerate() {
            if c == 0 {
                continue;
            }
            let top = (acc as f32 / total as f32 * h as f32) as i32;
            acc += c;
            let bottom = (acc as f32 / total as f32 * h as f32) as i32;
            r.fill_rect(x + px, y + h - bottom, 1, bottom - top, family_color(f as u32));
        }
    }
}

fn draw_cursor(r: &mut dyn Renderer, x: i32, y: i32) {
    const ARROW: [&str; 12] = [
        "#", "##", "#.#", "#..#", "#...#", "#....#", "#.....#", "#......#", "#....####", "#..#", "#.#", "##",
    ];
    for (dy, row) in ARROW.iter().enumerate() {
        for (dx, ch) in row.chars().enumerate() {
            let color = match ch {
                '#' => 0x000000,
                '.' => 0xffffff,
                _ => continue,
            };
            r.fill_rect(x + dx as i32, y + dy as i32, 1, 1, color);
        }
    }
}
