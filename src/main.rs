mod chronicle;
mod culture;
mod ecology;
mod font;
mod goals;
mod methods;
mod npc;
mod render;
mod social;
mod story;
mod ui;
mod world;

use render::{MinifbRenderer, Renderer};
use std::time::Instant;
use ui::{App, PANEL_W};

const WORLD_W: usize = 760;
const HEIGHT: usize = 720;

struct Args {
    headless: bool,
    seed: u64,
    ticks: u64,
    story: Option<story::StoryFile>,
}

/// --headless   no window, print stats
/// --seed N     same seed = same world history
/// --ticks N    how long to run headless (default 3000)
/// --story F    story file (default: story.toml if it exists)
/// --no-story   no story NPCs
fn parse_args() -> Args {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let seed = value("--seed")
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_secs()));
    let ticks = value("--ticks").and_then(|s| s.parse().ok()).unwrap_or(3000);
    let story = if args.iter().any(|a| a == "--no-story") {
        None
    } else {
        let path = value("--story").unwrap_or_else(|| "story.toml".to_string());
        match story::load(&path) {
            Ok(s) => Some(s),
            Err(e) => {
                if args.iter().any(|a| a == "--story") {
                    eprintln!("could not load story: {e}");
                }
                None
            }
        }
    };
    Args { headless: args.iter().any(|a| a == "--headless"), seed, ticks, story }
}

fn main() {
    let args = parse_args();
    if args.headless {
        run_headless(args);
        return;
    }

    // Swap this line for your own rasterizer once it implements `Renderer`.
    let mut renderer = MinifbRenderer::new("CHOICE engine", WORLD_W + PANEL_W as usize, HEIGHT);
    let world_w = renderer.width() - PANEL_W as usize;
    let mut app = App::new(world_w as f32, renderer.height() as f32, args.seed, args.story);

    let mut last = Instant::now();
    loop {
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(0.1);
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

fn run_headless(args: Args) {
    let mut app = App::new(WORLD_W as f32, HEIGHT as f32, args.seed, args.story);
    app.start();
    println!("seed {}", args.seed);
    while app.world.ticks < args.ticks && app.phase == ui::Phase::Running {
        let every: u64 = std::env::var("EVERY").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
        if app.update(1.0 / 60.0) && app.world.ticks % every == 0 {
            let w = &app.world;
            let s = &w.stats;
            if std::env::var("HUNGRY").is_ok() {
                for n in w.npcs.iter().filter(|n| n.food < 0.15).take(6) {
                    let a = n.active.as_ref().map(|a| format!("{} -> {:?} t{:.1} step{}", a.prim().name(), a.target, a.timer, a.step));
                    println!(
                        "   #{} food {:.2} water {:.2} hp {:.2} goal {:?} hes {:.1} stack {} doing {:?} at ({:.0},{:.0}) bushes>0.05: {}",
                        n.id, n.food, n.water, n.health, n.goal().map(|g| g.name()), n.hesitation, n.stack.len(), a, n.x, n.y,
                        w.bushes.iter().filter(|b| b.amount > 0.05).count()
                    );
                }
            }
            if std::env::var("STORY").is_ok() {
                for n in w.npcs.iter().filter(|n| n.name.is_some()) {
                    let a = n.active.as_ref().map(|a| format!("{} [{}]", a.method.name, a.prim().name()));
                    println!("   {} goal {:?} doing {:?} carry {:?} home ({:.0},{:.0}) at ({:.0},{:.0})", n.label(), n.goal().map(|g| g.name()), a, n.carry, n.home.0, n.home.1, n.x, n.y);
                }
            }
            if std::env::var("GOALS").is_ok() {
                let c = w.goal_counts();
                let line: Vec<String> = goals::GOALS.iter().filter(|g| c[g.index()] > 0).map(|g| format!("{} {}", g.short(), c[g.index()])).collect();
                let hes = w.npcs.iter().filter(|n| n.hesitation > 0.0).count();
                let idle = w.npcs.iter().filter(|n| n.active.is_none()).count();
                let hungry = w.npcs.iter().filter(|n| n.food < 0.2).count();
                println!("      goals: {}  | hesitating {hes} idle {idle} hungry {hungry}", line.join(" "));
            }
            println!(
                "t{:>5} pop {:>3} fam {:>2} gen {:>2} deer {:>2} wolf {:>2} | leaders {} feuds {} shelters {} planted {:>2} | talk {} insult {} gift {} trade {} raid {} fight {} kill {} | events {} discoveries {}",
                w.ticks,
                w.npcs.len(),
                w.families_alive(),
                w.max_generation(),
                w.animal_count(ecology::Species::Deer),
                w.animal_count(ecology::Species::Wolf),
                w.leaders().len(),
                w.active_feuds().len(),
                w.shelters(),
                w.planted_bushes(),
                s.talks,
                s.insults,
                s.gifts,
                s.trades,
                s.raids,
                s.fights,
                s.kills,
                w.chronicle.events.len(),
                w.culture.log.len(),
            );
        }
    }
    let w = &app.world;
    if w.npcs.is_empty() {
        println!("extinct at tick {}", w.ticks);
    }
    let causes: Vec<String> = world::DEATH_CAUSES.iter().zip(w.deaths_by).map(|((_, n), c)| format!("{n} {c}")).collect();
    println!("deaths {}: {}", w.deaths(), causes.join(", "));
    let s = &w.stats;
    println!(
        "hesitations {} copied-leader {} migrations {} offerings {} wolves-slain {} deer-hunted {} refusals {}",
        s.hesitations, s.copied_leader, s.migrations, s.offerings, s.wolves_slain, s.deer_hunted, s.refusals
    );
    let mut kinds: Vec<(String, usize)> = Vec::new();
    for e in &w.chronicle.events {
        match kinds.iter_mut().find(|k| k.0 == e.kind.name()) {
            Some(k) => k.1 += 1,
            None => kinds.push((e.kind.name().to_string(), 1)),
        }
    }
    let kinds: Vec<String> = kinds.iter().map(|(k, c)| format!("{k} {c}")).collect();
    println!("chronicle: {}", kinds.join(", "));
    let mut mags: Vec<f32> = w.npcs.iter().flat_map(|n| n.beliefs.iter().map(|b| b.magnitude)).collect();
    mags.sort_by(|a, b| a.total_cmp(b));
    if !mags.is_empty() {
        let big = mags.iter().filter(|&&m| m >= 4.0).count();
        println!(
            "legends: {} held, median x{:.2}, {:.1}% are x4 or bigger",
            mags.len(),
            mags[mags.len() / 2],
            100.0 * big as f32 / mags.len() as f32
        );
    }
    let mut legends: Vec<(&npc::Npc, &chronicle::Belief)> =
        w.npcs.iter().flat_map(|n| n.beliefs.iter().map(move |b| (n, b))).collect();
    legends.sort_by(|a, b| b.1.magnitude.total_cmp(&a.1.magnitude));
    for (n, b) in legends.iter().take(5) {
        let e = w.chronicle.get(b.event);
        println!(
            "  legend x{:.1} held by {} ({} hops, {}): truth = t{} {}",
            b.magnitude,
            n.label(),
            b.hops,
            if b.valence > 0.0 { "good" } else { "bad" },
            e.tick,
            e.text
        );
    }
    for e in w.culture.log.iter().take(10) {
        let how = match &e.how {
            culture::How::Found { by, generation } => format!("found by #{by} gen {generation}"),
            culture::How::TaughtBy(f) => format!("taught by family {f}"),
            culture::How::Authored(n) => format!("authored \"{n}\""),
        };
        println!("  t{:>5} family {:>2} {} ({how}) {}", e.tick, e.family, e.discovery.name(), e.method.clone().unwrap_or_default());
    }
    for l in w.leaders().iter().take(3) {
        println!("  leader {} family {} followers {} influence {:.2}", l.label(), l.family, l.followers, l.influence());
    }
}
