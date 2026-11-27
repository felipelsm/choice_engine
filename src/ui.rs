// Side panel + sim controls (start / pause / end).

use crate::creature::Personality;
use crate::font::{draw_text, text_width, GLYPH_H};
use crate::render::{Color, Input, Key, Renderer};
use crate::world::{action_color, World, MAX_POPULATION};
use crate::creature::Action;

pub const PANEL_W: i32 = 260;
pub const START_CREATURES: usize = 10;

const BG: Color = 0x1a1a1a;
const TEXT: Color = 0xdddddd;
const DIM: Color = 0x777777;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Ready,   // world generated, waiting for START
    Running,
    Paused,
    Ended,   // stopped by END or extinction
}

pub struct App {
    pub world: World,
    pub phase: Phase,
    world_w: f32,
    world_h: f32,
}

#[derive(Clone, Copy)]
enum ButtonId {
    Start,
    Pause,
    End,
}

struct Button {
    id: ButtonId,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    label: &'static str,
    enabled: bool,
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
            world_w,
            world_h,
        }
    }

    pub fn start(&mut self) {
        match self.phase {
            Phase::Ready => self.phase = Phase::Running,
            Phase::Ended => {
                self.world = World::new(self.world_w, self.world_h, START_CREATURES);
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

    /// Returns true on frames where a decision tick happened.
    pub fn update(&mut self, dt: f32) -> bool {
        if self.phase != Phase::Running {
            return false;
        }
        let ticked = self.world.update(dt);
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
            }
        }

        if input.clicked {
            if let Some(m) = input.mouse {
                for b in self.buttons() {
                    if b.enabled && b.contains(m) {
                        match b.id {
                            ButtonId::Start => self.start(),
                            ButtonId::Pause => self.toggle_pause(),
                            ButtonId::End => self.end(),
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
        vec![
            Button {
                id: ButtonId::Start, x: x0, y, w, h,
                label: if self.phase == Phase::Ended { "RESTART" } else { "START" },
                enabled: matches!(self.phase, Phase::Ready | Phase::Ended),
            },
            Button {
                id: ButtonId::Pause, x: x0 + w + gap, y, w, h,
                label: if self.phase == Phase::Paused { "RESUME" } else { "PAUSE" },
                enabled: running_or_paused,
            },
            Button {
                id: ButtonId::End, x: x0 + 2 * (w + gap), y, w, h,
                label: "END",
                enabled: running_or_paused,
            },
        ]
    }

    pub fn draw(&self, r: &mut dyn Renderer, mouse: Option<(i32, i32)>) {
        self.world.draw(r);

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
        draw_text(r, phase, x0, 44, 1, 0xffc107);
        draw_text(r, &format!("TICK: {}", w.ticks), x0, 62, 1, TEXT);
        draw_text(r, &format!("POPULATION: {} / {}", w.creatures.len(), MAX_POPULATION), x0, 76, 1, TEXT);
        draw_text(r, &format!("BIRTHS: {}  DEATHS: {}", w.births, w.deaths), x0, 90, 1, TEXT);

        // buttons
        for b in self.buttons() {
            let hover = mouse.is_some_and(|m| b.contains(m));
            let fill = match (b.enabled, hover) {
                (false, _) => 0x2a2a2a,
                (true, true) => 0x4a4a4a,
                (true, false) => 0x333333,
            };
            r.fill_rect(b.x, b.y, b.w, b.h, fill);
            let tw = text_width(b.label, 1);
            let color = if b.enabled { TEXT } else { DIM };
            draw_text(r, b.label, b.x + (b.w - tw) / 2, b.y + (b.h - GLYPH_H) / 2, 1, color);
        }

        // average personality
        let mut y = 170;
        draw_text(r, "AVERAGE PERSONALITY", x0, y, 1, TEXT);
        y += 18;
        match w.average_personality() {
            Some(avg) => draw_matrix(r, &avg, x0, y),
            None => draw_text(r, "NO CREATURES", x0, y, 1, DIM),
        }

        // legend
        let mut y = 300;
        draw_text(r, "BODY COLOR = PERSONALITY", x0, y, 1, TEXT);
        y += 14;
        draw_text(r, "GREEN FOOD  BLUE WATER  RED REST", x0, y, 1, DIM);
        y += 22;
        draw_text(r, "CENTER DOT = ACTION", x0, y, 1, TEXT);
        y += 14;
        for (i, (a, label)) in [(Action::WalkToFood, "FOOD"), (Action::WalkToWater, "WATER"), (Action::Rest, "REST")]
            .iter()
            .enumerate()
        {
            let lx = x0 + i as i32 * 70;
            r.fill_circle(lx + 3, y + 3, 3, action_color(*a));
            draw_text(r, label, lx + 10, y, 1, DIM);
        }
        y += 22;
        draw_text(r, "BARS = FOOD WATER BOREDOM", x0, y, 1, TEXT);

        // controls
        let y = self.world_h as i32 - 56;
        draw_text(r, "ENTER  START", x0, y, 1, DIM);
        draw_text(r, "SPACE  PAUSE/RESUME", x0, y + 12, 1, DIM);
        draw_text(r, "E      END", x0, y + 24, 1, DIM);
        draw_text(r, "ESC    QUIT", x0, y + 36, 1, DIM);
    }
}

/// 3x3 grid with state names across the top and action names down the side.
fn draw_matrix(r: &mut dyn Renderer, m: &Personality, x0: i32, y0: i32) {
    let col_x = |c: i32| x0 + 50 + c * 52;
    for (c, label) in ["HUNGER", "THIRST", "BORED"].iter().enumerate() {
        draw_text(r, label, col_x(c as i32), y0, 1, DIM);
    }
    for (row, label) in ["FOOD", "WATER", "REST"].iter().enumerate() {
        let y = y0 + 16 + row as i32 * 16;
        draw_text(r, label, x0, y, 1, DIM);
        for col in 0..3 {
            let v = m[row][col];
            // diagonal (need -> matching action) highlighted
            let color = if row == col { 0xffffff } else { TEXT };
            draw_text(r, &format!("{v:.2}"), col_x(col as i32), y, 1, color);
        }
    }
}
