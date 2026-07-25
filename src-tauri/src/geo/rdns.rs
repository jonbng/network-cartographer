//! Infer location hints from reverse-DNS hostnames.

use super::iata;
use super::lookup::GeoHint;

/// Pull city / IATA-style hints from a PTR hostname.
pub fn parse_hostname(host: &str) -> Option<GeoHint> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }

    // Tokenize on non-alphanumeric
    let tokens: Vec<&str> = host
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() >= 3)
        .collect();

    // Prefer standalone 3-letter IATA tokens
    for t in &tokens {
        if t.len() == 3 {
            if let Some(city) = iata::lookup(t) {
                return Some(GeoHint {
                    city: city.city.to_string(),
                    country: city.country.to_string(),
                    lat: city.lat,
                    lon: city.lon,
                    source: "rdns-iata".into(),
                    confidence: 0.82,
                });
            }
        }
    }

    // Codes embedded like "lax1", "sfo2", "ord-core"
    for t in &tokens {
        if t.len() >= 3 {
            let prefix = &t[..3];
            if prefix.chars().all(|c| c.is_ascii_alphabetic()) {
                if let Some(city) = iata::lookup(prefix) {
                    // Avoid false positives on common words
                    if !COMMON_FALSE_POSITIVES.contains(&prefix) {
                        return Some(GeoHint {
                            city: city.city.to_string(),
                            country: city.country.to_string(),
                            lat: city.lat,
                            lon: city.lon,
                            source: "rdns-iata-embed".into(),
                            confidence: 0.72,
                        });
                    }
                }
            }
        }
    }

    // City name substrings
    for (needle, city, country, lat, lon) in CITY_NAMES {
        if host.contains(needle) {
            return Some(GeoHint {
                city: (*city).to_string(),
                country: (*country).to_string(),
                lat: *lat,
                lon: *lon,
                source: "rdns-city".into(),
                confidence: 0.68,
            });
        }
    }

    None
}

const COMMON_FALSE_POSITIVES: &[&str] = &[
    "net", "com", "org", "cdn", "pop", "gw", "ae", "xe", "ge", "te", "be", "core",
    "edge", "node", "host", "ptr", "ip", "ipv", "v6", "eth", "lag", "bb", "as",
];

const CITY_NAMES: &[(&str, &str, &str, f64, f64)] = &[
    ("london", "London", "GB", 51.51, -0.13),
    ("paris", "Paris", "FR", 48.86, 2.35),
    ("frankfurt", "Frankfurt", "DE", 50.11, 8.68),
    ("amsterdam", "Amsterdam", "NL", 52.37, 4.90),
    ("singapore", "Singapore", "SG", 1.35, 103.82),
    ("tokyo", "Tokyo", "JP", 35.68, 139.69),
    ("sydney", "Sydney", "AU", -33.87, 151.21),
    ("seattle", "Seattle", "US", 47.61, -122.33),
    ("chicago", "Chicago", "US", 41.88, -87.63),
    ("dallas", "Dallas", "US", 32.78, -96.80),
    ("miami", "Miami", "US", 25.76, -80.19),
    ("ashburn", "Ashburn", "US", 39.04, -77.49),
    ("newyork", "New York", "US", 40.71, -74.01),
    ("new-york", "New York", "US", 40.71, -74.01),
    ("losangeles", "Los Angeles", "US", 34.05, -118.24),
    ("los-angeles", "Los Angeles", "US", 34.05, -118.24),
    ("sanfrancisco", "San Francisco", "US", 37.77, -122.42),
    ("san-francisco", "San Francisco", "US", 37.77, -122.42),
    ("hongkong", "Hong Kong", "HK", 22.32, 114.17),
    ("hong-kong", "Hong Kong", "HK", 22.32, 114.17),
    ("stockholm", "Stockholm", "SE", 59.33, 18.07),
    ("copenhagen", "Copenhagen", "DK", 55.68, 12.57),
    ("madrid", "Madrid", "ES", 40.42, -3.70),
    ("milan", "Milan", "IT", 45.46, 9.19),
    ("warsaw", "Warsaw", "PL", 52.23, 21.01),
    ("prague", "Prague", "CZ", 50.08, 14.44),
    ("vienna", "Vienna", "AT", 48.21, 16.37),
    ("zurich", "Zurich", "CH", 47.38, 8.54),
    ("dublin", "Dublin", "IE", 53.35, -6.26),
    ("toronto", "Toronto", "CA", 43.65, -79.38),
    ("montreal", "Montreal", "CA", 45.50, -73.57),
    ("vancouver", "Vancouver", "CA", 49.28, -123.12),
    ("sao-paulo", "São Paulo", "BR", -23.55, -46.63),
    ("saopaulo", "São Paulo", "BR", -23.55, -46.63),
];
