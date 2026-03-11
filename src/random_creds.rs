//! Random credential generation for initial config setup

use rand::seq::SliceRandom;
use rand::Rng;

/// Word lists for generating memorable usernames (adjective-color-noun pattern)
const ADJECTIVES: &[&str] = &[
    // Positive traits
    "Happy", "Clever", "Swift", "Brave", "Calm", "Bright", "Keen", "Bold", "Cool", "Fair", "Fond",
    "Glad", "Kind", "Mild", "Neat", "Nice", "Quick", "Safe", "Warm", "Wise", "Crisp", "Fresh",
    "Grand", "Prime", "Proud", "Sharp", "Smart", "Snug", "Spry", "Tidy", "Trim", "Vivid",
    // More adjectives
    "Agile", "Alert", "Ample", "Apt", "Astute", "Avid", "Aware", "Blest", "Brisk", "Busy", "Candid",
    "Casual", "Chipper", "Civil", "Clean", "Clear", "Comfy", "Cozy", "Dapper", "Deft", "Dense",
    "Eager", "Easy", "Elite", "Even", "Exact", "Expert", "Famous", "Fancy", "Fast", "Fine", "Firm",
    "Fleet", "Fluent", "Frank", "Free", "Gentle", "Gifted", "Glib", "Glossy", "Golden", "Good",
    "Graceful", "Great", "Green", "Handy", "Hardy", "Hasty", "Hearty", "Heavy", "Honest", "Humble",
    "Ideal", "Jolly", "Jovial", "Joyful", "Just", "Large", "Late", "Lavish", "Lean", "Legal",
    "Light", "Lively", "Local", "Logical", "Long", "Loose", "Loud", "Lovely", "Lucky", "Lusty",
    "Major", "Mature", "Mellow", "Merry", "Minor", "Modern", "Modest", "Moral", "Mutual", "Native",
    "Natural", "Naval", "Noble", "Normal", "Novel", "Open", "Optimal", "Oral", "Organic", "Outer",
    "Pale", "Parallel", "Patient", "Plain", "Plucky", "Plus", "Polite", "Popular", "Portable",
    "Positive", "Precious", "Pretty", "Prompt", "Proper", "Pure", "Quiet", "Radiant", "Rapid",
    "Rare", "Ready", "Real", "Regular", "Remote", "Rich", "Right", "Rigid", "Robust", "Rough",
    "Round", "Royal", "Rural", "Sacred", "Savvy", "Secure", "Senior", "Serene", "Shiny", "Short",
    "Silent", "Silver", "Simple", "Sleek", "Slick", "Slim", "Slow", "Small", "Smooth", "Snappy",
    "Social", "Soft", "Solid", "Sound", "Spare", "Special", "Speedy", "Stable", "Stark", "Steady",
    "Steep", "Still", "Strong", "Sturdy", "Subtle", "Super", "Sure", "Sweet", "Tacit", "Tall",
    "Tender", "Tense", "Thick", "Thin", "Thorough", "Tight", "Tiny", "Topaz", "Total", "Tough",
    "Tranquil", "True", "Trusty", "Upper", "Urban", "Urgent", "Useful", "Usual", "Valid", "Vast",
    "Verbal", "Vital", "Vocal", "Warm", "Wary", "Wealthy", "Weekly", "Weird", "Whole", "Wide",
    "Wild", "Witty", "Worthy", "Young", "Zany", "Zealous", "Zesty", "Zippy",
];

const NOUNS: &[&str] = &[
    // Animals - mammals
    "Fox",
    "Owl",
    "Bear",
    "Wolf",
    "Hawk",
    "Deer",
    "Hare",
    "Seal",
    "Crow",
    "Duck",
    "Frog",
    "Goat",
    "Lamb",
    "Lynx",
    "Mole",
    "Moth",
    "Newt",
    "Orca",
    "Puma",
    "Swan",
    "Toad",
    "Vole",
    "Wren",
    "Yak",
    "Bass",
    "Carp",
    "Crab",
    "Dove",
    "Elk",
    "Gull",
    "Jay",
    "Kite",
    "Lark",
    "Pike",
    "Slug",
    "Wasp",
    "Ray",
    "Ant",
    "Bat",
    "Bee",
    "Cod",
    "Eel",
    "Fly",
    "Hen",
    "Asp",
    "Ape",
    "Cat",
    "Dog",
    "Lion",
    "Tiger",
    "Panda",
    "Koala",
    "Sloth",
    "Otter",
    "Badger",
    "Ferret",
    "Stoat",
    "Mouse",
    "Shrew",
    "Lemur",
    "Tapir",
    "Bison",
    "Moose",
    "Camel",
    "Llama",
    "Zebra",
    "Horse",
    "Donkey",
    "Sheep",
    "Boar",
    "Stag",
    "Hound",
    // Birds
    "Eagle",
    "Raven",
    "Robin",
    "Finch",
    "Crane",
    "Heron",
    "Stork",
    "Ibis",
    "Egret",
    "Harpy",
    "Swift",
    "Martin",
    "Pipit",
    "Vireo",
    "Tanager",
    "Oriole",
    "Parrot",
    "Macaw",
    "Falcon",
    "Osprey",
    "Condor",
    "Vulture",
    "Pelican",
    "Albatross",
    // Fish & sea creatures
    "Shark",
    "Whale",
    "Squid",
    "Clam",
    "Prawn",
    "Shrimp",
    "Lobster",
    "Oyster",
    "Trout",
    "Salmon",
    "Perch",
    "Bream",
    "Sole",
    "Tuna",
    "Marlin",
    "Swordfish",
    // Insects
    "Cricket",
    "Beetle",
    "Mantis",
    "Cicada",
    "Firefly",
    "Hornet",
    "Ladybug",
    "Dragonfly",
    // Mythical/fun
    "Dragon",
    "Phoenix",
    "Griffin",
    "Sphinx",
    "Hydra",
    "Kraken",
    "Titan",
    "Golem",
    // Objects & nature
    "Acorn",
    "Anchor",
    "Arrow",
    "Aspen",
    "Atlas",
    "Atom",
    "Badge",
    "Beacon",
    "Bolt",
    "Brick",
    "Bridge",
    "Brook",
    "Bucket",
    "Cable",
    "Canyon",
    "Castle",
    "Cedar",
    "Chalk",
    "Chest",
    "Cliff",
    "Cloud",
    "Comet",
    "Compass",
    "Coral",
    "Creek",
    "Crest",
    "Crown",
    "Crystal",
    "Delta",
    "Dune",
    "Echo",
    "Ember",
    "Fern",
    "Flame",
    "Flint",
    "Flower",
    "Forest",
    "Forge",
    "Frost",
    "Galaxy",
    "Geyser",
    "Glacier",
    "Globe",
    "Grove",
    "Harbor",
    "Helm",
    "Horizon",
    "Island",
    "Jewel",
    "Kernel",
    "Lantern",
    "Lattice",
    "Leaf",
    "Ledge",
    "Lens",
    "Lily",
    "Lotus",
    "Maple",
    "Marble",
    "Meadow",
    "Mesa",
    "Meteor",
    "Mirror",
    "Mist",
    "Moon",
    "Moss",
    "Nebula",
    "Nest",
    "Nova",
    "Oak",
    "Oasis",
    "Ocean",
    "Orbit",
    "Orchid",
    "Peak",
    "Pearl",
    "Pebble",
    "Pine",
    "Planet",
    "Plaza",
    "Plume",
    "Pond",
    "Portal",
    "Prism",
    "Pulsar",
    "Quartz",
    "Quest",
    "Rain",
    "Ranch",
    "Reef",
    "Ridge",
    "River",
    "Rock",
    "Root",
    "Sage",
    "Sail",
    "Sand",
    "Seed",
    "Shell",
    "Shield",
    "Shore",
    "Sierra",
    "Silk",
    "Sky",
    "Slate",
    "Snow",
    "Solar",
    "Spark",
    "Spire",
    "Spring",
    "Spruce",
    "Star",
    "Steam",
    "Steel",
    "Stone",
    "Storm",
    "Stream",
    "Summit",
    "Sun",
    "Terrain",
    "Thunder",
    "Tide",
    "Timber",
    "Torch",
    "Tower",
    "Trail",
    "Tree",
    "Tulip",
    "Tundra",
    "Valley",
    "Vapor",
    "Vault",
    "Velvet",
    "Vertex",
    "Vine",
    "Vista",
    "Vortex",
    "Wave",
    "Willow",
    "Wind",
    "Wing",
    "Wood",
    "Zenith",
    "Zephyr",
];

const COLORS: &[&str] = &[
    // Basic colors
    "Red",
    "Blue",
    "Gold",
    "Jade",
    "Ruby",
    "Teal",
    "Navy",
    "Mint",
    "Rose",
    "Sage",
    "Plum",
    "Wine",
    "Lime",
    "Coal",
    "Snow",
    "Sand",
    "Rust",
    "Fern",
    "Clay",
    "Bark",
    "Moss",
    "Leaf",
    "Dusk",
    "Dawn",
    // Extended palette
    "Amber",
    "Aqua",
    "Azure",
    "Beige",
    "Black",
    "Blush",
    "Bone",
    "Brass",
    "Bronze",
    "Brown",
    "Buff",
    "Burgundy",
    "Camel",
    "Canary",
    "Candy",
    "Caramel",
    "Carmine",
    "Cedar",
    "Celery",
    "Cerise",
    "Charcoal",
    "Cherry",
    "Chestnut",
    "Cinnamon",
    "Citron",
    "Claret",
    "Cobalt",
    "Cocoa",
    "Coffee",
    "Copper",
    "Coral",
    "Corn",
    "Cream",
    "Crimson",
    "Cyan",
    "Ebony",
    "Ecru",
    "Eggplant",
    "Emerald",
    "Fawn",
    "Flax",
    "Forest",
    "Fuchsia",
    "Garnet",
    "Ginger",
    "Grape",
    "Gray",
    "Green",
    "Hazel",
    "Honey",
    "Indigo",
    "Iron",
    "Ivory",
    "Jet",
    "Khaki",
    "Lapis",
    "Lava",
    "Lavender",
    "Lemon",
    "Lilac",
    "Linen",
    "Magenta",
    "Mahogany",
    "Mango",
    "Maple",
    "Maroon",
    "Mauve",
    "Melon",
    "Midnight",
    "Mocha",
    "Mulberry",
    "Mustard",
    "Nutmeg",
    "Ocher",
    "Olive",
    "Onyx",
    "Orange",
    "Orchid",
    "Oyster",
    "Papaya",
    "Peach",
    "Pearl",
    "Periwinkle",
    "Pine",
    "Pink",
    "Pistachio",
    "Platinum",
    "Poppy",
    "Purple",
    "Quartz",
    "Raisin",
    "Raspberry",
    "Raven",
    "Salmon",
    "Sapphire",
    "Scarlet",
    "Sepia",
    "Sienna",
    "Silver",
    "Slate",
    "Smoke",
    "Steel",
    "Stone",
    "Straw",
    "Sunset",
    "Tan",
    "Tangerine",
    "Taupe",
    "Thistle",
    "Tiger",
    "Tomato",
    "Topaz",
    "Turquoise",
    "Umber",
    "Vanilla",
    "Vermilion",
    "Violet",
    "Wheat",
    "White",
    "Yellow",
];

/// Generate a memorable username from three random words
/// Pattern: AdjectiveColorNoun (e.g., "CleverJadeFox")
pub fn generate_username() -> String {
    let mut rng = rand::thread_rng();

    let adjective = ADJECTIVES.choose(&mut rng).unwrap();
    let color = COLORS.choose(&mut rng).unwrap();
    let noun = NOUNS.choose(&mut rng).unwrap();

    format!("{adjective}{color}{noun}")
}

/// Generate a random password with good entropy
/// Uses a mix of letters, numbers, and symbols
pub fn generate_password(length: usize) -> String {
    const CHARSET: &[u8] = b"abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789!@#$%&*";
    let mut rng = rand::thread_rng();

    (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_username_is_pascalcase() {
        let username = generate_username();
        // Should start with uppercase and have no separators
        assert!(username.chars().next().unwrap().is_uppercase());
        assert!(!username.contains('-'));
        assert!(!username.contains('_'));
        assert!(!username.contains(' '));
    }

    #[test]
    fn test_username_has_three_words() {
        // Generate multiple to ensure pattern holds
        for _ in 0..10 {
            let username = generate_username();
            // Count uppercase letters (starts of words)
            let uppercase_count = username.chars().filter(|c| c.is_uppercase()).count();
            assert_eq!(
                uppercase_count, 3,
                "Username should have exactly 3 PascalCase words"
            );
        }
    }

    #[test]
    fn test_username_uniqueness() {
        let u1 = generate_username();
        let u2 = generate_username();
        // This could theoretically fail, but with large word lists it's unlikely
        assert_ne!(u1, u2, "Usernames should be unique");
    }

    #[test]
    fn test_username_reasonable_length() {
        for _ in 0..10 {
            let username = generate_username();
            // Min: 3 letter words = 9, Max: ~8 letter words = 24
            assert!(username.len() >= 6, "Username too short: {}", username);
            assert!(username.len() <= 30, "Username too long: {}", username);
        }
    }

    #[test]
    fn test_password_length() {
        let password = generate_password(16);
        assert_eq!(password.len(), 16);
    }

    #[test]
    fn test_password_various_lengths() {
        assert_eq!(generate_password(8).len(), 8);
        assert_eq!(generate_password(12).len(), 12);
        assert_eq!(generate_password(20).len(), 20);
        assert_eq!(generate_password(32).len(), 32);
    }

    #[test]
    fn test_password_zero_length() {
        let password = generate_password(0);
        assert!(password.is_empty());
    }

    #[test]
    fn test_password_uniqueness() {
        let p1 = generate_password(16);
        let p2 = generate_password(16);
        assert_ne!(p1, p2, "Passwords should be unique");
    }

    #[test]
    fn test_password_character_set() {
        // Password should only contain allowed characters
        let allowed = "abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789!@#$%&*";
        for _ in 0..10 {
            let password = generate_password(20);
            for c in password.chars() {
                assert!(allowed.contains(c), "Invalid character in password: {}", c);
            }
        }
    }

    #[test]
    fn test_password_no_ambiguous_chars() {
        // Should not contain: l, I, O, 0, 1 (easily confused)
        let ambiguous = "lIO01";
        for _ in 0..20 {
            let password = generate_password(50);
            for c in ambiguous.chars() {
                assert!(
                    !password.contains(c),
                    "Password contains ambiguous char: {}",
                    c
                );
            }
        }
    }

    #[test]
    fn test_word_lists_not_empty() {
        assert!(!ADJECTIVES.is_empty());
        assert!(!NOUNS.is_empty());
        assert!(!COLORS.is_empty());
    }

    #[test]
    fn test_word_lists_have_variety() {
        // Ensure we have enough variety for good randomness
        assert!(ADJECTIVES.len() >= 50, "Need more adjectives");
        assert!(NOUNS.len() >= 50, "Need more nouns");
        assert!(COLORS.len() >= 20, "Need more colors");
    }
}
