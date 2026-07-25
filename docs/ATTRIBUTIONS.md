# Attributions

## Earth texture

`ui/public/earth-dark.jpg` is a dark-styled equirectangular Earth basemap used only for visualization in the 3D globe.

If you replace this asset for redistribution, use a basemap with a clear public license, for example:

- [NASA Blue Marble](https://visibleearth.nasa.gov/collection/1484/blue-marble) derivatives (NASA imagery; check current NASA media guidelines)
- Other public-domain or Creative Commons equirectangular maps

Do not redistribute MaxMind GeoLite2 databases with Network Cartographer; they have a separate license and must be obtained by each user (see `scripts/update-geolite2.sh`).

## Libraries

Network Cartographer depends on open-source packages declared in `package.json` and `src-tauri/Cargo.toml`, including Tauri, Three.js / globe.gl, and various Rust crates. Their licenses apply as usual.
