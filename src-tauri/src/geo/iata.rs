//! Common IATA airport codes → (city, country, lat, lon).
//! Used when reverse DNS embeds airport codes (e.g. `lax1.example.net`).

use std::collections::HashMap;
use std::sync::OnceLock;

pub struct City {
    pub city: &'static str,
    pub country: &'static str,
    pub lat: f64,
    pub lon: f64,
}

pub fn lookup(code: &str) -> Option<&'static City> {
    table().get(&code.to_ascii_uppercase()).copied()
}

fn table() -> &'static HashMap<String, &'static City> {
    static TABLE: OnceLock<HashMap<String, &'static City>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut m = HashMap::new();
        for c in CITIES {
            m.insert(c.0.to_string(), &c.1);
        }
        m
    })
}

// (IATA, City)
static CITIES: &[(&str, City)] = &[
    ("AMS", City { city: "Amsterdam", country: "NL", lat: 52.31, lon: 4.77 }),
    ("ARN", City { city: "Stockholm", country: "SE", lat: 59.65, lon: 17.92 }),
    ("ATL", City { city: "Atlanta", country: "US", lat: 33.64, lon: -84.43 }),
    ("BCN", City { city: "Barcelona", country: "ES", lat: 41.30, lon: 2.08 }),
    ("BER", City { city: "Berlin", country: "DE", lat: 52.37, lon: 13.50 }),
    ("BKK", City { city: "Bangkok", country: "TH", lat: 13.69, lon: 100.75 }),
    ("BNA", City { city: "Nashville", country: "US", lat: 36.12, lon: -86.68 }),
    ("BOS", City { city: "Boston", country: "US", lat: 42.36, lon: -71.01 }),
    ("BRU", City { city: "Brussels", country: "BE", lat: 50.90, lon: 4.48 }),
    ("BUD", City { city: "Budapest", country: "HU", lat: 47.43, lon: 19.26 }),
    ("CDG", City { city: "Paris", country: "FR", lat: 49.01, lon: 2.55 }),
    ("CGK", City { city: "Jakarta", country: "ID", lat: -6.13, lon: 106.66 }),
    ("CMH", City { city: "Columbus", country: "US", lat: 39.99, lon: -82.89 }),
    ("CPH", City { city: "Copenhagen", country: "DK", lat: 55.62, lon: 12.65 }),
    ("DAL", City { city: "Dallas", country: "US", lat: 32.85, lon: -96.85 }),
    ("DEN", City { city: "Denver", country: "US", lat: 39.86, lon: -104.67 }),
    ("DFW", City { city: "Dallas", country: "US", lat: 32.90, lon: -97.04 }),
    ("DME", City { city: "Moscow", country: "RU", lat: 55.41, lon: 37.91 }),
    ("DOH", City { city: "Doha", country: "QA", lat: 25.27, lon: 51.61 }),
    ("DTW", City { city: "Detroit", country: "US", lat: 42.21, lon: -83.35 }),
    ("DUB", City { city: "Dublin", country: "IE", lat: 53.42, lon: -6.27 }),
    ("DUS", City { city: "Düsseldorf", country: "DE", lat: 51.28, lon: 6.77 }),
    ("EWR", City { city: "Newark", country: "US", lat: 40.69, lon: -74.17 }),
    ("FCO", City { city: "Rome", country: "IT", lat: 41.80, lon: 12.25 }),
    ("FRA", City { city: "Frankfurt", country: "DE", lat: 50.04, lon: 8.56 }),
    ("GIG", City { city: "Rio de Janeiro", country: "BR", lat: -22.81, lon: -43.25 }),
    ("GRU", City { city: "São Paulo", country: "BR", lat: -23.43, lon: -46.47 }),
    ("HAM", City { city: "Hamburg", country: "DE", lat: 53.63, lon: 9.99 }),
    ("HEL", City { city: "Helsinki", country: "FI", lat: 60.32, lon: 24.96 }),
    ("HKG", City { city: "Hong Kong", country: "HK", lat: 22.31, lon: 113.91 }),
    ("HND", City { city: "Tokyo", country: "JP", lat: 35.55, lon: 139.78 }),
    ("IAD", City { city: "Ashburn", country: "US", lat: 38.94, lon: -77.46 }),
    ("IAH", City { city: "Houston", country: "US", lat: 29.98, lon: -95.34 }),
    ("ICN", City { city: "Seoul", country: "KR", lat: 37.46, lon: 126.44 }),
    ("IST", City { city: "Istanbul", country: "TR", lat: 41.28, lon: 28.75 }),
    ("JFK", City { city: "New York", country: "US", lat: 40.64, lon: -73.78 }),
    ("KIX", City { city: "Osaka", country: "JP", lat: 34.43, lon: 135.24 }),
    ("LAS", City { city: "Las Vegas", country: "US", lat: 36.08, lon: -115.15 }),
    ("LAX", City { city: "Los Angeles", country: "US", lat: 33.94, lon: -118.41 }),
    ("LGA", City { city: "New York", country: "US", lat: 40.78, lon: -73.87 }),
    ("LHR", City { city: "London", country: "GB", lat: 51.47, lon: -0.46 }),
    ("LIS", City { city: "Lisbon", country: "PT", lat: 38.78, lon: -9.14 }),
    ("MAD", City { city: "Madrid", country: "ES", lat: 40.49, lon: -3.57 }),
    ("MAN", City { city: "Manchester", country: "GB", lat: 53.35, lon: -2.27 }),
    ("MCI", City { city: "Kansas City", country: "US", lat: 39.30, lon: -94.71 }),
    ("MCO", City { city: "Orlando", country: "US", lat: 28.43, lon: -81.31 }),
    ("MEL", City { city: "Melbourne", country: "AU", lat: -37.67, lon: 144.84 }),
    ("MIA", City { city: "Miami", country: "US", lat: 25.80, lon: -80.29 }),
    ("MIL", City { city: "Milan", country: "IT", lat: 45.63, lon: 8.72 }),
    ("MUC", City { city: "Munich", country: "DE", lat: 48.35, lon: 11.79 }),
    ("MXP", City { city: "Milan", country: "IT", lat: 45.63, lon: 8.72 }),
    ("NRT", City { city: "Tokyo", country: "JP", lat: 35.77, lon: 140.39 }),
    ("NYC", City { city: "New York", country: "US", lat: 40.71, lon: -74.01 }),
    ("ORD", City { city: "Chicago", country: "US", lat: 41.98, lon: -87.90 }),
    ("OSL", City { city: "Oslo", country: "NO", lat: 60.19, lon: 11.10 }),
    ("PAR", City { city: "Paris", country: "FR", lat: 48.86, lon: 2.35 }),
    ("PDX", City { city: "Portland", country: "US", lat: 45.59, lon: -122.60 }),
    ("PEK", City { city: "Beijing", country: "CN", lat: 40.08, lon: 116.58 }),
    ("PHL", City { city: "Philadelphia", country: "US", lat: 39.87, lon: -75.24 }),
    ("PHX", City { city: "Phoenix", country: "US", lat: 33.43, lon: -112.01 }),
    ("PRG", City { city: "Prague", country: "CZ", lat: 50.10, lon: 14.26 }),
    ("PVG", City { city: "Shanghai", country: "CN", lat: 31.14, lon: 121.81 }),
    ("RDU", City { city: "Raleigh", country: "US", lat: 35.88, lon: -78.79 }),
    ("SEA", City { city: "Seattle", country: "US", lat: 47.45, lon: -122.31 }),
    ("SFO", City { city: "San Francisco", country: "US", lat: 37.62, lon: -122.38 }),
    ("SIN", City { city: "Singapore", country: "SG", lat: 1.36, lon: 103.99 }),
    ("SJC", City { city: "San Jose", country: "US", lat: 37.36, lon: -121.93 }),
    ("SLC", City { city: "Salt Lake City", country: "US", lat: 40.79, lon: -111.98 }),
    ("SOF", City { city: "Sofia", country: "BG", lat: 42.70, lon: 23.41 }),
    ("STO", City { city: "Stockholm", country: "SE", lat: 59.33, lon: 18.07 }),
    ("SVO", City { city: "Moscow", country: "RU", lat: 55.97, lon: 37.41 }),
    ("SYD", City { city: "Sydney", country: "AU", lat: -33.95, lon: 151.18 }),
    ("TPE", City { city: "Taipei", country: "TW", lat: 25.08, lon: 121.23 }),
    ("TXL", City { city: "Berlin", country: "DE", lat: 52.56, lon: 13.29 }),
    ("VIE", City { city: "Vienna", country: "AT", lat: 48.11, lon: 16.57 }),
    ("WAW", City { city: "Warsaw", country: "PL", lat: 52.17, lon: 20.97 }),
    ("YUL", City { city: "Montreal", country: "CA", lat: 45.47, lon: -73.74 }),
    ("YVR", City { city: "Vancouver", country: "CA", lat: 49.19, lon: -123.18 }),
    ("YYZ", City { city: "Toronto", country: "CA", lat: 43.68, lon: -79.63 }),
    ("ZRH", City { city: "Zurich", country: "CH", lat: 47.46, lon: 8.55 }),
    // Common metro/IX shorthand seen in PTR records
    ("ASH", City { city: "Ashburn", country: "US", lat: 39.04, lon: -77.49 }),
    ("EQX", City { city: "Ashburn", country: "US", lat: 39.04, lon: -77.49 }),
];
