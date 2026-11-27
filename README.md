# CHOICE engine

**C**hoice and **H**esitation **O**ver **I**nteractions with **C**oncurrent **E**ntities

A top-down civilization simulator written in Rust.

## What it does right now

Creatures wander a 2D world with food and water. Every second, each one makes a decision:

```
action = personality * state
```

- **state** is a vector of needs: hunger, thirst, boredom
- **personality** is a 3x3 matrix, different for every creature
- **action** gives a score for each option (walk to food, walk to water, dance), and the creature picks the highest

Needs grow over time and drop when the creature does the matching action.

## Running

```
cargo run               # opens a window (Esc to quit)
cargo run -- --headless # no window, prints decisions to the terminal
cargo test
```

## Graphics

All drawing goes through the `Renderer` trait in `src/render.rs`. Right now it uses minifb as a placeholder. The plan is to swap in a custom C rasterizer with a Rust wrapper.

## License

MIT