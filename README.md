# CHOICE engine

**C**hoice and **H**esitation **O**ver **I**nteractions with **C**oncurrent **E**ntities

A top-down civilization simulator written in Rust.

## What it does right now

Creatures wander a 2D world with food and water. Every second, each one makes a decision:

```
action = personality * state
```

- **state** is a vector of needs: hunger (1 - food), thirst (1 - water), boredom
- **personality** is a 3x3 matrix, different for every creature
- **action** gives a score for each option (walk to food, walk to water, rest), and the creature picks the highest

### Rules

- The sim starts with 10 creatures with random personalities
- Every 5 ticks, every creature loses 10% of a full stomach of food and water
- Standing at a food or water source refills it; resting lowers boredom
- Resting with full food and water makes one child. The parent spends half its food and water, and the child starts with that half
- The child's personality is the parent's with every entry nudged by up to 5%
- Running out of food or water kills a creature
- Population is capped at 500 for now (`MAX_POPULATION` in `src/world.rs`), since food and water never run out yet

### UI

- Body color comes from personality: each row sum's share of the total sets one channel (food = green, water = blue, rest = red)
- Center dot is the current action, and the bars are food, water and boredom
- The side panel shows the tick, population and the average personality matrix
- Start / pause / end with the buttons or with Enter / Space / E. Esc quits

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