//! Physical key layout, in key units (1.0 = one key pitch), following ZMK's physical layout
//! model: each key is an axis-aligned rectangle rotated by `rot` degrees (clockwise, y down)
//! around the point (`rx`, `ry`).

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// Rotation in degrees, clockwise on screen.
    pub rot: f64,
    pub rx: f64,
    pub ry: f64,
}

impl Key {
    /// Builds a key from ZMK's integer units: centi-key-units for positions, centi-degrees for rotation.
    pub fn from_zmk(w: i32, h: i32, x: i32, y: i32, rot: i32, rx: i32, ry: i32) -> Self {
        let u = |v: i32| f64::from(v) / 100.0;
        Key { x: u(x), y: u(y), w: u(w), h: u(h), rot: u(rot), rx: u(rx), ry: u(ry) }
    }

    /// The four corners after rotation: top-left, top-right, bottom-right, bottom-left.
    pub fn corners(&self) -> [(f64, f64); 4] {
        let (s, c) = self.rot.to_radians().sin_cos();
        let rotate = |px: f64, py: f64| {
            let (dx, dy) = (px - self.rx, py - self.ry);
            (self.rx + dx * c - dy * s, self.ry + dx * s + dy * c)
        };
        [
            rotate(self.x, self.y),
            rotate(self.x + self.w, self.y),
            rotate(self.x + self.w, self.y + self.h),
            rotate(self.x, self.y + self.h),
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Bounds {
    pub fn width(&self) -> f64 {
        self.max_x - self.min_x
    }

    pub fn height(&self) -> f64 {
        self.max_y - self.min_y
    }
}

/// Bounding box of all keys, rotation included.
pub fn bounds(keys: &[Key]) -> Bounds {
    let mut b = Bounds {
        min_x: f64::INFINITY,
        min_y: f64::INFINITY,
        max_x: f64::NEG_INFINITY,
        max_y: f64::NEG_INFINITY,
    };
    for (x, y) in keys.iter().flat_map(Key::corners) {
        b.min_x = b.min_x.min(x);
        b.min_y = b.min_y.min(y);
        b.max_x = b.max_x.max(x);
        b.max_y = b.max_y.max(y);
    }
    b
}

/// GEIST TOTEM, from zmk-config-totem `totem-layouts.dtsi`. Keymap order (0..37).
pub fn totem() -> Vec<Key> {
    #[rustfmt::skip]
    const KEYS: [[i32; 7]; 38] = [
        //  w    h     x    y    rot    rx    ry
        [100, 100,   73, 152, -1000,  123,  202],
        [100, 100,  186,  60,  -400,  236,  110],
        [100, 100,  300,   0,     0,    0,    0],
        [100, 100,  400,  50,     0,    0,    0],
        [100, 100,  500,  66,     0,    0,    0],
        [100, 100,  985,  66,     0,    0,    0],
        [100, 100, 1085,  50,     0,    0,    0],
        [100, 100, 1185,   0,     0,    0,    0],
        [100, 100, 1300,  60,   400, 1350,  110],
        [100, 100, 1412, 152,  1000, 1462,  202],
        [100, 100,   90, 250, -1000,  140,  300],
        [100, 100,  192, 160,  -400,  242,  210],
        [100, 100,  300, 100,     0,    0,    0],
        [100, 100,  400, 150,     0,    0,    0],
        [100, 100,  500, 166,     0,    0,    0],
        [100, 100,  985, 166,     0,    0,    0],
        [100, 100, 1085, 150,     0,    0,    0],
        [100, 100, 1185, 100,     0,    0,    0],
        [100, 100, 1293, 160,   400, 1343,  210],
        [100, 100, 1395, 250,  1000, 1445,  300],
        [100, 100,    0, 323, -1000,   50,  373],
        [100, 100,  106, 349, -1000,  156,  399],
        [100, 100,  199, 260,  -400,  249,  310],
        [100, 100,  300, 200,     0,    0,    0],
        [100, 100,  400, 250,     0,    0,    0],
        [100, 100,  500, 266,     0,    0,    0],
        [100, 100,  985, 266,     0,    0,    0],
        [100, 100, 1085, 250,     0,    0,    0],
        [100, 100, 1185, 200,     0,    0,    0],
        [100, 100, 1287, 260,   400, 1337,  310],
        [100, 100, 1379, 349,  1000, 1429,  399],
        [100, 100, 1485, 323,  1000, 1535,  373],
        [100, 100,  372, 371,     0,    0,    0],
        [100, 100,  483, 388,  1500,  533,  438],
        [100, 100,  585, 433,  3000,  635,  483],
        [100, 100,  900, 433, -3000,  950,  483],
        [100, 100, 1002, 388, -1500, 1052,  438],
        [100, 100, 1113, 371,     0,    0,    0],
    ];
    KEYS.iter()
        .map(|&[w, h, x, y, rot, rx, ry]| Key::from_zmk(w, h, x, y, rot, rx, ry))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn unrotated_corners() {
        let k = Key::from_zmk(100, 100, 300, 0, 0, 0, 0);
        assert_eq!(k.corners(), [(3.0, 0.0), (4.0, 0.0), (4.0, 1.0), (3.0, 1.0)]);
    }

    #[test]
    fn rotation_is_clockwise_around_origin_point() {
        // A 90° clockwise turn around the key's top-left corner swings the top-right corner down.
        let k = Key { x: 0.0, y: 0.0, w: 1.0, h: 1.0, rot: 90.0, rx: 0.0, ry: 0.0 };
        let [tl, tr, ..] = k.corners();
        assert!(close(tl.0, 0.0) && close(tl.1, 0.0));
        assert!(close(tr.0, 0.0) && close(tr.1, 1.0));
    }

    #[test]
    fn rotation_around_center_keeps_center() {
        let k = Key::from_zmk(100, 100, 73, 152, -1000, 123, 202);
        let c = k.corners();
        let cx = c.iter().map(|p| p.0).sum::<f64>() / 4.0;
        let cy = c.iter().map(|p| p.1).sum::<f64>() / 4.0;
        assert!(close(cx, 1.23) && close(cy, 2.02));
    }

    #[test]
    fn totem_is_symmetric() {
        let keys = totem();
        assert_eq!(keys.len(), 38);
        let b = bounds(&keys);
        let mid = (b.min_x + b.max_x) / 2.0;
        // Mirror pairs within each row: 0..10, 10..20, 20..32, 32..38.
        for (start, end) in [(0, 10), (10, 20), (20, 32), (32, 38)] {
            for i in 0..(end - start) / 2 {
                let (l, r) = (keys[start + i], keys[end - 1 - i]);
                let lc = l.x + l.w / 2.0;
                let rc = r.x + r.w / 2.0;
                assert!((lc - mid + rc - mid).abs() < 0.08, "keys {} / {}", start + i, end - 1 - i);
                assert!(close(l.rot, -r.rot), "keys {} / {}", start + i, end - 1 - i);
            }
        }
    }

    #[test]
    fn totem_bounds_include_rotation() {
        let b = bounds(&totem());
        // Key 20 is rotated -10° around its center, so it reaches left of x = 0.
        assert!(b.min_x < 0.0);
        assert!(b.width() > 15.85 && b.width() < 16.3);
        assert!(b.height() > 5.0 && b.height() < 5.6);
    }
}
