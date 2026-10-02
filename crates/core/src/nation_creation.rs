//! Port of `src/core/game/NationCreation.ts` (the pure subset).
//!
//! Scope: the name-generation machinery — `NAME_TEMPLATES` / `NOUNS` /
//! `O_TO_OES` / `SPECIAL_PLURALS`, `pluralize`, `generateNationName`,
//! `generateUniqueNationName`, `getCompactMapNationCount` and
//! `createRandomNations`. `createNationsForGame` is out of scope: its only
//! extra machinery is zod `GameStartInfo.config` branching, which rides on
//! the already-ported `GameMapSize` / `GameMode` / `GameType` /
//! `HumansVsNations` constants and adds no new deterministic math.
//!
//! Faithfulness notes:
//!
//! * The TS symbols `PLURAL_NOUN` / `NOUN` become `TplPart` variants; a
//!   template part that is neither symbol is a literal string.
//! * `pluralize` indexes JS strings by UTF-16 code unit. The `y` branch reads
//!   `noun[noun.length - 2]`, which is `undefined` for one-character words:
//!   `"aeiou".includes(undefined)` coerces the argument to the string
//!   `"undefined"`, never matches, and the word takes the `-ies` path. The
//!   port models the absent unit as `None` (never a vowel).
//! * `generateNationName` draws the template *before* the noun (two
//!   `nextInt(0, len)` calls in that order); the parts are joined with a
//!   single space.
//! * `generateUniqueNationName` retries up to 1000 times, then falls back to
//!   `base + " " + counter` with the counter starting at 1 and skipping
//!   already-used suffixed variants.
//! * `createRandomNations` interleaves RNG draws exactly as the TS does:
//!   `shuffleArray` (Fisher-Yates, `nextInt(0, i + 1)` per step), then one
//!   `nextID()` per manifest nation in shuffled order, then the extras
//!   filter/shuffle/pick sequence, then per procedural name the
//!   `generateUniqueNationName` draws followed by its `nextID()`.
//! * The Nation/PlayerInfo construction is modelled by the observable fields
//!   the capture reads back: name, spawn cell (present/absent + x/y), nation
//!   flag (present/null) and the `nextID()` string. `playerType` is always
//!   `NATION` and the remaining `PlayerInfo` fields are the fixed ctor
//!   defaults ported in `game_ts`.

use std::collections::HashSet;

use crate::pseudo_random::PseudoRandom;

/// One part of a name template: a literal, the noun marker, or the
/// plural-noun marker (the TS `NOUN` / `PLURAL_NOUN` symbols).
pub enum TplPart {
    Lit(&'static str),
    Noun,
    PluralNoun,
}

/// `NAME_TEMPLATES` — 194 templates in declaration order.
pub const NAME_TEMPLATES: [&[TplPart]; 194] = [
    &[TplPart::Lit("World Famous"), TplPart::Noun],
    &[TplPart::Lit("Famous"), TplPart::PluralNoun],
    &[TplPart::Lit("Comically Large"), TplPart::Noun],
    &[TplPart::Lit("Comically Small"), TplPart::Noun],
    &[TplPart::Lit("Massive"), TplPart::PluralNoun],
    &[TplPart::Lit("Friendly"), TplPart::Noun],
    &[TplPart::Lit("Evil"), TplPart::Noun],
    &[TplPart::Lit("Malicious"), TplPart::Noun],
    &[TplPart::Lit("Spiteful"), TplPart::Noun],
    &[TplPart::Lit("Suspicious"), TplPart::Noun],
    &[TplPart::Lit("Canonically Evil"), TplPart::Noun],
    &[TplPart::Lit("Limited Edition"), TplPart::Noun],
    &[TplPart::Lit("Patent Pending"), TplPart::Noun],
    &[TplPart::Lit("Patented"), TplPart::Noun],
    &[TplPart::Lit("Space"), TplPart::Noun],
    &[TplPart::Lit("Defend The"), TplPart::PluralNoun],
    &[TplPart::Lit("Anarchist"), TplPart::Noun],
    &[TplPart::Lit("Republic of"), TplPart::PluralNoun],
    &[TplPart::Lit("Slippery"), TplPart::Noun],
    &[TplPart::Lit("Wealthy"), TplPart::PluralNoun],
    &[TplPart::Lit("Certified"), TplPart::Noun],
    &[TplPart::Lit("Dr"), TplPart::Noun],
    &[TplPart::Lit("Runaway"), TplPart::Noun],
    &[TplPart::Lit("Chrome"), TplPart::Noun],
    &[TplPart::Lit("All New"), TplPart::Noun],
    &[TplPart::Lit("Top Shelf"), TplPart::PluralNoun],
    &[TplPart::Lit("Invading"), TplPart::PluralNoun],
    &[TplPart::Lit("Loyal To"), TplPart::PluralNoun],
    &[TplPart::Lit("United States of"), TplPart::Noun],
    &[TplPart::Lit("United States of"), TplPart::PluralNoun],
    &[TplPart::Lit("Flowing Rivers of"), TplPart::Noun],
    &[TplPart::Lit("House of"), TplPart::PluralNoun],
    &[TplPart::Lit("Certified Organic"), TplPart::Noun],
    &[TplPart::Lit("Unregulated"), TplPart::Noun],
    &[TplPart::Lit("Slightly Damp"), TplPart::Noun],
    &[TplPart::Lit("Suspiciously Quiet"), TplPart::PluralNoun],
    &[TplPart::Lit("Weaponized"), TplPart::Noun],
    &[TplPart::Lit("Accidentally Evil"), TplPart::Noun],
    &[TplPart::Lit("Extremely Loud"), TplPart::PluralNoun],
    &[TplPart::Lit("Bootleg"), TplPart::Noun],
    &[TplPart::Lit("Questionable"), TplPart::Noun],
    &[TplPart::Lit("Off-Brand"), TplPart::Noun],
    &[TplPart::Lit("Counterfeit"), TplPart::PluralNoun],
    &[TplPart::Lit("Sentient"), TplPart::PluralNoun],
    &[TplPart::Lit("Feral"), TplPart::PluralNoun],
    &[TplPart::Lit("Aggressively Friendly"), TplPart::PluralNoun],
    &[TplPart::Lit("Mildly Threatening"), TplPart::Noun],
    &[TplPart::Lit("Dangerously Cute"), TplPart::PluralNoun],
    &[TplPart::Lit("Legally Distinct"), TplPart::Noun],
    &[TplPart::Lit("Deeply Confused"), TplPart::PluralNoun],
    &[TplPart::Lit("Order of the"), TplPart::Noun],
    &[TplPart::Lit("Knights of the"), TplPart::Noun],
    &[TplPart::Lit("Cult of the"), TplPart::Noun],
    &[TplPart::Lit("League of"), TplPart::PluralNoun],
    &[TplPart::Lit("Band of"), TplPart::PluralNoun],
    &[TplPart::Lit("Council of"), TplPart::PluralNoun],
    &[TplPart::Lit("Assembly of"), TplPart::PluralNoun],
    &[TplPart::Lit("Haunted"), TplPart::Noun],
    &[TplPart::Lit("Cursed"), TplPart::Noun],
    &[TplPart::Lit("Blessed"), TplPart::Noun],
    &[TplPart::Lit("Radioactive"), TplPart::PluralNoun],
    &[TplPart::Lit("Deep Fried"), TplPart::Noun],
    &[TplPart::Lit("Gluten Free"), TplPart::PluralNoun],
    &[TplPart::Lit("Turbocharged"), TplPart::Noun],
    &[TplPart::Lit("Nomadic"), TplPart::PluralNoun],
    &[TplPart::Lit("Vengeful"), TplPart::PluralNoun],
    &[TplPart::Lit("Legendary"), TplPart::PluralNoun],
    &[TplPart::Lit("Outlaw"), TplPart::PluralNoun],
    &[TplPart::Lit("AFK"), TplPart::Noun],
    &[TplPart::Lit("Noob"), TplPart::Noun],
    &[TplPart::Lit("Pro"), TplPart::Noun],
    &[TplPart::Lit("Tryhard"), TplPart::PluralNoun],
    &[TplPart::Lit("Sweaty"), TplPart::PluralNoun],
    &[TplPart::Lit("Griefing"), TplPart::PluralNoun],
    &[TplPart::Lit("Speedrunning"), TplPart::PluralNoun],
    &[TplPart::Lit("Nerfed"), TplPart::PluralNoun],
    &[TplPart::Lit("Buffed"), TplPart::PluralNoun],
    &[TplPart::Lit("OP"), TplPart::Noun],
    &[TplPart::Lit("Overpowered"), TplPart::Noun],
    &[TplPart::Lit("Underpowered"), TplPart::PluralNoun],
    &[TplPart::Lit("Modded"), TplPart::PluralNoun],
    &[TplPart::Lit("Prestige"), TplPart::Noun],
    &[TplPart::Lit("Hardcore"), TplPart::PluralNoun],
    &[TplPart::Lit("Clutch"), TplPart::Noun],
    &[TplPart::Lit("Cracked"), TplPart::Noun],
    &[TplPart::Lit("Unranked"), TplPart::PluralNoun],
    &[TplPart::Lit("Max Level"), TplPart::Noun],
    &[TplPart::Lit("Ironman"), TplPart::Noun],
    &[TplPart::Noun, TplPart::Lit("For Hire")],
    &[TplPart::PluralNoun, TplPart::Lit("That Bite")],
    &[TplPart::PluralNoun, TplPart::Lit("Are Opps")],
    &[TplPart::Noun, TplPart::Lit("Hotel")],
    &[TplPart::PluralNoun, TplPart::Lit("The Movie")],
    &[TplPart::Noun, TplPart::Lit("Corporation")],
    &[TplPart::PluralNoun, TplPart::Lit("Inc")],
    &[TplPart::Noun, TplPart::Lit("Democracy")],
    &[TplPart::Noun, TplPart::Lit("Network")],
    &[TplPart::Noun, TplPart::Lit("Railway")],
    &[TplPart::Noun, TplPart::Lit("Congress")],
    &[TplPart::Noun, TplPart::Lit("Alliance")],
    &[TplPart::Noun, TplPart::Lit("Island")],
    &[TplPart::Noun, TplPart::Lit("Kingdom")],
    &[TplPart::Noun, TplPart::Lit("Empire")],
    &[TplPart::Noun, TplPart::Lit("Dynasty")],
    &[TplPart::Noun, TplPart::Lit("Cartel")],
    &[TplPart::Noun, TplPart::Lit("Cabal")],
    &[TplPart::Noun, TplPart::Lit("Land")],
    &[TplPart::Noun, TplPart::Lit("Oligarchy")],
    &[TplPart::Noun, TplPart::Lit("Nationalist")],
    &[TplPart::Noun, TplPart::Lit("State")],
    &[TplPart::Noun, TplPart::Lit("Duchy")],
    &[TplPart::Noun, TplPart::Lit("Ocean")],
    &[TplPart::Noun, TplPart::Lit("Syndicate")],
    &[TplPart::Noun, TplPart::Lit("Republic")],
    &[TplPart::Noun, TplPart::Lit("Province")],
    &[TplPart::Noun, TplPart::Lit("Dominion")],
    &[TplPart::Noun, TplPart::Lit("Commune")],
    &[TplPart::Noun, TplPart::Lit("Federation")],
    &[TplPart::Noun, TplPart::Lit("Parliament")],
    &[TplPart::Noun, TplPart::Lit("Tribunal")],
    &[TplPart::Noun, TplPart::Lit("Armada")],
    &[TplPart::Noun, TplPart::Lit("Rebellion")],
    &[TplPart::Noun, TplPart::Lit("Resistance")],
    &[TplPart::Noun, TplPart::Lit("Expedition")],
    &[TplPart::Noun, TplPart::Lit("Preservation Society")],
    &[TplPart::Noun, TplPart::Lit("Defense League")],
    &[TplPart::Noun, TplPart::Lit("Thunderdome")],
    &[TplPart::Noun, TplPart::Lit("Uprising")],
    &[TplPart::Noun, TplPart::Lit("Enthusiasts")],
    &[TplPart::Noun, TplPart::Lit("Appreciation Society")],
    &[TplPart::Noun, TplPart::Lit("Fan Club")],
    &[TplPart::Noun, TplPart::Lit("Simulation")],
    &[TplPart::PluralNoun, TplPart::Lit("Anonymous")],
    &[TplPart::PluralNoun, TplPart::Lit("With Attitude")],
    &[TplPart::PluralNoun, TplPart::Lit("Gone Wrong")],
    &[TplPart::PluralNoun, TplPart::Lit("on Vacation")],
    &[TplPart::PluralNoun, TplPart::Lit("in Disguise")],
    &[TplPart::PluralNoun, TplPart::Lit("With Hats")],
    &[TplPart::PluralNoun, TplPart::Lit("on Ice")],
    &[TplPart::PluralNoun, TplPart::Lit("United")],
    &[TplPart::PluralNoun, TplPart::Lit("Unhinged")],
    &[TplPart::PluralNoun, TplPart::Lit("Unleashed")],
    &[TplPart::PluralNoun, TplPart::Lit("Reloaded")],
    &[TplPart::PluralNoun, TplPart::Lit("After Dark")],
    &[TplPart::PluralNoun, TplPart::Lit("From Space")],
    &[TplPart::PluralNoun, TplPart::Lit("of Doom")],
    &[TplPart::PluralNoun, TplPart::Lit("Without Borders")],
    &[TplPart::Noun, TplPart::Lit("Meta")],
    &[TplPart::PluralNoun, TplPart::Lit("OP Please Nerf")],
    &[TplPart::Lit("Alternate"), TplPart::Noun, TplPart::Lit("Universe")],
    &[TplPart::Lit("Famous"), TplPart::Noun, TplPart::Lit("Collection")],
    &[TplPart::Lit("Supersonic"), TplPart::Noun, TplPart::Lit("Spaceship")],
    &[TplPart::Lit("Secret"), TplPart::Noun, TplPart::Lit("Agenda")],
    &[TplPart::Lit("Ballistic"), TplPart::Noun, TplPart::Lit("Missile")],
    &[TplPart::Lit("The"), TplPart::PluralNoun, TplPart::Lit("are SPIES")],
    &[TplPart::Lit("Traveling"), TplPart::Noun, TplPart::Lit("Circus")],
    &[TplPart::Lit("The"), TplPart::PluralNoun, TplPart::Lit("Lied")],
    &[TplPart::Lit("Sacred"), TplPart::Noun, TplPart::Lit("Knowledge")],
    &[TplPart::Lit("Quantum"), TplPart::Noun, TplPart::Lit("Computer")],
    &[TplPart::Lit("Hadron"), TplPart::Noun, TplPart::Lit("Collider")],
    &[TplPart::Lit("Large"), TplPart::Noun, TplPart::Lit("Obliterator")],
    &[TplPart::Lit("Interstellar"), TplPart::Noun, TplPart::Lit("Pirates")],
    &[TplPart::Lit("Alien"), TplPart::Noun, TplPart::Lit("Clan")],
    &[TplPart::Lit("Grand"), TplPart::Noun, TplPart::Lit("Alliance")],
    &[TplPart::Lit("Royal"), TplPart::Noun, TplPart::Lit("Army")],
    &[TplPart::Lit("Holy"), TplPart::Noun, TplPart::Lit("Empire")],
    &[TplPart::Lit("Eternal"), TplPart::Noun, TplPart::Lit("Cabal")],
    &[TplPart::Lit("Invading"), TplPart::Noun, TplPart::Lit("Empire")],
    &[TplPart::Lit("Immortal"), TplPart::Noun, TplPart::Lit("Pirates")],
    &[TplPart::Lit("Shadow"), TplPart::Noun, TplPart::Lit("Cabal")],
    &[TplPart::Lit("Secret"), TplPart::Noun, TplPart::Lit("Dynasty")],
    &[TplPart::Lit("The Great"), TplPart::Noun, TplPart::Lit("Army")],
    &[TplPart::Lit("The"), TplPart::Noun, TplPart::Lit("Matrix")],
    &[TplPart::Lit("Tax-Free"), TplPart::Noun, TplPart::Lit("Paradise")],
    &[TplPart::Lit("Self-Proclaimed"), TplPart::Noun, TplPart::Lit("Experts")],
    &[TplPart::Lit("Forbidden"), TplPart::Noun, TplPart::Lit("Zone")],
    &[TplPart::Lit("Reluctant"), TplPart::Noun, TplPart::Lit("Monarchy")],
    &[TplPart::Lit("Chaotic"), TplPart::Noun, TplPart::Lit("Collective")],
    &[TplPart::Lit("Unsanctioned"), TplPart::Noun, TplPart::Lit("Olympics")],
    &[TplPart::Lit("The"), TplPart::Noun, TplPart::Lit("Conspiracy")],
    &[TplPart::Lit("The"), TplPart::Noun, TplPart::Lit("Incident")],
    &[TplPart::Lit("The"), TplPart::Noun, TplPart::Lit("Situation")],
    &[TplPart::Lit("Premium"), TplPart::Noun, TplPart::Lit("Subscription")],
    &[TplPart::Lit("Clearance"), TplPart::Noun, TplPart::Lit("Warehouse")],
    &[TplPart::Lit("Budget"), TplPart::Noun, TplPart::Lit("Emporium")],
    &[TplPart::Lit("Overnight"), TplPart::Noun, TplPart::Lit("Delivery")],
    &[TplPart::Lit("National"), TplPart::Noun, TplPart::Lit("Reserve")],
    &[TplPart::Lit("The"), TplPart::Noun, TplPart::Lit("Dimension")],
    &[TplPart::Lit("The"), TplPart::Noun, TplPart::Lit("Prophecy")],
    &[TplPart::Lit("The"), TplPart::Noun, TplPart::Lit("Awakening")],
    &[TplPart::Lit("The"), TplPart::Noun, TplPart::Lit("Inquisition")],
    &[TplPart::Lit("Legendary"), TplPart::Noun, TplPart::Lit("Drop")],
    &[TplPart::Lit("Elite"), TplPart::Noun, TplPart::Lit("Squad")],
    &[TplPart::Lit("The"), TplPart::Noun, TplPart::Lit("Saga")],
];

/// `NOUNS` — 236 words in declaration order, duplicates included (the
/// `Fullsender` / `Mito` / `Mitochondria` triples are intentional).
pub const NOUNS: [&str; 236] = [
    "Snail",
    "Cow",
    "Giraffe",
    "Donkey",
    "Horse",
    "Mushroom",
    "Salad",
    "Kitten",
    "Fork",
    "Apple",
    "Pancake",
    "Tree",
    "Fern",
    "Seashell",
    "Turtle",
    "Casserole",
    "Gnome",
    "Frog",
    "Cheese",
    "Mold",
    "Clown",
    "Boat",
    "Robot",
    "Millionaire",
    "Billionaire",
    "Pigeon",
    "Fish",
    "Bumblebee",
    "Jelly",
    "Wizard",
    "Worm",
    "Rat",
    "Pumpkin",
    "Zombie",
    "Grass",
    "Bear",
    "Skunk",
    "Sandwich",
    "Butter",
    "Soda",
    "Pickle",
    "Potato",
    "Book",
    "Friend",
    "Feather",
    "Flower",
    "Oil",
    "Train",
    "Fan",
    "Salmon",
    "Cod",
    "Sink",
    "Villain",
    "Bug",
    "Car",
    "Soup",
    "Puppy",
    "Rock",
    "Stick",
    "Succulent",
    "Nerd",
    "Mercenary",
    "Ninja",
    "Burger",
    "Tomato",
    "Penguin",
    "Waffle",
    "Toaster",
    "Hamster",
    "Pretzel",
    "Walrus",
    "Raccoon",
    "Llama",
    "Noodle",
    "Goblin",
    "Muffin",
    "Coconut",
    "Biscuit",
    "Cactus",
    "Moose",
    "Platypus",
    "Yeti",
    "Sponge",
    "Spatula",
    "Trampoline",
    "Dolphin",
    "Taco",
    "Chainsaw",
    "Spoon",
    "Doorknob",
    "Bathrobe",
    "Lampshade",
    "Crowbar",
    "Shoelace",
    "Wheelbarrow",
    "Barnacle",
    "Armadillo",
    "Cabbage",
    "Wig",
    "Plunger",
    "Kazoo",
    "Napkin",
    "Pelican",
    "Turnip",
    "Canoe",
    "Igloo",
    "Stapler",
    "Ferret",
    "Anchovy",
    "Dumpling",
    "Mattress",
    "Parsnip",
    "Gargoyle",
    "Crayon",
    "Corgi",
    "Macaroni",
    "Blender",
    "Ukulele",
    "Flamingo",
    "Nugget",
    "Porcupine",
    "Tadpole",
    "Papaya",
    "Chinchilla",
    "Teapot",
    "Baguette",
    "Squid",
    "Otter",
    "Badger",
    "Hedgehog",
    "Mantis",
    "Scorpion",
    "Vulture",
    "Falcon",
    "Jackal",
    "Hyena",
    "Panther",
    "Stingray",
    "Octopus",
    "Basilisk",
    "Dragon",
    "Sphinx",
    "Phoenix",
    "Kraken",
    "Leviathan",
    "Mammoth",
    "Chimera",
    "Griffin",
    "Minotaur",
    "Cyclops",
    "Brick",
    "Anvil",
    "Torpedo",
    "Lantern",
    "Compass",
    "Telescope",
    "Pendulum",
    "Furnace",
    "Cauldron",
    "Beacon",
    "Anchor",
    "Dagger",
    "Gauntlet",
    "Helmet",
    "Shield",
    "Banner",
    "Trumpet",
    "Bagpipe",
    "Tambourine",
    "Accordion",
    "Xylophone",
    "Avocado",
    "Broccoli",
    "Radish",
    "Artichoke",
    "Kumquat",
    "Pomegranate",
    "Mango",
    "Truffle",
    "Croissant",
    "Lasagna",
    "Souffl\u{e9}",
    "Spaghetti",
    "Tsunami",
    "Tornado",
    "Avalanche",
    "Volcano",
    "Glacier",
    "Comet",
    "Meteor",
    "Nebula",
    "Supernova",
    "Quasar",
    "Abyss",
    "Labyrinth",
    "Caterpillar",
    "Chameleon",
    "Narwhal",
    "Capybara",
    "Pangolin",
    "Axolotl",
    "Sloth",
    "Lemur",
    "Alpaca",
    "Tapir",
    "Wombat",
    "Ocelot",
    "Manatee",
    "Ibis",
    "Kiwi",
    "Creeper",
    "Enderman",
    "Skeleton",
    "Necromancer",
    "Paladin",
    "Warlock",
    "Ranger",
    "Boss",
    "NPC",
    "Assassin",
    "Viking",
    "Samurai",
    "Pirate",
    "Champion",
    "Gladiator",
    "Demon",
    "Angel",
    "Fullsender",
    "Fullsender",
    "Fullsender",
    "Mito",
    "Mito",
    "Mito",
    "Mitochondria",
    "Mitochondria",
    "Mitochondria",
];

/// `O_TO_OES` — the irregular `-oes` set (Set iteration = insertion order).
pub const O_TO_OES: [&str; 4] = ["Potato", "Tomato", "Volcano", "Torpedo"];

/// `SPECIAL_PLURALS` — the irregular map in insertion order.
pub const SPECIAL_PLURALS: [(&str, &str); 11] = [
    ("Cactus", "Cacti"),
    ("Platypus", "Platypuses"),
    ("Moose", "Moose"),
    ("Octopus", "Octopi"),
    ("Cyclops", "Cyclopes"),
    ("Samurai", "Samurai"),
    ("Fish", "Fish"),
    ("Salmon", "Salmon"),
    ("Cod", "Cod"),
    ("Enderman", "Endermen"),
    ("Mitochondria", "Mitochondria"),
];

/// `pluralize(noun)` — branch order and UTF-16 indexing as in the TS.
pub fn pluralize(noun: &str) -> String {
    if let Some((_, p)) = SPECIAL_PLURALS.iter().find(|(k, _)| *k == noun) {
        return p.to_string();
    }
    let u: Vec<u16> = noun.encode_utf16().collect();
    let ends = |s: &str| u.ends_with(&s.encode_utf16().collect::<Vec<u16>>()[..]);
    if ends("s") || ends("ch") || ends("sh") || ends("x") || ends("z") {
        return format!("{noun}es");
    }
    if ends("y") {
        // JS `noun[noun.length - 2]`: absent for one-character words, which
        // `includes` coerces to the string "undefined" — never a vowel, so
        // the word still takes the -ies path.
        let prev_vowel = u.len() >= 2 && matches!(u[u.len() - 2], 0x61 | 0x65 | 0x69 | 0x6F | 0x75);
        if !prev_vowel {
            let head = String::from_utf16_lossy(&u[..u.len() - 1]);
            return format!("{head}ies");
        }
    }
    if O_TO_OES.contains(&noun) {
        return format!("{noun}es");
    }
    format!("{noun}s")
}

/// `generateNationName(random)` — template draw first, then noun draw.
pub fn generate_nation_name(rng: &mut PseudoRandom) -> String {
    let tpl = NAME_TEMPLATES[rng.next_int(0.0, NAME_TEMPLATES.len() as f64) as usize];
    let noun = NOUNS[rng.next_int(0.0, NOUNS.len() as f64) as usize];
    let mut parts: Vec<String> = Vec::with_capacity(tpl.len());
    for part in tpl {
        match part {
            TplPart::PluralNoun => parts.push(pluralize(noun)),
            TplPart::Noun => parts.push(noun.to_string()),
            TplPart::Lit(s) => parts.push((*s).to_string()),
        }
    }
    parts.join(" ")
}

/// `generateUniqueNationName(random, usedNames)` — 1000 retries, then the
/// `base counter` fallback with the counter starting at 1.
pub fn generate_unique_nation_name(rng: &mut PseudoRandom, used: &mut HashSet<String>) -> String {
    for _ in 0..1000 {
        let name = generate_nation_name(rng);
        if !used.contains(&name) {
            return name;
        }
    }
    let mut counter = 1i64;
    let base = generate_nation_name(rng);
    loop {
        let cand = format!("{base} {counter}");
        if !used.contains(&cand) {
            return cand;
        }
        counter += 1;
    }
}

/// `getCompactMapNationCount(manifestNationCount, isCompactMap)`.
pub fn get_compact_map_nation_count(manifest_count: f64, is_compact: bool) -> f64 {
    if manifest_count == 0.0 {
        return 0.0;
    }
    if is_compact {
        (manifest_count * 0.25).floor().max(1.0)
    } else {
        manifest_count
    }
}

/// A manifest / additional nation as the capture sees it (the
/// `TerrainMapLoader` interfaces, reduced to the fields `toNation` reads).
#[derive(Clone)]
pub struct NcNation {
    pub name: String,
    pub coord: Option<(f64, f64)>,
    pub flag: Option<String>,
}

/// The observable result of `toNation` / the inline `new Nation(...)` paths:
/// the PlayerInfo name + nextID() and the optional spawn cell / flag.
pub struct NcOut {
    pub name: String,
    pub coord: Option<(f64, f64)>,
    pub flag: Option<String>,
    pub id: String,
}

/// `createRandomNations(targetCount, manifestNations, additionalNations,
/// toNation, random)` — the RNG draw order is the port's contract: shuffle,
/// one `nextID()` per manifest nation in shuffled order, then the extras
/// filter/shuffle/pick, then per procedural name the unique-name draws
/// followed by its `nextID()`.
pub fn create_random_nations(
    target: usize,
    manifest: &[NcNation],
    extras: &[NcNation],
    rng: &mut PseudoRandom,
) -> Vec<NcOut> {
    let to_nation = |n: &NcNation, rng: &mut PseudoRandom| NcOut {
        name: n.name.clone(),
        coord: n.coord,
        flag: n.flag.clone(),
        id: rng.next_id(),
    };
    let shuffled = rng.shuffle_array(manifest);
    if target <= manifest.len() {
        return shuffled
            .iter()
            .take(target)
            .map(|n| to_nation(n, rng))
            .collect();
    }
    let mut nations: Vec<NcOut> = shuffled.iter().map(|n| to_nation(n, rng)).collect();
    let mut used: HashSet<String> = nations.iter().map(|n| n.name.clone()).collect();
    let mut remaining = target - manifest.len();

    if remaining > 0 && !extras.is_empty() {
        let candidates: Vec<NcNation> = extras
            .iter()
            .filter(|n| !used.contains(&n.name))
            .cloned()
            .collect();
        let shuffled_extras = rng.shuffle_array(&candidates);
        let picked = shuffled_extras.len().min(remaining);
        for extra in shuffled_extras.iter().take(picked) {
            nations.push(to_nation(extra, rng));
            used.insert(extra.name.clone());
        }
        remaining -= picked;
    }

    for _ in 0..remaining {
        let name = generate_unique_nation_name(rng, &mut used);
        used.insert(name.clone());
        nations.push(NcOut {
            name,
            coord: None,
            flag: None,
            id: rng.next_id(),
        });
    }
    nations
}

// ---------------------------------------------------------------- vectors op
//
// A flat `f64` token runner shared by the golden replay and the wasm probe.
// Strings cross as `[len, u0, .. u(len-1)]` (UTF-16 code units).
//
// kind table (mirrors the capture):
//   0 [0] -> [tN, (pN, (tag, lit?)*)*]            NAME_TEMPLATES dump
//   1 [0] -> [nN, (noun)*]                        NOUNS dump
//   2 [0] -> [kN, (word)*]                        O_TO_OES dump
//   3 [0] -> [kN, (key, value)*]                  SPECIAL_PLURALS dump
//   4 [noun] -> [plural]
//   5 [seed] -> [name]                            generateNationName
//   6 [seed, uN, (used)*] -> [name]               generateUniqueNationName
//   7 [n, isCompact] -> [count]
//   8 [seed, target, mN, (nat)*, eN, (nat)*] -> [kN, (out)*]
//     nat = [name, coordP, x, y, flagP, (flag)?]
//     out = [name, coordP, x, y, flagP, (flag)?, id]

struct Cur<'a>(&'a [f64], usize);
impl<'a> Cur<'a> {
    fn f(&mut self) -> f64 {
        let v = self.0[self.1];
        self.1 += 1;
        v
    }
    fn u(&mut self) -> usize {
        self.f() as usize
    }
    fn string(&mut self) -> String {
        let len = self.u();
        let units: Vec<u16> = (0..len).map(|_| self.f() as u16).collect();
        String::from_utf16_lossy(&units)
    }
    fn nation(&mut self) -> NcNation {
        let name = self.string();
        let coord = if self.u() == 1 {
            let x = self.f();
            let y = self.f();
            Some((x, y))
        } else {
            self.f();
            self.f();
            None
        };
        let flag = if self.u() == 1 { Some(self.string()) } else { None };
        NcNation { name, coord, flag }
    }
}

fn push_string(out: &mut Vec<f64>, s: &str) {
    let units: Vec<u16> = s.encode_utf16().collect();
    out.push(units.len() as f64);
    out.extend(units.iter().map(|&u| u as f64));
}

fn push_nation(out: &mut Vec<f64>, name: &str, coord: Option<(f64, f64)>, flag: &Option<String>) {
    push_string(out, name);
    match coord {
        Some((x, y)) => {
            out.push(1.0);
            out.push(x);
            out.push(y);
        }
        None => {
            out.push(0.0);
            out.push(0.0);
            out.push(0.0);
        }
    }
    match flag {
        Some(f) => {
            out.push(1.0);
            push_string(out, f);
        }
        None => out.push(0.0),
    }
}

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut c = Cur(args, 0);
    match kind {
        0 => {
            out.push(NAME_TEMPLATES.len() as f64);
            for tpl in NAME_TEMPLATES {
                out.push(tpl.len() as f64);
                for part in tpl {
                    match part {
                        TplPart::Lit(s) => {
                            out.push(0.0);
                            push_string(&mut out, s);
                        }
                        TplPart::Noun => out.push(1.0),
                        TplPart::PluralNoun => out.push(2.0),
                    }
                }
            }
        }
        1 => {
            out.push(NOUNS.len() as f64);
            for n in NOUNS {
                push_string(&mut out, n);
            }
        }
        2 => {
            out.push(O_TO_OES.len() as f64);
            for w in O_TO_OES {
                push_string(&mut out, w);
            }
        }
        3 => {
            out.push(SPECIAL_PLURALS.len() as f64);
            for (k, v) in SPECIAL_PLURALS {
                push_string(&mut out, k);
                push_string(&mut out, v);
            }
        }
        4 => {
            let noun = c.string();
            push_string(&mut out, &pluralize(&noun));
        }
        5 => {
            let mut rng = PseudoRandom::new(c.f());
            push_string(&mut out, &generate_nation_name(&mut rng));
        }
        6 => {
            let mut rng = PseudoRandom::new(c.f());
            let mut used: HashSet<String> = (0..c.u()).map(|_| c.string()).collect();
            push_string(&mut out, &generate_unique_nation_name(&mut rng, &mut used));
        }
        7 => {
            let n = c.f();
            let compact = c.u() == 1;
            out.push(get_compact_map_nation_count(n, compact));
        }
        8 => {
            let mut rng = PseudoRandom::new(c.f());
            let target = c.u();
            let manifest: Vec<NcNation> = (0..c.u()).map(|_| c.nation()).collect();
            let extras: Vec<NcNation> = (0..c.u()).map(|_| c.nation()).collect();
            let nations = create_random_nations(target, &manifest, &extras, &mut rng);
            out.push(nations.len() as f64);
            for n in &nations {
                push_nation(&mut out, &n.name, n.coord, &n.flag);
                push_string(&mut out, &n.id);
            }
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pluralize_branch_order() {
        // SPECIAL wins over the suffix rules.
        assert_eq!(pluralize("Cactus"), "Cacti");
        assert_eq!(pluralize("Fish"), "Fish");
        assert_eq!(pluralize("Moose"), "Moose");
        assert_eq!(pluralize("Mitochondria"), "Mitochondria");
        // s/ch/sh/x/z -> +es (checked before the y rule).
        assert_eq!(pluralize("Bus"), "Buses");
        assert_eq!(pluralize("Chainsaw"), "Chainsaws");
        assert_eq!(pluralize("Peach"), "Peaches");
        assert_eq!(pluralize("Sphinx"), "Sphinxes");
        assert_eq!(pluralize("Quiz"), "Quizes");
        // consonant + y -> -ies; vowel + y -> +s.
        assert_eq!(pluralize("Puppy"), "Puppies");
        assert_eq!(pluralize("Key"), "Keys");
        assert_eq!(pluralize("Donkey"), "Donkeys");
        // one-character "y": noun[-1] is undefined -> -ies path.
        assert_eq!(pluralize("y"), "ies");
        // O_TO_OES -> +es, after the y rule.
        assert_eq!(pluralize("Potato"), "Potatoes");
        assert_eq!(pluralize("Volcano"), "Volcanoes");
        // plain +s, including the non-ASCII tail.
        assert_eq!(pluralize("Snail"), "Snails");
        assert_eq!(pluralize("Souffl\u{e9}"), "Souffl\u{e9}s");
    }

    #[test]
    fn generate_name_is_deterministic() {
        let mut a = PseudoRandom::new(42.0);
        let mut b = PseudoRandom::new(42.0);
        for _ in 0..5 {
            assert_eq!(generate_nation_name(&mut a), generate_nation_name(&mut b));
        }
    }

    #[test]
    fn unique_name_skips_used() {
        let mut rng = PseudoRandom::new(7.0);
        let mut used = HashSet::new();
        let n1 = generate_unique_nation_name(&mut rng, &mut used);
        used.insert(n1.clone());
        let n2 = generate_unique_nation_name(&mut rng, &mut used);
        assert_ne!(n1, n2);
    }

    #[test]
    fn unique_name_fallback_counter() {
        // Saturate the retry space by marking every generated name used;
        // the fallback must produce `base 1`.
        let mut rng = PseudoRandom::new(3.0);
        let mut used = HashSet::new();
        // Force 1000 collisions: pre-mark the names the loop will draw.
        let mut probe = PseudoRandom::new(3.0);
        for _ in 0..1000 {
            used.insert(generate_nation_name(&mut probe));
        }
        let got = generate_unique_nation_name(&mut rng, &mut used);
        assert!(got.ends_with(" 1"));
    }

    #[test]
    fn compact_count() {
        assert_eq!(get_compact_map_nation_count(0.0, true), 0.0);
        assert_eq!(get_compact_map_nation_count(0.0, false), 0.0);
        assert_eq!(get_compact_map_nation_count(3.0, true), 1.0);
        assert_eq!(get_compact_map_nation_count(4.0, true), 1.0);
        assert_eq!(get_compact_map_nation_count(9.0, true), 2.0);
        assert_eq!(get_compact_map_nation_count(9.0, false), 9.0);
    }

    #[test]
    fn run_op_pluralize_roundtrip() {
        let mut args = vec![6.0];
        args.extend("Potato".encode_utf16().map(|u| u as f64));
        let res = run_op(4, &args);
        let len = res[0] as usize;
        let s = String::from_utf16_lossy(&res[1..1 + len].iter().map(|&u| u as u16).collect::<Vec<_>>());
        assert_eq!(s, "Potatoes");
    }
}
