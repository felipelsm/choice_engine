# CHOICE engine

**C**hoice and **H**esitation **O**ver **I**nteractions with **C**oncurrent **E**ntities

A top-down civilization simulator written in Rust.

## What it does right now

Creatures wander a 2D world with food and water. Every second, each one makes a decision:

```
action = personality * state
```

- **state** is a vector: hunger (1 - food), thirst (1 - water), boredom, health
- **personality** is a 4x4 matrix (actions x states), different for every creature
- **action** gives a score for each option (walk to food, walk to water, rest, reproduce), and the creature picks the highest

### Rules

- The sim starts with 20 creatures with random personalities: every entry 0 to 1, plus a bias on the main diagonal (`DIAGONAL_BIAS` in `src/creature.rs`)
- Every 5 ticks, every creature loses 10% of a full stomach of food and water
- Standing at a food or water source refills it; resting lowers boredom
- Reproducing makes one child and costs 30% food, water and boredom. It needs at least 30% food and water, and the child starts with the 30% that was spent
- The child's personality is the parent's with every entry nudged by up to 5%
- Health drops while food or water is empty, and while boredom is maxed out (the creature hasn't been resting). It heals while food and water are above 50% and boredom isn't maxed. Health 0 = dead
- Population is capped at 500 for now (`MAX_POPULATION` in `src/world.rs`), since food and water never run out yet

### UI

- Body color comes from personality: the food, water and rest rows' shares set green, blue and red
- Center dot is the current action, and the bars are food, water, health and boredom
- The side panel shows the tick, population, average health and the average personality matrix
- Start / pause / end with the buttons or with Enter / Space / E. Speed 0.25x to 16x with the - / + buttons or keys. Esc quits

## Running

```
cargo run               # opens a window
cargo run -- --headless # no window, prints a summary every 10 ticks
cargo test
```

## Graphics

All drawing and input go through the `Renderer` trait in `src/render.rs`. Right now it uses minifb as a placeholder. Text is drawn with `fill_rect` using the bitmap font in `src/font.rs`, so a new backend doesn't need text support. The plan is to swap in a custom C rasterizer with a Rust wrapper.

## License

MIT