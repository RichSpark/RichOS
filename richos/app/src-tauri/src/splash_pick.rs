//! Choose the launch design using the gallery catalog and the existing launch rules.

/// `splash.js`'s `SPLASH_SECONDS` and `MAX_SPLASH_SECONDS` (held equal by a test below).
pub const DEFAULT_SECONDS: f64 = 3.0;
pub const MAX_SECONDS: f64 = 5.0;
/// The library the page draws from, compiled in. Its own test (`ui/tests/splash.js` check 1)
/// holds it to plain data after the assignment, which is what lets this read it as JSON.
const LIBRARY: &str = include_str!("../../ui/splash-library.js");
/// The material entries the openings' ladder is built on: data, in the library's own shape.
const MATERIALS: &str = include_str!("../../ui/splash-materials.js");

/// One catalog entry, as far as the shell needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: String,
    pub ground: String,
    pub seconds: f64,
}

/// The catalog: the library in its own order, every entry with an id and a ground, then the
/// openings in theirs. `splash.js` validates the rest of each entry; the splash suite holds this
/// list equal to the page's pool.
pub fn catalog() -> Vec<Entry> {
    catalog_from(LIBRARY, MATERIALS, crate::openings::catalog())
}

/// The entries of one data file: everything after `window.<name> =` is JSON, either the list
/// itself or an object holding it under `variations`.
fn entries_of(source: &str, name: &str) -> Vec<Entry> {
    let Some(at) = source.find(name) else { return Vec::new() };
    let rest = &source[at..];
    let Some(eq) = rest.find('=') else { return Vec::new() };
    let json = rest[eq + 1..].trim().trim_end_matches(';').trim();
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else { return Vec::new() };
    let Some(variations) = value.get("variations").unwrap_or(&value).as_array() else { return Vec::new() };
    variations
        .iter()
        .filter_map(|v| {
            let id = v.get("id")?.as_str()?.to_string();
            let tokens = v.get("tokens")?;
            let ground = tokens.get("ground")?.as_str()?.to_string();
            parse_hex(&ground)?;
            let seconds = match tokens.get("seconds") {
                Some(serde_json::Value::String(s)) => s.parse::<f64>().ok().filter(|n| n.is_finite() && *n > 0.0),
                _ => None,
            }
            .map(|n| n.min(MAX_SECONDS))
            .unwrap_or(DEFAULT_SECONDS);
            Some(Entry { id, ground, seconds })
        })
        .collect()
}

fn catalog_from(library: &str, materials: &str, openings: &richos_core::openings::Catalog) -> Vec<Entry> {
    let mut entries = entries_of(library, "window.RichSplashLibrary");
    let mut sources = entries.clone();
    sources.extend(entries_of(materials, "window.RichSplashMaterials"));
    for opening in &openings.openings {
        let Some(from) = openings.base_of(opening).and_then(|base| sources.iter().find(|s| s.id == base)) else { continue };
        entries.push(Entry {
            id: richos_core::openings::key(&opening.id),
            ground: from.ground.clone(),
            seconds: openings.seconds_of(opening).map(|n| n.min(MAX_SECONDS)).unwrap_or(from.seconds),
        });
    }
    entries
}

/// `#RRGGBB` as three bytes.
pub fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.strip_prefix('#')?;
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some((byte(0)?, byte(2)?, byte(4)?))
}

/// THE PICK for a start, by the rules, from the catalog: `facts` is everything the rules read
/// (which start it is for, the user's own pick, what he has, the day).
pub fn pick_for(facts: &richos_core::launch::SplashFacts) -> Option<Entry> {
    let entries = catalog();
    let ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
    let id = richos_core::launch::pick_splash(&ids, facts)?.to_string();
    entries.into_iter().find(|e| e.id == id)
}


impl Entry {
    pub fn pack_id(&self) -> Option<&str> {
        match self.id.as_str() {
            "round-11/v1" => Some("ruled-standard"),
            "round-11/v2" => Some("midnight-suede"),
            id => id.strip_prefix(richos_core::openings::KEY_PREFIX),
        }
    }
}

pub fn default_entry() -> Option<Entry> {
    catalog().into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use richos_core::launch::SplashFacts;

    #[test]
    fn the_same_launch_rule_selects_both_starters_and_earned_milestones() {
        for (ordinal, pack) in [(1, "ruled-standard"), (2, "midnight-suede"), (3, "ruled-standard")] {
            let entry = pick_for(&SplashFacts { start: Some(ordinal), ..Default::default() }).unwrap();
            assert_eq!((entry.pack_id(), entry.seconds), (Some(pack), 3.0));
        }
        let facts = SplashFacts {
            pick: Some("power-100".into()),
            earned: vec!["power-100".into()],
            ..Default::default()
        };
        let entry = pick_for(&facts).unwrap();
        assert_eq!((entry.pack_id(), entry.seconds), (Some("power-100"), 5.0));
    }
}
