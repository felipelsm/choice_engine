mod creature;
mod culture;
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
        run_headless(3000);
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
        if app.update(dt) && app.world.ticks % 100 == 0 {
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
                "tick {:>5} pop {:>3} fam {:>2} gen {:>3} died({}) shelters {} sprouts {:>3} piled f{:.1} w{:.1} caution {:.2} | {}",
                w.ticks,
                w.creatures.len(),
                w.families_alive(),
                w.max_generation(),
                causes.join(" "),
                w.shelters(),
                w.sprouts,
                w.piled(creature::Item::Food).max(0.0),
                w.piled(creature::Item::Wood).max(0.0),
                w.average_caution(),
                top.join(", ")
            );
        }
    }
    if app.world.creatures.is_empty() {
        println!("extinct at tick {}", app.world.ticks);
    }
    let w = &app.world;
    let total: u64 = w.action_totals.iter().sum::<u64>().max(1);
    let shares: Vec<String> = creature::ACTIONS
        .iter()
        .map(|a| format!("{} {:.1}%", a.short(), 100.0 * w.action_totals[a.index()] as f64 / total as f64))
        .collect();
    println!("decisions: {}", shares.join("  "));
    let spread: Vec<String> = culture::DISCOVERIES
        .iter()
        .map(|d| format!("{} known by {} families", d.name(), w.culture.spread(*d)))
        .collect();
    println!("culture: {}", spread.join(", "));
    for e in w.culture.log.iter().take(12) {
        let how = match e.how {
            culture::How::Found { by, generation } => format!("found by #{by} (gen {generation})"),
            culture::How::TaughtBy(f) => format!("taught by family {f}"),
        };
        let changes: Vec<String> = e
            .biggest
            .iter()
            .map(|(a, s, v)| format!("{} <- {} +{:.2}", a.name(), creature::STATE_NAMES[*s], v))
            .collect();
        println!("  tick {:>5} family {:>2} {:<7} {} — {}  [{}]", e.tick, e.family, e.discovery.name(), how, e.discovery.fact(), changes.join(", "));
    }
}
