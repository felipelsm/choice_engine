// Everything graphics-related lives behind this trait.
// To use your own rasterizer, write a struct that implements `Renderer`
// (calling into your Rust wrapper) and swap it in main.rs. The sim never
// touches minifb directly.

pub type Color = u32; // 0xRRGGBB

pub trait Renderer {
    fn width(&self) -> usize;
    fn height(&self) -> usize;
    fn clear(&mut self, color: Color);
    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Color);
    fn fill_circle(&mut self, cx: i32, cy: i32, r: i32, color: Color);
    /// Push the frame to the screen. Returns false when the window was closed.
    fn present(&mut self) -> bool;
}

// ---------------------------------------------------------------------------
// Placeholder backend: a plain u32 pixel buffer shown with minifb.
// Your rasterizer probably also ends with "here is a framebuffer", so this
// should be easy to replace.
// ---------------------------------------------------------------------------

pub struct MinifbRenderer {
    window: minifb::Window,
    buffer: Vec<u32>,
    w: usize,
    h: usize,
}

impl MinifbRenderer {
    pub fn new(title: &str, w: usize, h: usize) -> Self {
        let mut window = minifb::Window::new(title, w, h, minifb::WindowOptions::default())
            .expect("could not open window");
        window.set_target_fps(60);
        Self { window, buffer: vec![0; w * h], w, h }
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
        if !self.window.is_open() || self.window.is_key_down(minifb::Key::Escape) {
            return false;
        }
        self.window
            .update_with_buffer(&self.buffer, self.w, self.h)
            .is_ok()
    }
}
