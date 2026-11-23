//! Turns GPS coordinates into a place name ("Reykjavík, Iceland") offline, using the
//! GeoNames list of cities with more than 15,000 people (CC BY 4.0,
//! <https://www.geonames.org>). No outside requests.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::sync::OnceLock;

static CITIES_GZ: &[u8] = include_bytes!("cities.tsv.gz");
static COUNTRIES_TSV: &str = include_str!("countries.tsv");

struct City {
    name: String,
    cc: String,
    lat: f64,
    lon: f64,
}

struct Index {
    grid: HashMap<(i32, i32), Vec<City>>,
    countries: HashMap<String, String>,
}

/// Grid cell size in degrees.
const CELL: f64 = 1.0;

fn load() -> Index {
    let mut grid: HashMap<(i32, i32), Vec<City>> = HashMap::new();
    let mut countries = HashMap::new();
    for l in COUNTRIES_TSV.split('\n') {
        if let Some((k, v)) = l.split_once('\t') {
            countries.insert(k.to_string(), v.to_string());
        }
    }
    let zr = flate2::read::GzDecoder::new(CITIES_GZ);
    for line in BufReader::new(zr).lines() {
        let Ok(line) = line else { break };
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 4 {
            continue;
        }
        let la: f64 = f[2].parse().unwrap_or(0.0);
        let lo: f64 = f[3].parse().unwrap_or(0.0);
        let k = ((la / CELL).floor() as i32, (lo / CELL).floor() as i32);
        grid.entry(k).or_default().push(City {
            name: f[0].to_string(),
            cc: f[1].to_string(),
            lat: la,
            lon: lo,
        });
    }
    Index { grid, countries }
}

fn index() -> &'static Index {
    static IDX: OnceLock<Index> = OnceLock::new();
    IDX.get_or_init(load)
}

/// "City, Country" for the nearest city within 60 km, else "".
pub fn place(lat: f64, lon: f64) -> String {
    let idx = index();
    const MAX_KM: f64 = 60.0;
    let mut best: Option<&City> = None;
    let mut best_d = f64::INFINITY;
    let (ci, cj) = ((lat / CELL).floor() as i32, (lon / CELL).floor() as i32);
    for di in -1..=1 {
        for dj in -1..=1 {
            if let Some(cs) = idx.grid.get(&(ci + di, cj + dj)) {
                for c in cs {
                    let d = haversine(lat, lon, c.lat, c.lon);
                    if d < best_d {
                        best = Some(c);
                        best_d = d;
                    }
                }
            }
        }
    }
    let Some(best) = best else {
        return String::new();
    };
    if best_d > MAX_KM {
        return String::new();
    }
    match idx.countries.get(&best.cc) {
        Some(n) if !n.is_empty() => format!("{}, {n}", best.name),
        _ => best.name.clone(),
    }
}

fn haversine(la1: f64, lo1: f64, la2: f64, lo2: f64) -> f64 {
    const R: f64 = 6371.0;
    let (p1, p2) = (la1.to_radians(), la2.to_radians());
    let (dp, dl) = (p2 - p1, (lo2 - lo1).to_radians());
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * R * a.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places() {
        assert_eq!(place(64.1466, -21.9426), "Reykjavík, Iceland");
        assert_eq!(place(42.6977, 23.3219), "Sofia, Bulgaria");
        assert_eq!(place(-48.87, -123.39), "", "Point Nemo");
    }
}
