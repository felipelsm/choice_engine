# civsim

Top-down civilization simulator in Rust.

## Run

```
cargo run               # window (Esc to quit)
cargo run -- --headless # no window, prints 10 seconds of decisions
cargo test
```

On Linux the placeholder window (minifb) needs X11 dev libs, e.g. `libx11-dev libxkbcommon-dev` (on NixOS add `xorg.libX11` and `libxkbcommon` to your shell).

## How creatures think

Every second (the decision tick) each creature does:

```
action = personality * state

[ food  ]   [ p00 p01 p02 ]   [ hunger  ]
[ water ] = [ p10 p11 p12 ] * [ thirst  ]
[ dance ]   [ p20 p21 p22 ]   [ boredom ]
```

The action vector is normalized into likelihoods (sums to 1) and the top one is picked.
Between ticks, hunger/thirst/boredom keep rising, and doing the chosen action lowers the matching need.

Personalities are random but biased toward the diagonal (hunger mostly pushes toward food, etc.). Change `random_personality()` in `src/creature.rs` for stranger creatures.

## Drawing

On screen: green squares are food, blue squares are water. Creature color shows the current action (green = food, blue = water, pink = dance), and the three bars over each one are hunger, thirst and boredom.

## Graphics / swapping in your own rasterizer

All drawing goes through the `Renderer` trait in `src/render.rs`
(`clear`, `fill_rect`, `fill_circle`, `present`). The sim never touches minifb directly.
To use a different rasterizer, implement `Renderer` for its wrapper type and change the one line in `main.rs` that creates `MinifbRenderer`.

## Layout

- `src/creature.rs` holds the state vector, personality matrix and decision logic
- `src/world.rs` holds resources, the decision tick, movement and drawing
- `src/render.rs` holds the renderer trait and the placeholder minifb backend
- `src/main.rs` holds the game loop
