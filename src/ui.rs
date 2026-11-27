// Side panel + sim controls (start / pause / end / speed) + three tabs:
//   STATS      current state, personality heatmap (average or a clicked creature)
//   EVOLUTION  how the population's genes changed: families over time, change
//              in every weight since tick 0, weight / caution over time
//   CULTURE    what families learned from real outcomes, and the discovery log
// Click a creature in the world to inspect it; click empty ground to clear.

use crate::creature::{Item, Personality, ACTIONS, N_ACTIONS, N_STATES, STATE_NAMES, STATE_SHORT};
use crate::culture::How;
use crate::font::{draw_text, text_width, GLYPH_H};
use crate::render::{Color, Input, Key, Renderer};
use crate::world::{action_color, family_color, World, DEATH_CAUSES};

pub const PANEL_W: i32 = 260;
pub const START_CREATURES: usize = 20;

/// Sim speed options. 1.0 = one decision tick per real second.
const SPEEDS: [f32; 7] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0];
const NORMAL_SPEED: usize = 2; // index of 1.0
/// Biggest time step the world takes at once. Faster speeds take more steps
/// per frame instead of bigger ones, so movement and eating stay accurate.
const MAX_STEP: f32 = 1.0 / 60.0;

const BG: Color = 0x1a1a1a;
const TEXT: Color = 0xdddddd;
const DIM: Color = 0x777777;
const ACCENT: Color = 0xffb300;
const CONTENT_W: i32 = 228;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Ready, // world generated, waiting for START
    Running,
    Paused,
    Ended, // stopped by END or extinction
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Stats,
    Evolution,
    Culture,
}

pub struct App {
    pub world: World,
    pub phase: Phase,
    pub tab: Tab,
    pub selected: Option<u64>, // creature id
    speed: usize,              // index into SPEEDS
    world_w: f32,
    world_h: f32,
}

#[derive(Clone, Copy)]
enum ButtonId {
    Start,
    Pause,
    End,
    Slower,
    Faster,
    StatsTab,
    EvolutionTab,
    CultureTab,
}

struct Button {
    id: ButtonId,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    label: &'static str,
    enabled: bool,
    active: bool, // highlighted (current tab)
}

impl Button {
    fn contains(&self, (mx, my): (i32, i32)) -> bool {
        mx >= self.x && mx < self.x + self.w && my >= self.y && my < self.y + self.h
    }
}

impl App {
    pub fn new(world_w: f32, world_h: f32) -> Self {
        Self {
            world: World::new(world_w, world_h, START_CREATURES),
            phase: Phase::Ready,
            tab: Tab::Stats,
            selected: None,
            speed: NORMAL_SPEED,
            world_w,
            world_h,
        }
    }

    pub fn start(&mut self) {
        match self.phase {
            Phase::Ready => self.phase = Phase::Running,
            Phase::Ended => {
                self.world = World::new(self.world_w, self.world_h, START_CREATURES);
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

    /// `dt` is real seconds since the last frame. Returns true if a decision
    /// tick happened.
    pub fn update(&mut self, dt: f32) -> bool {
        if self.phase != Phase::Running {
            return false;
        }
        let mut remaining = dt * SPEEDS[self.speed];
        let mut ticked = false;
        while remaining > 0.0 {
            let step = remaining.min(MAX_STEP);
            ticked |= self.world.update(step);
            remaining -= step;
        }
        if self.world.creatures.is_empty() {
            self.phase = Phase::Ended;
        }
        ticked
    }

    /// Handle mouse/keyboard. Returns false when the user wants to quit.
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
                        Tab::Culture => Tab::Stats,
                    }
                }
            }
        }

        if input.clicked {
            if let Some((mx, my)) = input.mouse {
                if (mx as f32) < self.world_w {
                    // click in the world: select the creature under the mouse (or clear)
                    self.selected = self.world.creature_at(mx as f32, my as f32, 12.0);
                } else {
                    for b in self.buttons() {
                        if b.enabled && b.contains((mx, my)) {
                            match b.id {
                                ButtonId::Start => self.start(),
                                ButtonId::Pause => self.toggle_pause(),
                                ButtonId::End => self.end(),
                                ButtonId::Slower => self.slower(),
                                ButtonId::Faster => self.faster(),
                                ButtonId::StatsTab => self.tab = Tab::Stats,
                                ButtonId::EvolutionTab => self.tab = Tab::Evolution,
                                ButtonId::CultureTab => self.tab = Tab::Culture,
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
        let (y, w, h, gap) = (120, 70, 26, 7);
        let running_or_paused = matches!(self.phase, Phase::Running | Phase::Paused);
        let button = |id, x, y, w, h, label, enabled, active| Button { id, x, y, w, h, label, enabled, active };
        vec![
            button(
                ButtonId::Start, x0, y, w, h,
                if self.phase == Phase::Ended { "RESTART" } else { "START" },
                matches!(self.phase, Phase::Ready | Phase::Ended), false,
            ),
            button(
                ButtonId::Pause, x0 + w + gap, y, w, h,
                if self.phase == Phase::Paused { "RESUME" } else { "PAUSE" },
                running_or_paused, false,
            ),
            button(ButtonId::End, x0 + 2 * (w + gap), y, w, h, "END", running_or_paused, false),
            button(ButtonId::Slower, x0, y + h + 8, 26, h, "-", self.speed > 0, false),
            button(
                ButtonId::Faster, x0 + 3 * w + 2 * gap - 26, y + h + 8, 26, h, "+",
                self.speed < SPEEDS.len() - 1, false,
            ),
            button(ButtonId::StatsTab, x0, 188, 73, 20, "STATS", true, self.tab == Tab::Stats),
            button(ButtonId::EvolutionTab, x0 + 76, 188, 74, 20, "EVOLUTION", true, self.tab == Tab::Evolution),
            button(ButtonId::CultureTab, x0 + 153, 188, 73, 20, "CULTURE", true, self.tab == Tab::Culture),
        ]
    }

    pub fn draw(&self, r: &mut dyn Renderer, mouse: Option<(i32, i32)>) {
        self.world.draw(r, self.tab == Tab::Evolution, self.selected);

        let px = self.world_w as i32;
        let x0 = px + 16;
        r.fill_rect(px, 0, PANEL_W, self.world_h as i32, BG);

        draw_text(r, "CHOICE ENGINE", x0, 16, 2, TEXT);

        let w = &self.world;
        let phase = match self.phase {
            Phase::Ready => "READY",
            Phase::Running => "RUNNING",
            Phase::Paused => "PAUSED",
            Phase::Ended if w.creatures.is_empty() => "ENDED - EXTINCT",
            Phase::Ended => "ENDED",
        };
        draw_text(r, phase, x0, 44, 1, ACCENT);
        let time = if w.is_night() { "NIGHT" } else { "DAY" };
        draw_text(r, &format!("TICK {}  DAY {} ({})", w.ticks, w.day(), time), x0, 58, 1, TEXT);
        draw_text(r, &format!("POP {}  BIRTHS {}  DEATHS {}", w.creatures.len(), w.births, w.deaths()), x0, 72, 1, TEXT);
        draw_text(r, &format!("SHELTERS {}  PLANTED BUSHES {}", w.shelters(), w.planted_bushes()), x0, 86, 1, TEXT);
        draw_text(
            r,
            &format!("PILED FOOD {:.1}  WOOD {:.1}", w.piled(Item::Food).max(0.0), w.piled(Item::Wood).max(0.0)),
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
        let tw = text_width(&label, 1);
        draw_text(r, &label, x0 + (3 * 70 + 2 * 7 - tw) / 2, 154 + (26 - GLYPH_H) / 2, 1, TEXT);

        match self.tab {
            Tab::Stats => self.draw_stats_tab(r, x0, mouse),
            Tab::Evolution => self.draw_evolution_tab(r, x0, mouse),
            Tab::Culture => self.draw_culture_tab(r, x0, mouse),
        }

        let y = self.world_h as i32 - 56;
        draw_text(r, "ENTER  START", x0, y, 1, DIM);
        draw_text(r, "SPACE  PAUSE/RESUME", x0, y + 12, 1, DIM);
        draw_text(r, "E      END", x0, y + 24, 1, DIM);
        draw_text(r, "ESC    QUIT", x0, y + 36, 1, DIM);
        draw_text(r, "+ / -  SPEED", x0 + 130, y, 1, DIM);
        draw_text(r, "TAB    SWITCH", x0 + 130, y + 12, 1, DIM);
        draw_text(r, "CLICK  INSPECT", x0 + 130, y + 24, 1, DIM);

        // cursor goes last so it's on top of everything
        if let Some((mx, my)) = mouse {
            draw_cursor(r, mx, my);
        }
    }

    fn draw_stats_tab(&self, r: &mut dyn Renderer, x0: i32, mouse: Option<(i32, i32)>) {
        let w = &self.world;
        let selected = self.selected.and_then(|id| w.creature(id));

        // which matrix to show: the clicked creature's, or the average
        let (title, matrix) = match selected {
            Some(c) => (format!("#{}", c.id), Some(c.personality)),
            None => ("AVG PERSONALITY (CLICK ONE)".to_string(), w.average_personality()),
        };
        draw_text(r, &title, x0, 216, 1, TEXT);
        match selected {
            Some(c) => draw_text(
                r,
                &format!("FAM {} GEN {} AGE {} CAUTION {:.2}", c.family, c.generation, c.age as u32, c.caution),
                x0, 228, 1, family_color(c.family),
            ),
            None => {
                let n = w.creatures.len().max(1) as f32;
                let health = w.creatures.iter().map(|c| c.health).sum::<f32>() / n;
                draw_text(r, &format!("AVG HEALTH {:.0}%  HESITATING {}", health * 100.0, w.hesitating()), x0, 228, 1, DIM);
            }
        }

        if let Some(c) = selected {
            let known: Vec<&str> = w.culture.known_by(c.family).iter().map(|d| d.name()).collect();
            let knows = if known.is_empty() { "NOTHING YET".to_string() } else { known.join(" ") };
            let title_w = text_width(&title, 1);
            draw_text(r, &format!("KNOWS {knows}"), x0 + title_w + 6, 216, 1, 0xce93d8);
        }

        let counts = w.action_counts();
        let Some(m) = matrix else {
            draw_text(r, "NO CREATURES", x0, 250, 1, DIM);
            return;
        };
        // for one creature, mark its current action instead of showing counts
        let marks: Vec<String> = match selected {
            Some(c) => ACTIONS.iter().map(|&a| if a == c.action { "<".into() } else { String::new() }).collect(),
            None => counts.iter().map(|n| n.to_string()).collect(),
        };
        let hovered = draw_heatmap(r, &m, &marks, x0, 242, mouse, heat_color);
        let info_y = 242 + 22 + N_ACTIONS as i32 * CELL + 4;
        match hovered {
            Some((row, col)) => {
                draw_text(r, &format!("{} <- {}", ACTIONS[row].name(), STATE_NAMES[col]), x0, info_y, 1, TEXT);
                draw_text(r, &format!("WEIGHT {:.2}", m[row][col]), x0, info_y + 12, 1, ACCENT);
            }
            None => draw_text(r, "BRIGHTER = STRONGER PULL", x0, info_y, 1, DIM),
        }

        // population graph
        let hist: Vec<f32> = w.pop_history.iter().map(|&p| p as f32).collect();
        let max = hist.iter().copied().fold(1.0, f32::max);
        draw_text(r, &format!("POPULATION (MAX {})", max as u32), x0, 458, 1, TEXT);
        draw_line_chart(r, &hist, x0, 470, CONTENT_W, 36, 0x4fc3f7, true);

        // deaths + legend
        let causes: Vec<String> = DEATH_CAUSES
            .iter()
            .zip(w.deaths_by)
            .map(|((_, name), n)| format!("{name} {n}"))
            .collect();
        draw_text(r, &format!("DIED: {}", causes[..3].join(" ")), x0, 514, 1, DIM);
        draw_text(r, &format!("      {}", causes[3..].join(" ")), x0, 526, 1, DIM);
        draw_text(r, "BODY: R WILD G SOCIAL B BUILDER", x0, 544, 1, DIM);
        draw_text(r, "DOT = ACTION  ? = HESITATING", x0, 556, 1, DIM);
        draw_text(r, "ORANGE = FOOD   BROWN = WOOD", x0, 568, 1, DIM);
        draw_text(r, "HUT = WOOD PILE BIG ENOUGH TO", x0, 580, 1, DIM);
        draw_text(r, "      SHELTER. PALE BUSH = PLANTED", x0, 592, 1, DIM);
    }

    fn draw_evolution_tab(&self, r: &mut dyn Renderer, x0: i32, mouse: Option<(i32, i32)>) {
        let w = &self.world;

        draw_text(
            r,
            &format!("FAMILIES ({} OF {} ALIVE)", w.families_alive(), w.n_families),
            x0, 216, 1, TEXT,
        );
        draw_families(r, &w.family_history, x0, 228, CONTENT_W, 80);
        draw_text(r, "SHARE OF POPULATION, TICK 0 -> NOW", x0, 312, 1, DIM);

        draw_text(
            r,
            &format!("MAX GEN {}  CAUTION {:.2}  MISTAKES {}", w.max_generation(), w.average_caution(), w.mistakes),
            x0, 330, 1, TEXT,
        );

        // change of the average personality since tick 0
        let (Some(first), Some(now)) = (w.personality_history.first(), w.average_personality()) else {
            draw_text(r, "NO CREATURES", x0, 350, 1, DIM);
            return;
        };
        let mut diff = [[0.0; N_STATES]; N_ACTIONS];
        for row in 0..N_ACTIONS {
            for col in 0..N_STATES {
                diff[row][col] = now[row][col] - first[row][col];
            }
        }
        draw_text(r, "CHANGE SINCE TICK 0 (HOVER)", x0, 348, 1, TEXT);
        let blank: Vec<String> = vec![String::new(); N_ACTIONS];
        let hovered = draw_heatmap(r, &diff, &blank, x0, 362, mouse, diff_color);
        let info_y = 362 + 22 + N_ACTIONS as i32 * CELL + 4;

        match hovered {
            Some((row, col)) => {
                draw_text(r, &format!("{} <- {}", ACTIONS[row].name(), STATE_NAMES[col]), x0, info_y, 1, TEXT);
                draw_text(
                    r,
                    &format!("WAS {:.2}  NOW {:.2}", first[row][col], now[row][col]),
                    x0, info_y + 12, 1, ACCENT,
                );
                let series: Vec<f32> = w.personality_history.iter().map(|p| p[row][col]).collect();
                draw_text(r, "THIS WEIGHT OVER TIME", x0, info_y + 28, 1, DIM);
                draw_line_chart(r, &series, x0, info_y + 40, CONTENT_W, 36, ACCENT, false);
            }
            None => {
                draw_text(r, "GREEN = GREW   RED = SHRANK", x0, info_y, 1, DIM);
                draw_text(r, "AVG CAUTION OVER TIME (0 TO 1)", x0, info_y + 28, 1, DIM);
                let mut series = w.caution_history.clone();
                // pin the scale to 0..1 so drift is honest
                series.insert(0, 0.0);
                series.insert(1, 1.0);
                draw_line_chart(r, &series[2..], x0, info_y + 40, CONTENT_W, 36, 0xce93d8, false);
            }
        }
    }
}

impl App {
    fn draw_culture_tab(&self, r: &mut dyn Renderer, x0: i32, mouse: Option<(i32, i32)>) {
        let w = &self.world;
        let culture = &w.culture;
        let selected = self.selected.and_then(|id| w.creature(id));

        // which learned matrix to show: the clicked creature's family, or the
        // average over every living creature's family
        let (title, matrix) = match selected {
            Some(c) => (format!("FAMILY {} LEARNED", c.family), Some(*culture.learned(c.family))),
            None if w.creatures.is_empty() => ("AVG LEARNED".to_string(), None),
            None => {
                let mut avg = [[0.0; N_STATES]; N_ACTIONS];
                for c in &w.creatures {
                    let m = culture.learned(c.family);
                    for a in 0..N_ACTIONS {
                        for s in 0..N_STATES {
                            avg[a][s] += m[a][s];
                        }
                    }
                }
                let n = w.creatures.len() as f32;
                avg.iter_mut().flatten().for_each(|v| *v /= n);
                ("AVG LEARNED (CLICK ONE)".to_string(), Some(avg))
            }
        };
        draw_text(r, &title, x0, 216, 1, TEXT);
        let spread: Vec<String> = crate::culture::DISCOVERIES
            .iter()
            .map(|d| format!("{} {}", d.name(), culture.spread(*d)))
            .collect();
        draw_text(r, &format!("FAMILIES: {}", spread.join(" ")), x0, 228, 1, DIM);

        if let Some(m) = matrix {
            let marks: Vec<String> = vec![String::new(); N_ACTIONS];
            let hovered = draw_heatmap(r, &m, &marks, x0, 242, mouse, culture_color);
            let info_y = 242 + 22 + N_ACTIONS as i32 * CELL + 4;
            match hovered {
                Some((row, col)) => {
                    draw_text(r, &format!("{} <- {}", ACTIONS[row].name(), STATE_NAMES[col]), x0, info_y, 1, TEXT);
                    draw_text(r, &format!("LEARNED +{:.2} ON TOP OF GENES", m[row][col]), x0, info_y + 12, 1, ACCENT);
                }
                None => draw_text(r, "BRIGHTER = LEARNED HABIT", x0, info_y, 1, DIM),
            }
        }

        // discovery log, newest first
        draw_text(r, &format!("DISCOVERY LOG ({})", culture.log.len()), x0, 456, 1, TEXT);
        let mut y = 470;
        if culture.log.is_empty() {
            draw_text(r, "NOTHING DISCOVERED YET", x0, y, 1, DIM);
        }
        for e in culture.log.iter().rev() {
            if y > 640 {
                break;
            }
            let color = family_color(e.family);
            match e.how {
                How::Found { by, generation } => {
                    draw_text(
                        r,
                        &format!("T{} FAM {} {} #{} G{}", e.tick, e.family, e.discovery.name(), by, generation),
                        x0, y, 1, color,
                    );
                    y += 12;
                    draw_text(r, &format!(" {}", e.discovery.fact()), x0, y, 1, DIM);
                    y += 12;
                    for (a, s, v) in &e.biggest {
                        draw_text(r, &format!(" {} <- {} +{:.2}", a.short(), STATE_SHORT[*s], v), x0, y, 1, TEXT);
                        y += 12;
                    }
                }
                How::TaughtBy(from) => {
                    draw_text(r, &format!("T{} FAM {} GOT {} FROM {}", e.tick, e.family, e.discovery.name(), from), x0, y, 1, color);
                    y += 12;
                }
            }
            y += 3;
        }
    }
}

/// 0 -> dark grey, 0.6+ -> purple. Learned weights are smaller than genes.
fn culture_color(v: f32) -> Color {
    lerp_color(0x1e1e1e, 0xce93d8, (v / 0.6).clamp(0.0, 1.0))
}

const CELL: i32 = 12;

/// Heatmap: rows = actions, columns = states, `marks` drawn at the end of
/// each row. Returns the hovered (row, col), if any, and outlines it.
fn draw_heatmap(
    r: &mut dyn Renderer,
    m: &Personality,
    marks: &[String],
    x0: i32,
    y0: i32,
    mouse: Option<(i32, i32)>,
    color: fn(f32) -> Color,
) -> Option<(usize, usize)> {
    let gx = x0 + 32; // grid left
    let gy = y0 + 22; // grid top

    // column labels alternate between two rows so they don't run together
    for (c, label) in STATE_SHORT.iter().enumerate() {
        let ly = if c % 2 == 0 { y0 } else { y0 + 10 };
        draw_text(r, label, gx + c as i32 * CELL, ly, 1, DIM);
    }

    for (row, action) in ACTIONS.iter().enumerate() {
        let y = gy + row as i32 * CELL;
        r.fill_circle(x0 + 2, y + 5, 2, action_color(*action));
        draw_text(r, action.short(), x0 + 7, y + 3, 1, DIM);
        for col in 0..N_STATES {
            r.fill_rect(gx + col as i32 * CELL, y, CELL - 1, CELL - 1, color(m[row][col]));
        }
        draw_text(r, &marks[row], gx + N_STATES as i32 * CELL + 3, y + 3, 1, TEXT);
    }

    let hovered = mouse.and_then(|(mx, my)| {
        let (col, row) = ((mx - gx) / CELL, (my - gy) / CELL);
        let inside = mx >= gx && my >= gy && (col as usize) < N_STATES && (row as usize) < N_ACTIONS;
        inside.then_some((row as usize, col as usize))
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

/// 0 -> dark grey, 1.3+ -> amber.
fn heat_color(v: f32) -> Color {
    lerp_color(0x1e1e1e, 0xffb300, (v / 1.3).clamp(0.0, 1.0))
}

/// Negative -> red, 0 -> dark grey, positive -> green. Full color at +-0.3.
fn diff_color(d: f32) -> Color {
    let t = (d / 0.3).clamp(-1.0, 1.0);
    if t >= 0.0 {
        lerp_color(0x1e1e1e, 0x66bb6a, t)
    } else {
        lerp_color(0x1e1e1e, 0xef5350, -t)
    }
}

/// Line (or filled area) chart of `values`, squeezed or stretched to `w`
/// pixels so the whole history always fits. Min / max labels on the right.
fn draw_line_chart(r: &mut dyn Renderer, values: &[f32], x: i32, y: i32, w: i32, h: i32, color: Color, filled: bool) {
    r.fill_rect(x, y, w, h, 0x111111);
    if values.is_empty() {
        return;
    }
    let lo = if filled { 0.0 } else { values.iter().copied().fold(f32::INFINITY, f32::min) };
    let hi = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let span = (hi - lo).max(1e-6);
    for px in 0..w {
        let i = (px as usize * values.len()) / w as usize;
        let v = values[i.min(values.len() - 1)];
        let vy = ((v - lo) / span * (h - 1) as f32) as i32;
        if filled {
            r.fill_rect(x + px, y + h - 1 - vy, 1, vy + 1, color);
        } else {
            r.fill_rect(x + px, y + h - 2 - vy, 1, 2, color);
        }
    }
    if !filled {
        draw_text(r, &format!("{hi:.2}"), x + w - 24, y + 2, 1, DIM);
        draw_text(r, &format!("{lo:.2}"), x + w - 24, y + h - 9, 1, DIM);
    }
}

/// Muller-style plot: each column is one moment in time, split into each
/// family's share of the population (bottom = family 0). Families that die
/// out disappear; successful ones take over the column.
fn draw_families(r: &mut dyn Renderer, history: &[Vec<u32>], x: i32, y: i32, w: i32, h: i32) {
    r.fill_rect(x, y, w, h, 0x111111);
    if history.is_empty() {
        return;
    }
    for px in 0..w {
        let i = (px as usize * history.len()) / w as usize;
        let counts = &history[i.min(history.len() - 1)];
        let total: u32 = counts.iter().sum();
        if total == 0 {
            continue;
        }
        let mut acc = 0u32;
        for (family, &n) in counts.iter().enumerate() {
            if n == 0 {
                continue;
            }
            let top = (acc as f32 / total as f32 * h as f32) as i32;
            acc += n;
            let bottom = (acc as f32 / total as f32 * h as f32) as i32;
            r.fill_rect(x + px, y + h - bottom, 1, bottom - top, family_color(family as u32));
        }
    }
}

/// Arrow cursor: '#' = black outline, '.' = white fill.
fn draw_cursor(r: &mut dyn Renderer, x: i32, y: i32) {
    const ARROW: [&str; 12] = [
        "#",
        "##",
        "#.#",
        "#..#",
        "#...#",
        "#....#",
        "#.....#",
        "#......#",
        "#....####",
        "#..#",
        "#.#",
        "##",
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
