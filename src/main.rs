mod creature;
mod font;
mod render;
mod ui;
mod world;

use render::{MinifbRenderer, Renderer};
use std::time::Instant;
use ui::{App, PANEL_W};

const WORLD_W: usize = 760;
const HEIGHT: usize = 600;

fn main() {
    // `cargo run -- --headless` runs without a window and prints a summary.
    if std::env::args().any(|a| a == "--headless") {
        run_headless(300);
        return;
    }

    // Swap this line for your own rasterizer once it implements `Renderer`.
    let mut renderer = MinifbRenderer::new("CHOICE engine", WORLD_W + PANEL_W as usize, HEIGHT);
    let world_w = renderer.width() - PANEL_W as usize;
    let mut app = App::new(world_w as f32, renderer.height() as f32);

    let mut last = Instant::now();
    loop {
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(0.1); // clamp big hiccups
        last = now;

        let input = renderer.input();
        if !app.handle(&input) {
            break;
        }
        app.update(dt);

        renderer.clear(0x000000);
        app.draw(&mut renderer, input.mouse);
        if !renderer.present() {
            break;
        }
    }
}

/// Simulate `ticks` decision ticks at 60 steps/sec, no graphics.
fn run_headless(ticks: u64) {
    let mut app = App::new(WORLD_W as f32, HEIGHT as f32);
    app.start();
    let dt = 1.0 / 60.0;
    while app.world.ticks < ticks && app.phase == ui::Phase::Running {
        if app.update(dt) && app.world.ticks % 10 == 0 {
            let w = &app.world;
            print!("tick {:>4}  pop {:>4}  births {:>4}  deaths {:>4}", w.ticks, w.creatures.len(), w.births, w.deaths);
            if let Some(p) = w.average_personality() {
                print!("  avg diag [{:.2} {:.2} {:.2}]", p[0][0], p[1][1], p[2][2]);
            }
            println!();
        }
    }
    if app.world.creatures.is_empty() {
        println!("extinct at tick {}", app.world.ticks);
    }
}
