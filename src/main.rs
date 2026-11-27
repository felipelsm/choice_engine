mod creature;
mod render;
mod world;

use render::{MinifbRenderer, Renderer};
use std::time::Instant;
use world::World;

const WIDTH: usize = 800;
const HEIGHT: usize = 600;
const N_CREATURES: usize = 12;

fn main() {
    // `cargo run -- --headless` runs without a window and prints decisions.
    if std::env::args().any(|a| a == "--headless") {
        let mut world = World::new(WIDTH as f32, HEIGHT as f32, N_CREATURES);
        run_headless(&mut world, 10);
        return;
    }

    // Swap this line for your own rasterizer once it implements `Renderer`.
    let mut renderer = MinifbRenderer::new("civsim", WIDTH, HEIGHT);
    let mut world = World::new(renderer.width() as f32, renderer.height() as f32, N_CREATURES);

    let mut last = Instant::now();
    loop {
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(0.1); // clamp big hiccups
        last = now;

        if world.update(dt) {
            print_decisions(&world);
        }
        world.draw(&mut renderer);
        if !renderer.present() {
            break;
        }
    }
}

/// Simulate `seconds` of game time at 60 steps/sec, no graphics.
fn run_headless(world: &mut World, seconds: u32) {
    let dt = 1.0 / 60.0;
    for _ in 0..seconds * 60 {
        if world.update(dt) {
            print_decisions(world);
        }
    }
}

fn print_decisions(world: &World) {
    println!("--- tick {} ---", world.ticks);
    for c in &world.creatures {
        println!(
            "#{:<2} state [h {:.2} t {:.2} b {:.2}]  -> p [food {:.2} water {:.2} dance {:.2}]  => {}",
            c.id,
            c.state[0], c.state[1], c.state[2],
            c.last_scores[0], c.last_scores[1], c.last_scores[2],
            c.action.name()
        );
    }
}
