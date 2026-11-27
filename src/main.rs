mod creature;
mod font;
mod render;
mod ui;
mod world;

use render::{MinifbRenderer, Renderer};
use std::time::Instant;
use ui::{App, PANEL_W};

const WORLD_W: usize = 760;
const HEIGHT: usize = 720;

fn main() {
    // `cargo run -- --headless` runs without a window and prints a summary.
    if std::env::args().any(|a| a == "--headless") {
        run_headless(900);
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
            let counts = w.action_counts();
            let mut top: Vec<(usize, usize)> = counts.iter().copied().enumerate().collect();
            top.sort_by(|a, b| b.1.cmp(&a.1));
            let top: Vec<String> = top
                .iter()
                .take(4)
                .filter(|(_, n)| *n > 0)
                .map(|(i, n)| format!("{} {}", creature::ACTIONS[*i].short(), n))
                .collect();
            let causes: Vec<String> = world::DEATH_CAUSES
                .iter()
                .zip(w.deaths_by)
                .map(|((_, name), n)| format!("{}{}", &name[..2], n))
                .collect();
            println!(
                "tick {:>4}  pop {:>4}  born {:>5}  died {:>5} ({})  villages {:>2}  shelters {:>2}  farms {:>2}  stored {:>5.1}  | {}",
                w.ticks,
                w.creatures.len(),
                w.births,
                w.deaths,
                causes.join(" "),
                w.active_villages(),
                w.shelters_built(),
                w.farms(),
                w.total_stock(),
                top.join(", ")
            );
        }
    }
    if app.world.creatures.is_empty() {
        println!("extinct at tick {}", app.world.ticks);
    }
}
