//! City names for workspaces and the random branch suffix.

use std::collections::HashSet;

/// Single word ASCII city names. Each becomes a workspace name and a folder.
pub const CITIES: &[&str] = &[
    "Accra",
    "Adelaide",
    "Algiers",
    "Almaty",
    "Amman",
    "Amsterdam",
    "Ankara",
    "Antwerp",
    "Athens",
    "Auckland",
    "Baku",
    "Bangkok",
    "Barcelona",
    "Beirut",
    "Belgrade",
    "Bergen",
    "Berlin",
    "Bogota",
    "Bologna",
    "Boston",
    "Brasilia",
    "Bratislava",
    "Brisbane",
    "Brussels",
    "Bucharest",
    "Budapest",
    "Cairo",
    "Calgary",
    "Cannes",
    "Cardiff",
    "Casablanca",
    "Chennai",
    "Chicago",
    "Colombo",
    "Copenhagen",
    "Cordoba",
    "Dakar",
    "Dallas",
    "Delhi",
    "Denver",
    "Dhaka",
    "Doha",
    "Dresden",
    "Dublin",
    "Durban",
    "Edinburgh",
    "Florence",
    "Funchal",
    "Geneva",
    "Genoa",
    "Ghent",
    "Glasgow",
    "Gothenburg",
    "Granada",
    "Hamburg",
    "Hanoi",
    "Havana",
    "Helsinki",
    "Hobart",
    "Houston",
    "Istanbul",
    "Jakarta",
    "Jerusalem",
    "Kabul",
    "Kampala",
    "Karachi",
    "Kigali",
    "Kingston",
    "Kobe",
    "Krakow",
    "Kyiv",
    "Kyoto",
    "Lagos",
    "Lahore",
    "Leeds",
    "Lima",
    "Lisbon",
    "Ljubljana",
    "London",
    "Lyon",
    "Madrid",
    "Malaga",
    "Malmo",
    "Manila",
    "Marseille",
    "Medellin",
    "Melbourne",
    "Memphis",
    "Miami",
    "Milan",
    "Minsk",
    "Montreal",
    "Mumbai",
    "Munich",
    "Nairobi",
    "Naples",
    "Nagoya",
    "Nantes",
    "Nice",
    "Oaxaca",
    "Odessa",
    "Orlando",
    "Osaka",
    "Oslo",
    "Ottawa",
    "Oxford",
    "Palermo",
    "Panama",
    "Paris",
    "Perth",
    "Phoenix",
    "Porto",
    "Prague",
    "Quebec",
    "Quito",
    "Reykjavik",
    "Riga",
    "Rome",
    "Rotterdam",
    "Salzburg",
    "Santiago",
    "Sapporo",
    "Seattle",
    "Seoul",
    "Seville",
    "Singapore",
    "Sofia",
    "Stockholm",
    "Stuttgart",
    "Sydney",
    "Taipei",
    "Tallinn",
    "Tangier",
    "Tbilisi",
    "Tehran",
    "Tokyo",
    "Toronto",
    "Tunis",
    "Turin",
    "Utrecht",
    "Valencia",
    "Vancouver",
    "Venice",
    "Vienna",
    "Vilnius",
    "Warsaw",
    "Wellington",
    "Yangon",
    "Yokohama",
    "Zagreb",
    "Zurich",
];

/// Folder name for a city: lowercase, spaces and other characters become dashes.
pub fn city_slug(city: &str) -> String {
    slugify(city)
}

/// Lowercase `[a-z0-9-]`, no repeated or edge dashes. Empty input gives an empty string.
pub fn slugify(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// Picks a city that is not in `used` (compared by slug) and passes `is_free`.
/// `pick(n)` returns a number below `n`. When every city is taken, a numeric suffix is added.
pub fn pick_city(
    used: &HashSet<String>,
    is_free: &dyn Fn(&str) -> bool,
    pick: &mut dyn FnMut(usize) -> usize,
) -> String {
    let taken = |name: &str| used.contains(&city_slug(name)) || !is_free(&city_slug(name));

    let mut fresh: Vec<&str> = CITIES.iter().copied().filter(|c| !taken(c)).collect();
    if !fresh.is_empty() {
        return fresh.swap_remove(pick(fresh.len())).to_string();
    }

    for n in 2.. {
        let mut options: Vec<String> = CITIES
            .iter()
            .map(|c| format!("{c}-{n}"))
            .filter(|c| !taken(c))
            .collect();
        if !options.is_empty() {
            let i = pick(options.len());
            return options.swap_remove(i);
        }
    }
    unreachable!("the numeric suffix loop always finds a free name")
}

const SUFFIX_ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

/// Five random lowercase letters and digits, like `gk0o9`.
pub fn random_suffix(pick: &mut dyn FnMut(usize) -> usize) -> String {
    (0..5)
        .map(|_| SUFFIX_ALPHABET[pick(SUFFIX_ALPHABET.len())] as char)
        .collect()
}

/// Prefix for generated branch names, derived from a GitHub login or git user name.
pub fn branch_prefix(raw: &str) -> String {
    let slug = slugify(raw);
    if slug.is_empty() {
        "circles".to_string()
    } else {
        slug
    }
}

/// Folder name for a project under `~/circles/workspaces`.
pub fn project_dir_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches(|c| c == '-' || c == '.').to_string();
    if cleaned.is_empty() {
        "project".to_string()
    } else {
        cleaned
    }
}

pub fn cities_are_unique_slugs() -> bool {
    let mut seen = HashSet::new();
    CITIES.iter().all(|c| seen.insert(city_slug(c)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first(_n: usize) -> usize {
        0
    }

    #[test]
    fn city_list_is_clean() {
        assert!(cities_are_unique_slugs());
        assert!(CITIES.len() >= 100);
        for c in CITIES {
            assert!(c.chars().all(|ch| ch.is_ascii_alphabetic()), "{c}");
            assert_eq!(city_slug(c), c.to_lowercase());
        }
    }

    #[test]
    fn picks_an_unused_city() {
        let used: HashSet<String> = ["accra".to_string(), "adelaide".to_string()].into();
        let city = pick_city(&used, &|_| true, &mut first);
        assert!(!used.contains(&city_slug(&city)));
        assert!(CITIES.contains(&city.as_str()));
    }

    #[test]
    fn skips_cities_whose_folder_exists() {
        let city = pick_city(&HashSet::new(), &|slug| slug != "accra", &mut first);
        assert_eq!(city, "Adelaide");
    }

    #[test]
    fn adds_a_suffix_when_every_city_is_taken() {
        let used: HashSet<String> = CITIES.iter().map(|c| city_slug(c)).collect();
        let city = pick_city(&used, &|_| true, &mut first);
        assert_eq!(city, "Accra-2");
        assert_eq!(city_slug(&city), "accra-2");

        let mut used2 = used.clone();
        used2.extend(CITIES.iter().map(|c| format!("{}-2", city_slug(c))));
        assert_eq!(pick_city(&used2, &|_| true, &mut first), "Accra-3");
    }

    #[test]
    fn random_choices_cover_the_list() {
        let mut counter = 0;
        let mut seen = HashSet::new();
        for _ in 0..CITIES.len() {
            let mut pick = |n: usize| {
                counter += 7;
                counter % n
            };
            seen.insert(pick_city(&HashSet::new(), &|_| true, &mut pick));
        }
        assert!(seen.len() > 20);
    }

    #[test]
    fn suffix_has_five_safe_chars() {
        let mut n = 0;
        let mut pick = |len: usize| {
            n += 11;
            n % len
        };
        let s = random_suffix(&mut pick);
        assert_eq!(s.len(), 5);
        assert!(s
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }

    #[test]
    fn prefixes_are_branch_safe() {
        assert_eq!(branch_prefix("decimozs"), "decimozs");
        assert_eq!(branch_prefix("Marlon Martin"), "marlon-martin");
        assert_eq!(branch_prefix("  --Weird__Name!! "), "weird-name");
        assert_eq!(branch_prefix("***"), "circles");
        assert_eq!(branch_prefix(""), "circles");
    }

    #[test]
    fn project_dirs_are_path_safe() {
        assert_eq!(project_dir_name("roster"), "roster");
        assert_eq!(project_dir_name("my app/v2"), "my-app-v2");
        assert_eq!(project_dir_name("../evil"), "evil");
        assert_eq!(project_dir_name("..."), "project");
    }
}
