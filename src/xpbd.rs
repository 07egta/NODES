//! Bounded, deterministic XPBD particle dynamics. Independent of graph state.
pub type Vec3 = [f64; 3];

#[derive(Clone, Copy, Debug)]
pub struct Distance {
    pub a: usize,
    pub b: usize,
    pub rest: f64,
    pub compliance: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub dt: f64,
    pub iterations: usize,
    pub gravity: Vec3,
    pub damping: f64,
    pub floor: Option<f64>,
    pub friction: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub positions: Vec<Vec3>,
    pub velocities: Vec<Vec3>,
    pub frame: u32,
}

fn finite(v: Vec3) -> bool {
    v.iter().all(|x| x.is_finite() && x.abs() <= 1e9)
}

/// Advance one frame atomically. Inverse mass zero fixes a particle.
/// Compliance is nonnegative; zero represents a hard distance constraint.
pub fn step(
    state: &mut State,
    weights: &[f64],
    constraints: &[Distance],
    cfg: Settings,
    stop: &dyn Fn() -> bool,
) -> Result<(), String> {
    let n = state.positions.len();
    if n > 60_000
        || state.velocities.len() != n
        || weights.len() != n
        || constraints.len() > 300_000
        || !(1e-5..=0.1).contains(&cfg.dt)
        || !(1..=128).contains(&cfg.iterations)
        || !(0.0..=1.0).contains(&cfg.damping)
        || !(0.0..=1.0).contains(&cfg.friction)
        || !finite(cfg.gravity)
        || cfg.floor.is_some_and(|z| !z.is_finite() || z.abs() > 1e9)
        || weights.iter().any(|w| !w.is_finite() || !(0.0..=1e9).contains(w))
        || state.positions.iter().chain(&state.velocities).any(|p| !finite(*p))
        || constraints.iter().any(|c| {
            c.a >= n || c.b >= n || c.a == c.b
                || !(0.0..=1e9).contains(&c.rest)
                || !(0.0..=1e9).contains(&c.compliance)
        })
        || (n + constraints.len()).saturating_mul(cfg.iterations) > 30_000_000
    {
        return Err("Invalid XPBD state or settings".into());
    }
    let frame = state.frame.checked_add(1).ok_or("Frame overflow")?;
    let mut next = state.clone();
    let mut lambda = vec![0.0; constraints.len()];
    let dt = cfg.dt;
    for (i, &w) in weights.iter().enumerate() {
        if i % 1024 == 0 && stop() {
            return Err("Simulation cancelled".into());
        }
        if w == 0.0 {
            next.velocities[i] = [0.0; 3];
            continue;
        }
        for k in 0..3 {
            next.velocities[i][k] =
                state.velocities[i][k] * (1.0 - cfg.damping) + dt * cfg.gravity[k];
            next.positions[i][k] += dt * next.velocities[i][k];
        }
    }
    for _ in 0..cfg.iterations {
        for (j, c) in constraints.iter().enumerate() {
            if j % 1024 == 0 && stop() {
                return Err("Simulation cancelled".into());
            }
            let d = std::array::from_fn::<_, 3, _>(|k| {
                next.positions[c.a][k] - next.positions[c.b][k]
            });
            let length = d.iter().map(|x| x * x).sum::<f64>().sqrt();
            let (wa, wb) = (weights[c.a], weights[c.b]);
            if wa + wb == 0.0 {
                continue;
            }
            if length <= 1e-12 {
                if c.rest > 1e-12 {
                    return Err("Coincident distance constraint".into());
                }
                continue;
            }
            let alpha = c.compliance / (dt * dt);
            let dl = (-(length - c.rest) - alpha * lambda[j]) / (wa + wb + alpha);
            lambda[j] += dl;
            for (k, component) in d.iter().enumerate() {
                let correction = dl * component / length;
                next.positions[c.a][k] += wa * correction;
                next.positions[c.b][k] -= wb * correction;
            }
        }
        if let Some(z) = cfg.floor {
            for (i, &w) in weights.iter().enumerate() {
                if w > 0.0 {
                    next.positions[i][2] = next.positions[i][2].max(z);
                }
            }
        }
    }
    for (i, &w) in weights.iter().enumerate() {
        if w == 0.0 {
            continue;
        }
        for k in 0..3 {
            next.velocities[i][k] = (next.positions[i][k] - state.positions[i][k]) / dt;
        }
        if cfg.floor.is_some_and(|z| next.positions[i][2] <= z) {
            next.velocities[i][2] = next.velocities[i][2].max(0.0);
            next.velocities[i][0] *= 1.0 - cfg.friction;
            next.velocities[i][1] *= 1.0 - cfg.friction;
        }
    }
    if stop() {
        return Err("Simulation cancelled".into());
    }
    if next.positions.iter().chain(&next.velocities).any(|p| !finite(*p)) {
        return Err("XPBD diverged".into());
    }
    next.frame = frame;
    *state = next;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            dt: 0.01,
            iterations: 8,
            gravity: [0.0, 0.0, -9.81],
            damping: 0.0,
            floor: None,
            friction: 0.0,
        }
    }

    fn initial() -> State {
        State {
            positions: vec![[0.0; 3], [2.0, 0.0, 0.0]],
            velocities: vec![[0.0; 3]; 2],
            frame: 0,
        }
    }

    #[test]
    fn hard_distance_and_pin() {
        let mut s = initial();
        let c = Distance { a: 0, b: 1, rest: 1.0, compliance: 0.0 };
        step(&mut s, &[0.0, 1.0], &[c], settings(), &|| false).unwrap();
        assert_eq!(s.positions[0], [0.0; 3]);
        let l = s.positions[1].iter().map(|x| x*x).sum::<f64>().sqrt();
        assert!((l - 1.0).abs() < 1e-10);
        assert_eq!(s.frame, 1);
    }

    #[test]
    fn cancellation_is_atomic() {
        let mut s = initial();
        let before = s.clone();
        assert!(step(&mut s, &[1.0; 2], &[], settings(), &|| true).is_err());
        assert_eq!(s, before);
    }

    #[test]
    fn replay_is_deterministic_and_floor_holds() {
        let mut a = initial();
        let mut b = a.clone();
        let mut cfg = settings();
        cfg.floor = Some(-0.1);
        for _ in 0..100 {
            step(&mut a, &[1.0; 2], &[], cfg, &|| false).unwrap();
            step(&mut b, &[1.0; 2], &[], cfg, &|| false).unwrap();
        }
        assert_eq!(a, b);
        assert!(a.positions.iter().all(|p| p[2] >= -0.1));
    }
}
