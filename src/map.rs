//! The maps: static axis-aligned blocks, jump pads and spawn points, plus the
//! collision helpers used by the player controller, the bots, grenades and hit
//! detection. Pure data and math, so the server can use it without any rendering.

use bevy::math::Vec3;

/// Half extents of the player's collision box when standing (0.8 x 1.8 x 0.8 m).
pub const PLAYER_HALF: Vec3 = Vec3::new(0.4, 0.9, 0.4);
/// The same when crouching or sliding (1.2 m tall).
pub const CROUCH_HALF: Vec3 = Vec3::new(0.4, 0.6, 0.4);
/// Eye height above the feet.
pub const STAND_EYE: f32 = 1.6;
pub const CROUCH_EYE: f32 = 1.0;
/// Half the side of the little box a grenade collides as.
pub const GRENADE_HALF: f32 = 0.12;
const GRENADE_GRAVITY: f32 = 20.0;

pub const MAP_NAMES: [&str; 2] = ["Arena", "Depot"];

pub fn player_half(crouching: bool) -> Vec3 {
    if crouching { CROUCH_HALF } else { PLAYER_HALF }
}

/// Slightly fatter box used for bullet hits, to be forgiving at high speed.
pub fn hitbox_half(crouching: bool) -> Vec3 {
    player_half(crouching) + Vec3::new(0.08, 0.05, 0.08)
}

/// The part of the bullet hitbox that counts as the head: the top of it,
/// narrower than the shoulders. `center` is the centre of the collision box.
pub fn head_box(center: Vec3, crouching: bool) -> (Vec3, Vec3) {
    const HEAD_HALF: Vec3 = Vec3::new(0.3, 0.27, 0.3);
    let top = center.y + hitbox_half(crouching).y;
    let mid = Vec3::new(center.x, top - HEAD_HALF.y, center.z);
    (mid - HEAD_HALF, mid + HEAD_HALF)
}

pub struct Block {
    pub min: Vec3,
    pub max: Vec3,
    pub color: [f32; 3],
    /// Invisible blocks still collide (used for the ceiling).
    pub visible: bool,
}

/// Two kinds of pad, told apart by `launch`:
/// - arrow pads (`launch` has a horizontal part) throw you that way, keeping
///   only half the speed you came in with, so they land you somewhere precise;
/// - plain pads (`launch` points straight up) only throw you up, keeping
///   your speed and direction.
pub struct JumpPad {
    /// Centre of the pad on the floor.
    pub center: Vec3,
    pub half_size: f32,
    /// Velocity a player standing still is launched with.
    pub launch: Vec3,
}

impl JumpPad {
    fn arrow(center: Vec3, launch: Vec3) -> JumpPad {
        JumpPad { center, half_size: 1.0, launch }
    }

    fn plain(center: Vec3, lift: f32) -> JumpPad {
        JumpPad { center, half_size: 1.0, launch: Vec3::Y * lift }
    }

    /// The horizontal direction an arrow pad pushes you in, or `None` for a plain pad.
    pub fn direction(&self) -> Option<Vec3> {
        let push = self.launch.with_y(0.0);
        (push.length_squared() > 1e-6).then(|| push.normalize())
    }

    /// Velocity after touching the pad while moving at `vel`.
    pub fn launch_velocity(&self, vel: Vec3) -> Vec3 {
        let kept = if self.direction().is_some() { 0.5 } else { 1.0 };
        Vec3::new(vel.x * kept + self.launch.x, self.launch.y, vel.z * kept + self.launch.z)
    }
}

pub struct Spawn {
    pub pos: Vec3,
    pub yaw: f32,
}

pub struct Map {
    /// Index into `MAP_NAMES`.
    pub id: u8,
    pub blocks: Vec<Block>,
    pub pads: Vec<JumpPad>,
    pub spawns: Vec<Spawn>,
    /// Centre of the zone to hold in king of the hill.
    pub hill: Vec3,
}

const FLOOR: [f32; 3] = [0.20, 0.22, 0.26];
const WALL: [f32; 3] = [0.30, 0.33, 0.40];
const COVER: [f32; 3] = [0.75, 0.42, 0.18];
const PILLAR: [f32; 3] = [0.45, 0.48, 0.55];
const PLATFORM: [f32; 3] = [0.22, 0.45, 0.60];
const LEDGE: [f32; 3] = [0.50, 0.30, 0.60];

const DEPOT_FLOOR: [f32; 3] = [0.25, 0.24, 0.22];
const DEPOT_WALL: [f32; 3] = [0.36, 0.33, 0.30];
const CONCRETE: [f32; 3] = [0.52, 0.51, 0.48];
const CONTAINER_RED: [f32; 3] = [0.62, 0.20, 0.16];
const CONTAINER_BLUE: [f32; 3] = [0.16, 0.34, 0.60];
const CRATE: [f32; 3] = [0.62, 0.46, 0.24];
const BARRIER: [f32; 3] = [0.80, 0.68, 0.20];
const ROOF: [f32; 3] = [0.20, 0.42, 0.36];

/// Arena half size and wall height. Every map uses the same outer box.
const A: f32 = 32.0;
const H: f32 = 16.0;

const QUADRANTS: [(f32, f32); 4] = [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)];
const CARDINALS: [(f32, f32); 4] = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)];

impl Map {
    /// Unknown ids fall back to the first map.
    pub fn by_id(id: u8) -> Map {
        match id {
            1 => Map::depot(),
            _ => Map::arena(),
        }
    }

    /// Open and vertical: a central tower, corner platforms and wall ledges.
    pub fn arena() -> Map {
        let mut blocks = Vec::new();
        let mut pads = Vec::new();
        let mut spawns = Vec::new();
        shell(&mut blocks, FLOOR, WALL);
        let mut add = |min: [f32; 3], max: [f32; 3], color: [f32; 3]| {
            blocks.push(Block {
                min: Vec3::from(min),
                max: Vec3::from(max),
                color,
                visible: true,
            });
        };

        // Central tower: a pillar holding a platform, reached with the inner jump pads.
        add([-1.5, 0.0, -1.5], [1.5, 3.5, 1.5], PILLAR);
        add([-4.5, 3.5, -4.5], [4.5, 4.0, 4.5], PLATFORM);

        for (sx, sz) in QUADRANTS {
            // Low cover you can hop onto.
            let (cx, cz) = (12.0 * sx, 12.0 * sz);
            add([cx - 2.0, 0.0, cz - 2.0], [cx + 2.0, 1.2, cz + 2.0], COVER);

            // Raised corner platform with a step up to it.
            let (x0, x1) = minmax(21.0 * sx, A * sx);
            let (z0, z1) = minmax(21.0 * sz, A * sz);
            add([x0, 0.0, z0], [x1, 2.5, z1], PLATFORM);
            let (s0, s1) = minmax(18.0 * sx, 21.0 * sx);
            let (t0, t1) = minmax(24.0 * sz, 29.0 * sz);
            add([s0, 0.0, t0], [s1, 1.25, t1], COVER);

            spawns.push(spawn_facing_center(Vec3::new(27.0 * sx, 2.5, 27.0 * sz)));

            // Plain pad by the tower: run across it to hop onto the platform,
            // or over the low cover.
            pads.push(JumpPad::plain(Vec3::new(7.0 * sx, 0.0, 7.0 * sz), 14.0));
        }

        for (dx, dz) in CARDINALS {
            let dir = Vec3::new(dx, 0.0, dz);
            let side = Vec3::new(dz, 0.0, dx);

            // Tall pillar that breaks the long sight lines.
            let c = dir * 18.0;
            add([c.x - 1.5, 0.0, c.z - 1.5], [c.x + 1.5, 6.0, c.z + 1.5], PILLAR);

            // High ledge along the middle of each wall.
            let a = dir * 27.5 - side * 10.0;
            let b = dir * A + side * 10.0;
            let (x0, x1) = minmax(a.x, b.x);
            let (z0, z1) = minmax(a.z, b.z);
            add([x0, 4.5, z0], [x1, 5.0, z1], LEDGE);

            // Inner pad: up onto the central tower.
            pads.push(JumpPad::arrow(dir * 8.5, -dir * 7.0 + Vec3::Y * 15.5));
            // Outer pad: up onto the wall ledge.
            pads.push(JumpPad::arrow(dir * 24.0, dir * 6.5 + Vec3::Y * 17.0));

            spawns.push(spawn_facing_center(dir * 13.5 + side * 5.0));
        }

        Map {
            id: 0,
            hill: Vec3::ZERO,
            blocks,
            pads,
            spawns,
        }
    }

    /// A cluttered freight yard: containers, crates and barriers everywhere, so
    /// you are rarely more than a few steps from cover. A roofed bunker sits in
    /// the middle.
    pub fn depot() -> Map {
        let mut blocks = Vec::new();
        let mut pads = Vec::new();
        let mut spawns = Vec::new();
        shell(&mut blocks, DEPOT_FLOOR, DEPOT_WALL);
        // A block given by the centre and half size of its footprint, and its
        // bottom and top.
        let mut add = |cx: f32, cz: f32, hx: f32, hz: f32, y0: f32, y1: f32, color: [f32; 3]| {
            blocks.push(Block {
                min: Vec3::new(cx - hx, y0, cz - hz),
                max: Vec3::new(cx + hx, y1, cz + hz),
                color,
                visible: true,
            });
        };

        // Central bunker: four pillars under a roof, with a crate inside. The
        // inner jump pads put you on the roof.
        add(0.0, 0.0, 1.0, 1.0, 0.0, 1.2, CRATE);
        add(0.0, 0.0, 5.5, 5.5, 3.0, 3.4, ROOF);

        for (sx, sz) in QUADRANTS {
            add(4.0 * sx, 4.0 * sz, 1.0, 1.0, 0.0, 3.0, CONCRETE);
            // Parapet on each roof corner.
            add(4.5 * sx, 4.5 * sz, 1.0, 1.0, 3.4, 4.2, CONCRETE);

            // Two shipping containers, too tall to see over.
            let (first, second) = if sx * sz > 0.0 {
                (CONTAINER_RED, CONTAINER_BLUE)
            } else {
                (CONTAINER_BLUE, CONTAINER_RED)
            };
            add(13.0 * sx, 8.0 * sz, 1.5, 3.5, 0.0, 2.8, first);
            add(8.0 * sx, 19.0 * sz, 3.5, 1.5, 0.0, 2.8, second);

            // Crates: stand behind the low ones, climb the stacked pair.
            add(8.5 * sx, 8.5 * sz, 1.0, 1.0, 0.0, 1.2, CRATE);
            add(16.0 * sx, 14.5 * sz, 1.0, 1.0, 0.0, 1.2, CRATE);
            add(18.0 * sx, 14.5 * sz, 1.0, 1.0, 0.0, 2.4, CRATE);
            add(27.0 * sx, 16.0 * sz, 1.0, 1.0, 0.0, 1.2, CRATE);

            // Barriers: they hide you only when you crouch.
            add(20.0 * sx, 22.0 * sz, 3.0, 0.4, 0.0, 1.3, BARRIER);
            add(23.0 * sx, 9.0 * sz, 0.4, 3.0, 0.0, 1.3, BARRIER);

            // A wall that shields the corner spawn.
            add(28.0 * sx, 24.5 * sz, 2.5, 0.4, 0.0, 2.0, CONCRETE);
            spawns.push(spawn_facing_center(Vec3::new(28.5 * sx, 0.0, 28.5 * sz)));

            // Plain pad beside the near container: run at it to land on top.
            pads.push(JumpPad::plain(Vec3::new(16.5 * sx, 0.0, 8.0 * sz), 12.0));
        }

        for (dx, dz) in CARDINALS {
            let dir = Vec3::new(dx, 0.0, dz);
            let side = Vec3::new(dz, 0.0, dx);
            let (along, across) = (dir.abs(), side.abs());

            // A long wall across each lane, and a barrier closer to the middle.
            let c = dir * 21.0;
            let half = across * 5.0 + along * 0.5;
            add(c.x, c.z, half.x, half.z, 0.0, 3.5, CONCRETE);
            let c = dir * 14.5;
            let half = across * 2.5 + along * 0.4;
            add(c.x, c.z, half.x, half.z, 0.0, 1.3, BARRIER);

            // Inner pad: up onto the bunker roof.
            pads.push(JumpPad::arrow(dir * 9.5, -dir * 7.0 + Vec3::Y * 14.5));
            // Outer pad: back over the long wall, into the fight.
            pads.push(JumpPad::arrow(dir * 27.0, -dir * 7.0 + Vec3::Y * 15.0));

            spawns.push(spawn_facing_center(dir * 27.0 + side * 7.0));
        }

        Map {
            id: 1,
            hill: Vec3::ZERO,
            blocks,
            pads,
            spawns,
        }
    }

    /// Moves a box with half extents `half` by `vel * dt`, sliding along blocks.
    /// Velocity is zeroed on any axis that hits something. Returns true if the
    /// box ended up standing on a block.
    pub fn move_and_slide(&self, pos: &mut Vec3, vel: &mut Vec3, half: Vec3, dt: f32) -> bool {
        let total = *vel * dt;
        let steps = (total.abs().max_element() / 0.25).ceil().clamp(1.0, 32.0) as usize;
        let step = total / steps as f32;
        let mut grounded = false;
        for _ in 0..steps {
            // Horizontal axes first, so that walking off a ledge behaves sensibly.
            for axis in [0, 2, 1] {
                let delta = step[axis];
                if delta == 0.0 || vel[axis] == 0.0 {
                    continue;
                }
                pos[axis] += delta;
                for block in &self.blocks {
                    if !overlaps(*pos, half, block) {
                        continue;
                    }
                    if delta > 0.0 {
                        pos[axis] = block.min[axis] - half[axis];
                    } else {
                        pos[axis] = block.max[axis] + half[axis];
                        if axis == 1 {
                            grounded = true;
                        }
                    }
                    vel[axis] = 0.0;
                }
            }
        }
        grounded
    }

    /// True if a box with half extents `half` fits at `center` without touching a block.
    pub fn is_free(&self, center: Vec3, half: Vec3) -> bool {
        !self.blocks.iter().any(|block| overlaps(center, half, block))
    }

    /// Advances a grenade by one `GRENADE_STEP`: it falls, bounces off blocks
    /// and rolls to a stop. Returns true on a bounce hard enough to hear.
    pub fn step_grenade(&self, pos: &mut Vec3, vel: &mut Vec3, dt: f32) -> bool {
        const RESTITUTION: f32 = 0.42;
        /// Share of the sideways speed kept through each contact.
        const GRIP: f32 = 0.84;
        let half = Vec3::splat(GRENADE_HALF);
        vel.y -= GRENADE_GRAVITY * dt;
        let steps = (vel.length() * dt / 0.1).ceil().clamp(1.0, 64.0);
        let sub_dt = dt / steps;
        let mut bounced = false;
        for _ in 0..steps as usize {
            for axis in [0, 2, 1] {
                let delta = vel[axis] * sub_dt;
                if delta == 0.0 {
                    continue;
                }
                pos[axis] += delta;
                let Some(block) = self.blocks.iter().find(|block| overlaps(*pos, half, block)) else {
                    continue;
                };
                pos[axis] = if delta > 0.0 {
                    block.min[axis] - half[axis]
                } else {
                    block.max[axis] + half[axis]
                };
                let impact = vel[axis].abs();
                *vel *= GRIP;
                // Too slow to bounce: it stays put on that axis (resting on the floor).
                vel[axis] = if impact > 1.5 { -vel[axis] * RESTITUTION / GRIP } else { 0.0 };
                bounced |= impact > 3.0;
            }
        }
        bounced
    }

    /// Returns the jump pad at the player's `feet`, if any.
    pub fn pad_at(&self, feet: Vec3) -> Option<&JumpPad> {
        self.pads.iter().find(|pad| {
            (feet.x - pad.center.x).abs() < pad.half_size
                && (feet.z - pad.center.z).abs() < pad.half_size
                && (feet.y - pad.center.y).abs() < 0.3
        })
    }

    /// Distance along the ray to the closest block, if it is nearer than `max`.
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        self.blocks
            .iter()
            .filter_map(|block| ray_box(origin, dir, block.min, block.max))
            .filter(|t| *t < max)
            .min_by(f32::total_cmp)
    }

    /// True if no block is in the way between the two points.
    pub fn line_of_sight(&self, from: Vec3, to: Vec3) -> bool {
        let distance = from.distance(to);
        distance < 1e-3 || self.raycast(from, (to - from) / distance, distance).is_none()
    }
}

/// Floor, the four walls and an invisible ceiling.
fn shell(blocks: &mut Vec<Block>, floor: [f32; 3], wall: [f32; 3]) {
    let mut add = |min: [f32; 3], max: [f32; 3], color: [f32; 3], visible: bool| {
        blocks.push(Block {
            min: Vec3::from(min),
            max: Vec3::from(max),
            color,
            visible,
        });
    };
    add([-A - 2.0, -2.0, -A - 2.0], [A + 2.0, 0.0, A + 2.0], floor, true);
    add([-A - 2.0, 0.0, -A - 2.0], [-A, H, A + 2.0], wall, true);
    add([A, 0.0, -A - 2.0], [A + 2.0, H, A + 2.0], wall, true);
    add([-A, 0.0, -A - 2.0], [A, H, -A], wall, true);
    add([-A, 0.0, A], [A, H, A + 2.0], wall, true);
    add([-A - 2.0, 30.0, -A - 2.0], [A + 2.0, 32.0, A + 2.0], wall, false);
}

fn minmax(a: f32, b: f32) -> (f32, f32) {
    (a.min(b), a.max(b))
}

/// `feet` is a point on the ground; the spawn is the box centre standing there.
fn spawn_facing_center(feet: Vec3) -> Spawn {
    Spawn {
        pos: feet + Vec3::Y * (PLAYER_HALF.y + 0.05),
        yaw: feet.x.atan2(feet.z),
    }
}

fn overlaps(center: Vec3, half: Vec3, block: &Block) -> bool {
    const EPS: f32 = 1e-4;
    center.x + half.x > block.min.x + EPS
        && center.x - half.x < block.max.x - EPS
        && center.y + half.y > block.min.y + EPS
        && center.y - half.y < block.max.y - EPS
        && center.z + half.z > block.min.z + EPS
        && center.z - half.z < block.max.z - EPS
}

/// Slab test. Returns the entry distance (0 if the origin is inside the box).
pub fn ray_box(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let mut t_near = 0.0_f32;
    let mut t_far = f32::INFINITY;
    for axis in 0..3 {
        if dir[axis].abs() < 1e-8 {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
            continue;
        }
        let inv = 1.0 / dir[axis];
        let (t0, t1) = minmax((min[axis] - origin[axis]) * inv, (max[axis] - origin[axis]) * inv);
        t_near = t_near.max(t0);
        t_far = t_far.min(t1);
        if t_near > t_far {
            return None;
        }
    }
    Some(t_near)
}

/// Forward direction for a yaw/pitch pair (yaw 0 looks down -Z).
pub fn look_dir(yaw: f32, pitch: f32) -> Vec3 {
    Vec3::new(-yaw.sin() * pitch.cos(), pitch.sin(), -yaw.cos() * pitch.cos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_head_is_at_eye_height() {
        for (crouching, eye) in [(false, STAND_EYE), (true, CROUCH_EYE)] {
            let center = Vec3::new(3.0, 5.0, -2.0);
            let feet = center.y - player_half(crouching).y;
            let (min, max) = head_box(center, crouching);
            let shot_at = |y: f32| ray_box(Vec3::new(3.0, y, 10.0), Vec3::NEG_Z, min, max).is_some();
            assert!(shot_at(feet + eye), "crouching: {crouching}");
            assert!(!shot_at(center.y - 0.1), "crouching: {crouching}");
            assert!(max.y <= center.y + hitbox_half(crouching).y + 1e-4);
        }
    }

    fn maps() -> Vec<Map> {
        (0..MAP_NAMES.len() as u8).map(Map::by_id).collect()
    }

    #[test]
    fn ids_match_the_name_table() {
        for (index, map) in maps().iter().enumerate() {
            assert_eq!(map.id as usize, index);
        }
    }

    #[test]
    fn spawns_are_clear_and_on_the_ground() {
        for map in maps() {
            assert!(map.spawns.len() >= 8);
            for spawn in &map.spawns {
                assert!(map.is_free(spawn.pos, PLAYER_HALF), "map {} spawn {:?}", map.id, spawn.pos);
                let (mut pos, mut vel) = (spawn.pos, Vec3::new(0.0, -5.0, 0.0));
                assert!(map.move_and_slide(&mut pos, &mut vel, PLAYER_HALF, 0.1));
                assert!(spawn.pos.y - pos.y < 0.1, "map {} spawn {:?} floats", map.id, spawn.pos);
            }
        }
    }

    #[test]
    fn pads_are_not_blocked() {
        for map in maps() {
            for pad in &map.pads {
                let standing = pad.center + Vec3::Y * PLAYER_HALF.y;
                assert!(map.is_free(standing, PLAYER_HALF), "map {} pad {:?}", map.id, pad.center);
                assert!(map.pad_at(pad.center).is_some());
            }
        }
    }

    /// Follows a player launched by each pad and checks where they come down.
    #[test]
    fn depot_pads_land_where_intended() {
        let map = Map::depot();
        for pad in map.pads.iter().filter(|pad| pad.direction().is_some()) {
            let mut pos = pad.center + Vec3::Y * PLAYER_HALF.y;
            let mut vel = pad.launch;
            let mut landed = false;
            for _ in 0..600 {
                vel.y -= 22.0 / 120.0;
                if map.move_and_slide(&mut pos, &mut vel, PLAYER_HALF, 1.0 / 120.0) {
                    landed = true;
                    break;
                }
            }
            assert!(landed);
            let feet = pos.y - PLAYER_HALF.y;
            let from_center = pos.with_y(0.0).length();
            if pad.center.length() < 15.0 {
                // Inner pads: onto the bunker roof.
                assert!((feet - 3.4).abs() < 0.01 && from_center < 5.5, "landed at {pos:?}");
            } else {
                // Outer pads: over the long wall, onto the ground behind it.
                assert!(feet.abs() < 0.01 && from_center < 20.0, "landed at {pos:?}");
            }
        }
    }

    #[test]
    fn every_map_has_both_kinds_of_pad() {
        for map in maps() {
            assert!(map.pads.iter().any(|pad| pad.direction().is_some()), "map {}", map.id);
            assert!(map.pads.iter().any(|pad| pad.direction().is_none()), "map {}", map.id);
        }
    }

    #[test]
    fn plain_pads_keep_your_direction() {
        let pad = JumpPad::plain(Vec3::ZERO, 14.0);
        let vel = pad.launch_velocity(Vec3::new(9.0, -3.0, -4.0));
        assert_eq!(vel, Vec3::new(9.0, 14.0, -4.0));
        let arrow = JumpPad::arrow(Vec3::ZERO, Vec3::new(7.0, 15.0, 0.0));
        assert_eq!(arrow.direction(), Some(Vec3::X));
        assert_eq!(arrow.launch_velocity(Vec3::ZERO), arrow.launch);
    }

    #[test]
    fn grenades_bounce_and_come_to_rest() {
        let map = Map::arena();
        let mut pos = Vec3::new(10.0, 2.0, 0.0);
        let mut vel = Vec3::new(6.0, 4.0, 3.0);
        let mut bounces = 0;
        for _ in 0..600 {
            bounces += map.step_grenade(&mut pos, &mut vel, 1.0 / 60.0) as u32;
        }
        assert!(bounces >= 1);
        assert!((pos.y - GRENADE_HALF).abs() < 1e-3, "resting height {}", pos.y);
        assert!(vel.length() < 0.5, "still moving at {vel:?}");
    }
}
