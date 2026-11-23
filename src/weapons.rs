//! The weapons. One table shared by the client (which fires them) and the
//! server (which checks fire rates and applies the damage).

pub type WeaponId = u8;

pub struct Weapon {
    pub name: &'static str,
    /// Damage per bullet (per pellet for the shotgun, per stab for the knife).
    pub damage: i32,
    /// Seconds between shots.
    pub interval: f32,
    /// Fires for as long as the trigger is held.
    pub auto: bool,
    /// Bullets per shot.
    pub pellets: usize,
    pub magazine: u32,
    pub reload: f32,
    /// Bullet speed in m/s.
    pub speed: f32,
    /// How fast the bullet drops (m/s²).
    pub gravity: f32,
    /// Half angle (radians) of the cone shots land in, from the hip and aimed.
    pub hip_spread: f32,
    pub aim_spread: f32,
    /// Field of view factor when aiming down sights; smaller zooms in more.
    pub zoom: f32,
    /// Running speed factor while it is in your hands.
    pub mobility: f32,
    /// Size of the gun in first person: width, height, length (metres).
    pub size: [f32; 3],
    /// How far in front of the eye a sight's dot is held when aiming: close
    /// for a gun held to the shoulder, further out for a pistol held in front.
    pub sight_relief: f32,
    /// Playback speed of the gunshot sound: lower is deeper.
    pub pitch: f32,
    /// How far each shot throws the view up, and at most sideways (radians).
    pub kick_up: f32,
    pub kick_side: f32,
    /// Damage is multiplied by this when a bullet hits the head.
    pub headshot: f32,
    /// The 3D model drawn for it. Without one it is drawn as a few boxes of `size`.
    pub model: Option<Model>,
}

/// A weapon's glTF model. It is scaled to the weapon's `size` length and centred
/// where the box gun would be, so it needs no hand-tuned numbers.
pub struct Model {
    /// Under `assets/`.
    pub path: &'static str,
    /// The barrel points along +Z in the file (forward in the game is -Z).
    pub barrel_plus_z: bool,
    /// The sights it can carry besides its iron sights, on the file's
    /// `SightMount` node (see `gun_model.rs`). Empty without a mount.
    pub sights: &'static [SightId],
}

pub type SightId = u8;
/// No sight: the gun's own iron sights.
pub const IRON_SIGHTS: SightId = 0;
pub const RED_DOT: SightId = 1;
pub const SCOPE: SightId = 2;
pub const REFLEX: SightId = 3;

pub struct Sight {
    pub name: &'static str,
    /// A glTF model (under `assets/`) in metres, with an empty named `AimPoint`
    /// at its dot. It goes on the gun's `SightMount` node, the gun's nodes named
    /// `IronSight...` are hidden under it, and aiming lines up the dot with the
    /// crosshair. `None` for the iron sights.
    pub path: Option<&'static str>,
    /// Field of view factor when aiming through it, instead of the gun's own.
    pub zoom: Option<f32>,
    /// Fully aimed, the view through it is a scope's: the gun is hidden and a
    /// lens with a reticle fills the screen.
    pub overlay: bool,
}

pub const SIGHTS: [Sight; 4] = [
    Sight {
        name: "Iron sights",
        path: None,
        zoom: None,
        overlay: false,
    },
    Sight {
        name: "Red dot",
        path: Some("models/sights/red_dot.glb"),
        zoom: None,
        overlay: false,
    },
    Sight {
        name: "Scope",
        path: Some("models/sights/scope.glb"),
        zoom: Some(0.25),
        overlay: true,
    },
    // An open window with a ring round the dot: low and light, so it suits a pistol too.
    Sight {
        name: "Reflex",
        path: Some("models/sights/reflex.glb"),
        zoom: None,
        overlay: false,
    },
];

/// Unknown ids are the iron sights.
pub fn sight(id: SightId) -> &'static Sight {
    SIGHTS.get(id as usize).unwrap_or(&SIGHTS[IRON_SIGHTS as usize])
}

/// The sights a weapon can carry besides its iron sights.
pub fn sights_for(id: WeaponId) -> &'static [SightId] {
    weapon(id).model.as_ref().map_or(&[], |model| model.sights)
}

/// Whether a weapon can carry a sight other than its iron sights.
pub fn takes_sights(id: WeaponId) -> bool {
    !sights_for(id).is_empty()
}

/// The sight a weapon really carries when `wanted` is asked for: a sight it
/// cannot take (or an unknown one) leaves it with its iron sights.
pub fn sight_on(weapon_id: WeaponId, wanted: SightId) -> SightId {
    if sights_for(weapon_id).contains(&wanted) {
        wanted
    } else {
        IRON_SIGHTS
    }
}

/// How far a knife reaches, in metres.
pub const KNIFE_RANGE: f32 = 2.6;
/// Seconds it takes to bring a weapon up after switching to it.
pub const DRAW_TIME: f32 = 0.35;

pub const RIFLE: WeaponId = 0;
pub const SNIPER: WeaponId = 3;
pub const PISTOL: WeaponId = 4;
pub const KNIFE: WeaponId = 5;
/// Not in the table: used only to label grenade kills.
pub const GRENADE: WeaponId = 6;
/// The weapons that can go in a gun slot are `0..GUN_COUNT`.
pub const GUN_COUNT: WeaponId = 5;

pub const WEAPONS: [Weapon; 6] = [
    Weapon {
        name: "Rifle",
        damage: 20,
        interval: 0.11,
        auto: true,
        pellets: 1,
        magazine: 30,
        reload: 1.6,
        speed: 200.0,
        gravity: 4.0,
        hip_spread: 0.014,
        aim_spread: 0.0,
        zoom: 0.62,
        mobility: 1.0,
        size: [0.04, 0.055, 0.3],
        sight_relief: 0.08,
        pitch: 1.0,
        kick_up: 0.014,
        kick_side: 0.006,
        headshot: 2.0,
        model: Some(Model {
            path: "models/M4A1.gltf",
            barrel_plus_z: true,
            sights: &[RED_DOT, REFLEX],
        }),
    },
    Weapon {
        name: "SMG",
        damage: 13,
        interval: 0.065,
        auto: true,
        pellets: 1,
        magazine: 36,
        reload: 1.3,
        speed: 150.0,
        gravity: 5.0,
        hip_spread: 0.024,
        aim_spread: 0.008,
        zoom: 0.75,
        mobility: 1.05,
        size: [0.04, 0.06, 0.2],
        sight_relief: 0.08,
        pitch: 1.3,
        kick_up: 0.012,
        kick_side: 0.007,
        headshot: 1.6,
        model: Some(Model {
            path: "models/MP7.gltf",
            barrel_plus_z: true,
            sights: &[RED_DOT, REFLEX],
        }),
    },
    Weapon {
        name: "Shotgun",
        damage: 12,
        interval: 0.8,
        auto: false,
        pellets: 9,
        magazine: 6,
        reload: 2.2,
        speed: 130.0,
        gravity: 6.0,
        hip_spread: 0.06,
        aim_spread: 0.045,
        zoom: 0.8,
        mobility: 1.0,
        size: [0.05, 0.06, 0.34],
        sight_relief: 0.08,
        pitch: 0.7,
        kick_up: 0.075,
        kick_side: 0.018,
        headshot: 1.5,
        model: Some(Model {
            path: "models/Shotgun.glb",
            barrel_plus_z: true,
            sights: &[],
        }),
    },
    Weapon {
        name: "Sniper",
        damage: 90,
        interval: 1.2,
        auto: false,
        pellets: 1,
        magazine: 5,
        reload: 2.4,
        speed: 400.0,
        gravity: 3.0,
        hip_spread: 0.05,
        aim_spread: 0.0,
        zoom: 0.55,
        mobility: 0.9,
        size: [0.035, 0.05, 0.44],
        sight_relief: 0.08,
        pitch: 0.55,
        kick_up: 0.1,
        kick_side: 0.012,
        headshot: 2.5,
        model: Some(Model {
            path: "models/SCAR.gltf",
            barrel_plus_z: true,
            sights: &[RED_DOT, REFLEX, SCOPE],
        }),
    },
    Weapon {
        name: "Pistol",
        damage: 26,
        interval: 0.17,
        auto: false,
        pellets: 1,
        magazine: 12,
        reload: 1.1,
        speed: 170.0,
        gravity: 5.0,
        hip_spread: 0.012,
        aim_spread: 0.002,
        zoom: 0.78,
        mobility: 1.05,
        size: [0.035, 0.05, 0.15],
        sight_relief: 0.18,
        pitch: 1.45,
        kick_up: 0.03,
        kick_side: 0.007,
        headshot: 2.0,
        model: Some(Model {
            path: "models/M1911Tactical.gltf",
            barrel_plus_z: false,
            sights: &[RED_DOT, REFLEX],
        }),
    },
    Weapon {
        name: "Knife",
        damage: 150,
        interval: 0.2,
        auto: true,
        pellets: 0,
        magazine: 0,
        reload: 0.0,
        speed: 0.0,
        gravity: 0.0,
        hip_spread: 0.0,
        aim_spread: 0.0,
        zoom: 1.0,
        mobility: 1.32,
        // Blade and handle together.
        size: [0.012, 0.04, 0.3],
        sight_relief: 0.08,
        pitch: 1.0,
        kick_up: 0.0,
        kick_side: 0.0,
        headshot: 1.0,
        model: Some(Model {
            path: "models/Knife.glb",
            barrel_plus_z: true,
            sights: &[],
        }),
    },
];

/// Unknown ids are treated as the rifle.
pub fn weapon(id: WeaponId) -> &'static Weapon {
    WEAPONS.get(id as usize).unwrap_or(&WEAPONS[RIFLE as usize])
}

/// Damage one bullet (or stab) of this weapon deals.
pub fn hit_damage(id: WeaponId, headshot: bool) -> i32 {
    let stats = weapon(id);
    if headshot {
        (stats.damage as f32 * stats.headshot).round() as i32
    } else {
        stats.damage
    }
}

/// Name for the kill feed.
pub fn weapon_name(id: WeaponId) -> &'static str {
    if id == GRENADE { "Grenade" } else { weapon(id).name }
}

/// The gun after `current` in the list, skipping the one in the other slot.
pub fn next_gun(current: WeaponId, other_slot: WeaponId) -> WeaponId {
    let mut next = (current + 1) % GUN_COUNT;
    if next == other_slot {
        next = (next + 1) % GUN_COUNT;
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_consistent() {
        assert_eq!(WEAPONS[KNIFE as usize].name, "Knife");
        assert_eq!(WEAPONS[PISTOL as usize].name, "Pistol");
        for gun in &WEAPONS[..GUN_COUNT as usize] {
            assert!(gun.pellets >= 1 && gun.magazine >= 1 && gun.speed > 50.0, "{}", gun.name);
            assert!(gun.aim_spread <= gun.hip_spread, "{}", gun.name);
        }
    }

    #[test]
    fn cycling_guns_never_duplicates_a_slot() {
        for other in 0..GUN_COUNT {
            let mut gun = (other + 1) % GUN_COUNT;
            for _ in 0..10 {
                gun = next_gun(gun, other);
                assert!(gun != other && gun < GUN_COUNT);
            }
        }
    }
}
