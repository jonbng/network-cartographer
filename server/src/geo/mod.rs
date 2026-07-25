mod asn;
mod iata;
mod infer;
mod lookup;
mod path_cache;
mod rdns;

pub use asn::AsnDb;
pub use infer::{pending_ips, GeoHop};
pub use lookup::GeoCache;
pub use path_cache::PathGeoCache;

fn debug(message: impl std::fmt::Display) {
    let enabled = std::env::var("NETCART_DEBUG")
        .map(|value| !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false"))
        .unwrap_or(false);
    if enabled {
        eprintln!("  Debug      {message}");
    }
}
