//! Synthetic Mainnet-like mesh fill: Vec::with_capacity must not bind EdgeAttrs::with_capacity.

pub fn fill_region_mesh(region_id: u32, rings: Vec<(f64, f64)>) {
    let mut pts = Vec::with_capacity(rings.len());
    for (x, y) in rings {
        pts.push((x + region_id as f64, y));
    }
    let _ = pts;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rings_one() {
        fill_region_mesh(1, vec![(0.0, 0.0)]);
    }

    #[test]
    fn rings_two() {
        fill_region_mesh(2, vec![(0.0, 0.0), (1.0, 1.0)]);
    }

    #[test]
    fn rings_three() {
        fill_region_mesh(3, vec![(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)]);
    }

    #[test]
    fn rings_four() {
        fill_region_mesh(4, Vec::new());
    }
}
