# CHOICE engine

**C**hoice and **H**esitation **O**ver **I**nteractions with **C**oncurrent **E**ntities

A top-down civilization simulator written in Rust.

## What it does right now

Creatures live in a 2D world with food, water and day/night. Every second, each one makes a decision:

```
action = personality * state
```

- **state** is 14 numbers between 0 and 1 (below)
- **personality** is a 13 x 14 matrix (actions x states), different for every creature
- the creature picks the highest-scoring action it can actually do right now; if the best one is impossible (e.g. sharing food it doesn't have) it falls back to the next best

### States

| State | Meaning |
|---|---|
| Hunger, Thirst | 1 - food / water in the stomach |
| Boredom | rises over time, resting lowers it |
| Health | 0 = dead |
| Age | age / lifespan |
| Food far | distance to the nearest food source that has food |
| Crowding | how many others are close |
| Loneliness | rises when alone, drops near others |
| Kin near | how many family members are close |
| Fear | spikes when robbed, fades over time |
| Carrying | food in hand / carry capacity |
| Home far | distance to home village (1 = no home) |
| Cold | 1 at night when not under a shelter |
| Bias | always 1, gives each action a default pull |

### Actions

| Action | What it does |
|---|---|
| Eat | eat carried food, or walk to the nearest food source or the home stockpile |
| Drink | walk to water |
| Rest | lower boredom (twice as fast under a shelter) |
| Reproduce | make a child (costs 30% food, water and boredom, 15 tick cooldown) |
| Explore | wander somewhere new |
| Follow | move toward family members |
| Share | give food to the hungriest neighbor |
| Steal | take food from a nearby non-family creature; both get hurt and the victim gets scared |
| Flee | run away from whoever robbed you |
| Gather | pick up food to carry instead of eating it |
| Store | drop carried food in the home stockpile (founds or joins a village if you have none) |
| Build | spend carried food building the village shelter |
| Farm | spend carried food planting a new food source next to home |

### Rules

- The sim starts with 20 creatures, each its own family. Children keep the parent's family and village
- Food sources run out and regrow. Farms are smaller but can be planted near villages. Water never runs out
- Every 5 ticks, every creature loses 10% of a full stomach of food and water
- Health drops when food or water is empty, when boredom is maxed, at night without shelter, and from fights. It heals when none of those apply and food and water are above 50%. Health 0 or old age (200 to 300 ticks) = dead
- A day is 60 ticks, the last 40% of it is night
- Random personalities: every entry is 0 to 0.3, plus an "instinct" bump on one natural pair per action (hunger -> eat, cold -> build, ...). See `INSTINCTS` in `src/creature.rs`
- The child's personality is the parent's with every entry nudged by up to 5%
- Population is capped at 1000 as a safety net; food is what limits it in practice

### UI

- Body color comes from personality: red = wild (explore, steal, flee), green = social (follow, share), blue = settler (gather, store, build, farm)
- Center dot is the current action (colors match the heatmap rows). Bars are food, water, health and boredom. Orange square = carrying food
- Houses are finished shelters, outlines are shelters being built, orange squares next to them are stockpiles
- The side panel shows stats, a heatmap of the average personality (hover a cell for details), how many creatures are doing each action, and population over time
- Start / pause / end with the buttons or with Enter / Space / E. Speed 0.25x to 16x with the - / + buttons or keys. Esc quits

## Running

```
cargo run               # opens a window
cargo run -- --headless # no window, prints stats every 10 ticks (incl. causes of death)
cargo test
```

## Graphics

All drawing and input go through the `Renderer` trait in `src/render.rs`. Right now it uses minifb as a placeholder. Text is drawn with `fill_rect` using the bitmap font in `src/font.rs`, so a new backend doesn't need text support. The plan is to swap in a custom C rasterizer with a Rust wrapper.

## License

MIT