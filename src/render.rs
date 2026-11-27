// Everything graphics- and input-related lives behind this trait.
// To use your own rasterizer, write a struct that implements `Renderer`
// (calling into your Rust wrapper) and swap it in main.rs. The sim never
// touches minifb directly. Text is drawn with fill_rect (see font.rs), so
// your rasterizer doesn't need its own text support.

pub type Color = u32; // 0xRRGGBB

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Space,
    Enter,
    E,
    Escape,
    Plus,
    Minus,
    Tab,
}

/// Input gathered since the last frame.
#[derive(Default)]
pub struct Input {
    pub mouse: Option<(i32, i32)>,
    pub clicked: bool,      // left button went down this frame
    pub pressed: Vec<Key>,  // keys that went down this frame
}

pub trait Renderer {
    fn width(&self) -> usize;
    fn height(&self) -> usize;
    fn clear(&mut self, color: Color);
    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Color);
    fn fill_circle(&mut self, cx: i32, cy: i32, r: i32, color: Color);
    /// Push the frame to the screen. Returns false when the window was closed.
    fn present(&mut self) -> bool;
    fn input(&mut self) -> Input;
}

// ---------------------------------------------------------------------------
// Placeholder backend: a plain u32 pixel buffer shown with minifb.
// ---------------------------------------------------------------------------

pub struct MinifbRenderer {
    window: minifb::Window,
    buffer: Vec<u32>,
    w: usize,
    h: usize,
    mouse_was_down: bool,
}

impl MinifbRenderer {
    pub fn new(title: &str, w: usize, h: usize) -> Self {
        let mut window = minifb::Window::new(title, w, h, minifb::WindowOptions::default())
            .expect("could not open window");
        window.set_target_fps(60);
        window.set_cursor_visibility(false); // the sim draws its own cursor (ui.rs)
        Self { window, buffer: vec![0; w * h], w, h, mouse_was_down: false }
    }

    fn put(&mut self, x: i32, y: i32, color: Color) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.buffer[y as usize * self.w + x as usize] = color;
        }
    }
}

impl Renderer for MinifbRenderer {
    fn width(&self) -> usize { self.w }
    fn height(&self) -> usize { self.h }

    fn clear(&mut self, color: Color) {
        self.buffer.fill(color);
    }

    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Color) {
        for py in y..y + h {
            for px in x..x + w {
                self.put(px, py, color);
            }
        }
    }

    fn fill_circle(&mut self, cx: i32, cy: i32, r: i32, color: Color) {
        for py in -r..=r {
            for px in -r..=r {
                if px * px + py * py <= r * r {
                    self.put(cx + px, cy + py, color);
                }
            }
        }
    }

    fn present(&mut self) -> bool {
        if !self.window.is_open() {
            return false;
        }
        self.window
            .update_with_buffer(&self.buffer, self.w, self.h)
            .is_ok()
    }

    fn input(&mut self) -> Input {
        use minifb::{KeyRepeat, MouseButton, MouseMode};

        let mouse = self
            .window
            .get_mouse_pos(MouseMode::Discard)
            .map(|(x, y)| (x as i32, y as i32));

        let down = self.window.get_mouse_down(MouseButton::Left);
        let clicked = down && !self.mouse_was_down;
        self.mouse_was_down = down;

        let pressed = self
            .window
            .get_keys_pressed(KeyRepeat::No)
            .into_iter()
            .filter_map(|k| match k {
                minifb::Key::Space => Some(Key::Space),
                minifb::Key::Enter => Some(Key::Enter),
                minifb::Key::E => Some(Key::E),
                minifb::Key::Escape => Some(Key::Escape),
                minifb::Key::Equal | minifb::Key::NumPadPlus => Some(Key::Plus),
                minifb::Key::Minus | minifb::Key::NumPadMinus => Some(Key::Minus),
                minifb::Key::Tab => Some(Key::Tab),
                _ => None,
            })
            .collect();

        Input { mouse, clicked, pressed }
    }
}
