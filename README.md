# CHOICE engine

**C**hoice and **H**esitation **O**ver **I**nteractions with **C**oncurrent **E**ntities

A top-down civilization simulator written in Rust.

## What it does right now

Creatures live in a 2D world with bushes, trees, water and day/night. Every second, each one makes a decision:

```
scores = (genes + family culture) * state
```

- **state** is 15 numbers between 0 and 1 (below)
- **genes** is a 13 x 15 matrix (actions x states), inherited from the parent with small mutations
- **family culture** is another 13 x 15 matrix, learned by the family from real outcomes and passed down (see Culture)
- the creature picks the highest-scoring action it can actually do right now; if the best one is impossible (e.g. sharing food it doesn't have) it falls back to the next best

### Choice and hesitation

When the top two scores are close, a creature has a dilemma. How it handles it depends on its **caution** gene (0 to 1):

- **cautious**: stands still for a moment (up to 1 second on a perfect tie), then picks the best option
- **rash**: acts at once, but may pick the runner-up by mistake (up to 50% on a perfect tie)

Evolution decides which pays off. Hesitating creatures show a `?`.

Creatures also **play**: 5% of decisions are a random possible action (epsilon-greedy exploration). That's how anyone ever tries dropping things.

### States

| State | Meaning |
|---|---|
| Hunger, Thirst | 1 - food / water in the stomach |
| Boredom | rises over time, resting lowers it |
| Health | 0 = dead |
| Age | age / lifespan |
| Food far | distance to the nearest bush or own-family food pile |
| Crowding | how many others are close |
| Loneliness | rises when alone, drops near others |
| Kin near | how many family members are close |
| Fear | spikes when robbed, fades over time |
| Carry food, Carry wood | what's in hand / carry capacity |
| Near home | 1 at home (where you were born), 0 at 100 px or more |
| Cold | 1 at night when not near a shelter |
| Bias | always 1, gives each action a default pull |

### Actions

| Action | What it does |
|---|---|
| Eat | eat carried food, or walk to the nearest bush or own-family food pile |
| Drink | walk to water |
| Rest | lower boredom (twice as fast near a shelter) |
| Reproduce | make a child (costs 30% food, water and boredom, 15 tick cooldown) |
| Explore | wander somewhere new |
| Follow | move toward family members |
| Share | give food to the hungriest neighbor (and teach their family what yours knows) |
| Steal | take food from a nearby non-family creature; both get hurt and the victim gets scared |
| Flee | run away from whoever robbed you |
| Pick up food | walk to food and carry it instead of eating it |
| Pick up wood | walk to a tree and carry wood |
| Drop | drop whatever you're carrying, right here |
| Go home | walk home |

There is no "farm" or "build" action. Those can only emerge from the physics:

- dropped things form **piles**. Food piles belong to the family that dropped them, slowly spoil, and now and then **sprout into a new bush**
- a wood pile with enough wood is a **shelter**: warm at night, better rest

### Culture

Each family keeps a log of what it has found out, and every child inherits it. A discovery is only recorded when it really happens, so the log is objectively true:

| Discovery | What happened |
|---|---|
| SEEDS | a food pile the family dropped sprouted into a bush |
| WARMTH | a wood pile the family built kept one of them warm at night |
| PANTRY | a family member ate from food the family had stored (30+ seconds old) |

**How the family learns which behavior caused it** (credit assignment, the classic reinforcement-learning way):

1. every creature remembers its last 10 decisions: what it felt (state) and what it did (action)
2. when it drops something, that memory is stored on the pile
3. when the pile pays off, the family's culture matrix is strengthened along that memory, most recent steps the most:
   ```
   culture[action_k][s] += 0.08 * reward * 0.7^k * state_k[s]
   ```
   (reward-modulated Hebbian learning with an eligibility trace, see Sutton & Barto ch. 12)

Nobody writes down what a discovery means. If the dropper had walked home before dropping, the family learns "carrying food -> go home"; if it dropped next to a bush, it learns that. Superstitions happen too. Culture slowly fades unless it keeps paying off, families teach each other when they share food, and knowledge dies with a family if nobody learned it from them.

### Rules

- The sim starts with 20 creatures, each its own family with a random caution gene. Children keep the parent's family and home
- Bushes run out and regrow. Planted bushes (sprouted from piles) are smaller. Trees regrow wood. Water never runs out
- Every 5 ticks, every creature loses 10% of a full stomach of food and water
- Health drops when food or water is empty, when boredom is maxed, at night away from a shelter, and from fights. It heals when none of those apply and food and water are above 50%. Health 0 or old age (200 to 300 ticks) = dead
- A day is 60 ticks, the last 40% of it is night
- Random genes: every entry is 0 to 0.3, plus an "instinct" bump on natural pairs (hunger -> eat, fear -> flee, ...) and weak curiosity for picking up and dropping. Nothing tells them to bring things home. See `INSTINCTS` in `src/creature.rs`
- Children's genes are the parent's with every entry nudged by up to 5%, and caution moved by up to 0.05

### UI

Three tabs (click, or press Tab):

- **Stats**: counts, gene heatmap (average, or a clicked creature's), what everyone is doing, population over time, causes of death
- **Evolution**: each family's share of the population over time, how every average gene changed since tick 0 (hover a cell to see it over time), caution over time. Creatures are colored by family on this tab
- **Culture**: learned matrix (average, or a clicked creature's family), which discoveries each family knows, and the discovery log with what each lesson strengthened

Click a creature to inspect it (its home is marked with a white square); click empty ground to go back to averages. Start / pause / end with the buttons or Enter / Space / E. Speed 0.25x to 256x with - / + (use `cargo run --release` for the top speeds). Esc quits.

## Running

```
cargo run               # opens a window
cargo run -- --headless # no window: 3000 ticks of stats, how often each action was chosen, and the discovery log
cargo test
```

## Graphics

All drawing and input go through the `Renderer` trait in `src/render.rs`. Right now it uses minifb as a placeholder. Text is drawn with `fill_rect` using the bitmap font in `src/font.rs`, so a new backend doesn't need text support. The plan is to swap in a custom C rasterizer with a Rust wrapper.

## License

MIT
