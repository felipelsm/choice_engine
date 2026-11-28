# CHOICE engine

**C**hoice and **H**esitation **O**ver **I**nteractions with **C**oncurrent **E**ntities

A top-down civilization simulator written in Rust. NPCs pick goals, pick ways to reach them, talk, trade, fight, follow leaders, hold grudges against whole families, and tell each other stories that get bigger every time they're retold. Nothing on top of the rules is scripted.

## Running

```
cargo run --release                       # window, random seed
cargo run --release -- --seed 42          # same seed = same history
cargo run --release -- --headless         # no window, prints stats every 200 ticks
cargo run --release -- --headless --seed 3 --ticks 2500
cargo run --release -- --story my.toml    # other story file (default: story.toml if it exists)
cargo run --release -- --no-story         # no story NPCs
cargo test
```

Headless extras (environment variables):

| Variable | What it prints |
|---|---|
| `EVERY=N` | stats line every N ticks instead of 200 |
| `GOALS=1` | how many NPCs are on each goal, how many are hesitating / idle / hungry |
| `STORY=1` | what each story NPC is doing |
| `HUNGRY=1` | details on the hungriest NPCs |

At the end it prints deaths by cause, the chronicle (what really happened), how inflated the legends got, the biggest legends next to the truth, the discovery log and the leaders.

## How an NPC thinks

There are two layers. A **goal** says what it wants ("fix hunger"). A **method** says how ("walk to the bush and eat" or "ask someone for food"). This is basically the "options" idea from reinforcement learning.

### Layer 1: goals

Every second each NPC scores its 14 goals:

```
score(goal) = (genes[goal] . state) * trait_multiplier(goal) + leader_nudge + urgency
```

- **state** is 16 numbers between 0 and 1: hunger, thirst, fatigue, health, age, cold, fear, loneliness, carrying, near home, threat, kin threat, grudge, prey near, reverence, bias (always 1)
- **genes** is a 14 x 16 matrix (goals x states), inherited and mutated
- **trait_multiplier** comes from personality (an aggressive NPC multiplies RAID, a vengeful one AVENGE, and so on)
- **urgency** adds +1 when food or water is under 0.15 or fatigue is over 0.9, so nobody starves while thinking about something else
- a goal only counts if it's possible right now and the NPC knows at least one method for it

Worked example. Say a villager has hunger 0.8, cold 0.6 at night, and everything else low. Its FIX HUNGER row has a big hunger weight, so FIX HUNGER scores 0.9. KEEP WARM scores 0.7. It picks FIX HUNGER.

| Goal | Score |
|---|---|
| FIX HUNGER | 0.90 |
| KEEP WARM | 0.70 |
| REST | 0.35 |

Now it eats a bit. Hunger drops to 0.5, so FIX HUNGER falls to 0.6 and KEEP WARM is the best. It does **not** switch right away though, because of stubbornness (next section).

### Sticking with a goal

Goals live on a stack. The rules, in order:

1. **Done or impossible**: pop it
2. **Give up**: after `10 + 50 * patience` ticks without finishing, drop it and don't pick it again for 15 ticks
3. **Emergency**: STAY SAFE and DEFEND get pushed on top when they pass 0.6, and the old goal resumes after
4. **Switch** only if `challenger > current * (1 + 0.6 * stubbornness) + 0.02`

So in the example, with stubbornness 0.5 the bar is `0.6 * 1.3 + 0.02 = 0.80`. KEEP WARM (0.7) doesn't clear it, so the NPC keeps eating.

### Hesitation

When the challenger is close (over 85% of the bar), the NPC **hesitates** once per goal: it stands still for up to `deliberation` seconds and shows a `?`. While it's hesitating, if it follows a leader, it may just copy the leader's goal (chance = conformity). That's how groups end up doing the same thing.

NPCs also **play**: `0.02 + 0.08 * curiosity` of method choices are random. That's how anyone ever tries dropping food on the ground.

### Layer 2: methods

A method is a list of primitive steps. There are 26 primitives (eat, drink, rest, pick up food/wood, drop, go home, talk, ask, trade, share, steal, attack, flee, mate, offer, rally, guard...). Methods come from three places:

| Source | Example |
|---|---|
| Innate | FIX HUNGER can EAT BUSH, EAT PILE, ASK, TRADE or STEAL |
| Learned | the family found out that "pick up food, go home, drop" makes a pantry, and it becomes a named method |
| Authored | written in the story file, like `bring_wood_home` or `plant` |

Methods are scored by distance, what's in hand, personality and (for learned ones) how well they've paid off.

### Personality

16 traits between 0 and 1, inherited and mutated:

| Group | Traits |
|---|---|
| How it decides | stubbornness, deliberation, patience, curiosity, memory, conformity |
| Body | speed, strength (stronger costs more food) |
| Temper | aggression, courage, vengefulness |
| Social | loyalty, sociability, thrift, charisma, ambition |

### Mind tiers

| Mind | Memory | Learned methods |
|---|---|---|
| SIMPLE | 3 steps | no, innate only |
| VILLAGER | 4 to 16 steps (from the memory trait) | yes |
| STORY | 20 steps, repeats multi-step plans | yes, plus authored ones |

1 in 4 founders are simple. Children of story NPCs are villagers.

### Explain yourself

Click an NPC. The Stats tab shows its goal, what it's doing, **why** (the two states that pushed that goal the most, like `WHY: FATIGUE +0.89, AGE +0.15`), its body, traits, prestige and followers, who it likes and hates, which families it distrusts, and which legends it believes.

## Story NPCs

`story.toml` adds hand-made NPCs:

```toml
[[npc]]
name = "Old Builder"
mind = "story"
x = 380
y = 360
lifespan = 1500
methods = ["bring_wood_home", "plant"]
traits = { sociability = 0.9, patience = 0.9, charisma = 0.8 }
knows = [{ discovery = "warmth", note = "Learned it from the mountain people" }]
```

Each story NPC starts its own family. What it `knows` goes into that family's culture and is marked AUTHORED in the log. From there it spreads only the normal way, by teaching.

## Culture

Each family keeps a log of what it found out. A discovery is only logged when it really happens:

| Discovery | What happened |
|---|---|
| SEEDS | a food pile the family dropped sprouted into a bush |
| WARMTH | a wood pile the family built kept one of them warm at night |
| PANTRY | someone ate food the family had stored for 30+ seconds |

How it knows which behavior caused it: whoever drops something stores its last few steps on the pile. When the pile pays off, those steps become a named method for the family. If the dropper happened to walk home first, the family learns "carry food home". Superstitions can happen too. Methods fade if they stop paying off, families teach each other when they're friendly, and know-how dies with a family if nobody learned it.

## Social life

### Talking

Two NPCs that meet may talk. How much A likes B:

```
attitude = kin bonus + affinity(A, B) + 0.6 * family_feeling(A, B's family)
```

Depending on attitude and temper, A will CHAT, PRAISE or INSULT. B reacts: likes A more, likes A less, or gets annoyed. If annoyance passes B's patience, B **snaps** and attacks.

### Asking and trading

- **Ask**: a hungry NPC begs. The other gives if attitude and generosity (low thrift) are high enough
- **Trade**: food for wood. The price gets worse the less they like each other, and enemies refuse to trade at all
- **Share / steal**: giving builds affinity, stealing destroys it and scares the victim

### Families and prejudice

Every NPC has feelings about individuals **and** about whole families. Being raided, seeing kin killed, hearing gossip and hearing feud stories all move the family feeling. That's how prejudice shows up: an NPC can distrust someone from family 22 it has never met.

### Reproduction and migration

- **Mate**: two NPCs who like each other have a child. Genes and traits are a crossover of both parents, then mutated. The child joins the initiator's family
- **Bud**: alone, a mutated copy of itself
- **Migration**: an ambitious NPC far from home may found a new family. Followers who are close may join (chance = conformity). The new family takes the old one's know-how. Alone, it just settles in its old family

## Conflict

- **Raid**: steal from another family's pile or member
- **Attack / Defend**: fights deal damage based on strength. Being attacked raises threat, seeing kin attacked raises kin threat (DEFEND). Allies nearby join
- **Avenge**: grudges come from being hurt, from kin being killed, and from bad legends. Vengeful NPCs go after the culprit or its family
- **Feuds**: every fight or raid between two families is an incident. 3 incidents make a FEUD. It goes in the chronicle and each side is told a version where **the other family started it**. Kindness between them (gifts, trades, and especially a shared child, which counts as 3) can add up to PEACE

## Ecology

- **Deer** graze grass and bushes, run from wolves and people, and breed when fed (max 50)
- **Wolves** hunt deer, and lone people away from shelters when hungry, more at night (max 8)
- Hunting a deer drops meat for the hunter's family. Killing a wolf earns prestige
- If a species dies out, a few wander in from the edges, so the predator-prey cycle keeps going

## Leadership

```
influence = charisma * (prestige + dominance) * (2 if rallying)
```

- **prestige** comes from rescues, wolf kills, discoveries and gifts, and slowly decays
- **dominance** is strength times fights won
- an NPC follows the most influential NPC it trusts in view, if that NPC's influence is over `max(0.1, 1.5 * its own)`
- anyone with 3+ followers is a **leader** (gold ring). Followers are pulled toward the leader's goal, copy it when hesitating, and come when it **rallies**
- when a leader dies, or a follower migrates, groups split

## Chronicle and legends

The **chronicle** is the truth: rescues, raids, killings, wolf attacks, wolves slain, famines, discoveries, first shelters, leaders dying, migrations, feuds, peace.

What NPCs **believe** is something else. Witnesses get a belief: event, subject (a person, a family, a place, wolves, or "a stranger"), good or bad, and a size (magnitude), starting at x1. When NPCs talk they swap stories:

| Effect | Rule |
|---|---|
| Drift | each retelling changes the size by up to ±20% |
| Absent heroes grow | if the story is about a person who isn't there, it grows 4% per retelling. If they're there, it shrinks 5% |
| Detail loss | 8% chance per retelling that the person's name is lost and they become "a stranger" |
| Trust | stories from someone you dislike are believed less |
| Hops | how many retellings away from a witness |
| Fade | every belief shrinks a little each tick |
| Hardship | when things are bad, NPCs sometimes remember old good stories and they grow a bit |

Each NPC holds at most 8 stories. Big good stories about a place make it **sacred** (NPCs go there to make offerings when things are bad). Big bad stories about a family or wolves feed grudges and prejudice.

So "a stranger saved our village and we worship them" can happen, but it's rare: the rescue has to happen, the name has to get lost, and the story has to keep being retold long after everyone who saw it is dead.

## UI

Four tabs (click, or press Tab):

- **Stats**: average goal genes as a heatmap with how many NPCs are on each goal, population over time, causes of death. Click an NPC to see the explain view instead
- **Evolve**: each family's share of the population over time, how traits drifted since the start, how goal genes changed (hover a cell for its history)
- **Culture**: a family's methods and the discovery log (found, taught or authored)
- **History**: leaders, feuds, the chronicle, and either a clicked NPC's beliefs next to the truth or the biggest legend alive

On the map: body color = family, dot = goal, `?` = hesitating, gold ring = leader, dotted line = following, tan = deer, grey = wolf, hut = shelter (door in the family color), orange/brown squares = food/wood piles.

Controls: Enter start, Space pause, E end, Tab switch view, - / + speed (0.25x to 256x), click to inspect, Esc quit.

## Code map

| File | What's in it |
|---|---|
| `main.rs` | CLI flags, window loop, headless runner |
| `goals.rs` | goals, instincts, the goal stack and `choose_goal` |
| `methods.rs` | primitives, methods (innate, learned, authored) |
| `npc.rs` | states, traits, minds, relationships, reproduction |
| `social.rs` | conversations, asking, trading |
| `culture.rs` | discoveries, family know-how, teaching |
| `chronicle.rs` | events, beliefs, retelling |
| `ecology.rs` | deer and wolves |
| `story.rs` | loading `story.toml` |
| `world.rs` | the world: perception, decisions, running methods, interactions |
| `ui.rs` | panels, tabs, drawing the map |
| `render.rs`, `font.rs` | the `Renderer` trait and a bitmap font |

## Graphics

All drawing and input go through the `Renderer` trait in `src/render.rs`. Right now it uses minifb as a placeholder. Text is drawn with `fill_rect` using the bitmap font in `src/font.rs`, so a new backend doesn't need text support. The plan is to swap in a custom C rasterizer with a Rust wrapper.

## License

MIT
