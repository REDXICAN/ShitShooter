//! Weapon models from `assets/models`, drawn in place of the box guns for the
//! weapons that have one (see `Weapon::model`).
//!
//! Every gun holder (the first-person gun, and each remote player's gun) gets a
//! child per model and shows the one for the weapon in hand. Once a model has
//! loaded it is measured, turned barrel-forward, centred on the holder and
//! scaled to its weapon's `size` length, so a new model drops in as it is.
//!
//! A gun can carry a sight (`Model::sight`), another glTF file in metres. It is
//! mounted on the gun's `SightMount` node, whose position and scale (sight
//! metres to gun-file units) are set in the gun's file. The gun's `IronSight...`
//! nodes are hidden under it, and aiming lines up the sight's `AimPoint` node
//! with the crosshair.

use std::f32::consts::PI;

use bevy::{camera::primitives::Aabb, light::NotShadowCaster, prelude::*};

use crate::weapons::{WEAPONS, WeaponId, weapon};

pub struct GunModelPlugin;

impl Plugin for GunModelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FittedModels>()
            .add_systems(Startup, load_models)
            .add_systems(
                Update,
                (
                    attach_models,
                    fit_models,
                    measure_sights,
                    show_models.in_set(GunModelSystems::Show),
                )
                    .chain(),
            )
            .configure_sets(
                Update,
                GunModelSystems::Show
                    .after(crate::player::PlayerSystems::Camera)
                    .after(crate::remote::RemoteSystems),
            );
    }
}

/// Shows the model of the weapon each holder has. Holders set their weapon before this.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GunModelSystems {
    Show,
}

/// An entity that holds a gun: its children get one model per modelled weapon.
#[derive(Component)]
pub struct GunHolder {
    pub weapon: WeaponId,
    /// Off for the first-person gun, whose shadow would float in front of you.
    pub shadows: bool,
}

/// What is known about each modelled weapon once its model is fitted, the same
/// for every holder. `None` while it is loading, and for weapons drawn as boxes.
#[derive(Resource, Default)]
pub struct FittedModels([Option<Fitted>; WEAPONS.len()]);

#[derive(Clone, Copy)]
pub struct Fitted {
    /// Width, height and length in metres, centred on the holder.
    pub size: Vec3,
    /// Where the sight's dot is, relative to the holder, once its sight is in.
    pub aim_point: Option<Vec3>,
}

impl FittedModels {
    pub fn get(&self, id: WeaponId) -> Option<Fitted> {
        self.0.get(id as usize).copied().flatten()
    }
}

/// The loaded scenes of each weapon's model and sight, by weapon id.
#[derive(Resource)]
struct ModelScenes(Vec<Option<GunScenes>>);

/// A gun's scene, and its sight's if it has one.
type GunScenes = (Handle<WorldAsset>, Option<Handle<WorldAsset>>);

/// One weapon's model under a holder. Hidden until it is fitted.
#[derive(Component)]
struct GunModel {
    weapon: WeaponId,
    fitted: bool,
    shadows: bool,
    sight: SightState,
}

enum SightState {
    None,
    /// Mounted; this is the sight's scene entity, measured once it has loaded.
    Loading(Entity),
    Measured,
}

const SIGHT_MOUNT: &str = "SightMount";
const AIM_POINT: &str = "AimPoint";
const IRON_SIGHT_PREFIX: &str = "IronSight";

fn load_models(mut commands: Commands, asset_server: Res<AssetServer>) {
    let load = |path: &'static str| asset_server.load(GltfAssetLabel::Scene(0).from_asset(path));
    let scenes = WEAPONS
        .iter()
        .map(|w| w.model.as_ref().map(|model| (load(model.path), model.sight.map(load))))
        .collect();
    commands.insert_resource(ModelScenes(scenes));
}

fn attach_models(
    mut commands: Commands,
    scenes: Res<ModelScenes>,
    holders: Query<(Entity, &GunHolder), Added<GunHolder>>,
) {
    for (holder, settings) in &holders {
        for (id, scenes) in scenes.0.iter().enumerate() {
            let Some((scene, _)) = scenes else {
                continue;
            };
            let model = commands
                .spawn((
                    GunModel {
                        weapon: id as WeaponId,
                        fitted: false,
                        shadows: settings.shadows,
                        sight: SightState::None,
                    },
                    Transform::default(),
                    Visibility::Hidden,
                    ChildOf(holder),
                ))
                .id();
            // Its own entity, so the model is measured in its file's units,
            // whatever the fit above it does.
            commands.spawn((WorldAssetRoot(scene.clone()), ChildOf(model)));
        }
    }
}

/// Measures each newly loaded model, fits it in place of the box gun, and
/// mounts its sight.
fn fit_models(
    mut commands: Commands,
    scenes: Res<ModelScenes>,
    mut fitted: ResMut<FittedModels>,
    mut models: Query<(&mut GunModel, &mut Transform, &Children)>,
    descendants: Query<&Children>,
    globals: Query<&GlobalTransform>,
    meshes: Query<(&GlobalTransform, Option<&Aabb>), With<Mesh3d>>,
    names: Query<&Name>,
) {
    for (mut model, mut transform, children) in &mut models {
        if model.fitted {
            continue;
        }
        let Some(&scene) = children.first() else {
            continue;
        };
        let Ok(scene_global) = globals.get(scene) else {
            continue;
        };
        let to_scene = scene_global.affine().inverse();
        let (mut low, mut high) = (Vec3::MAX, Vec3::MIN);
        let mut parts = Vec::new();
        let mut waiting = false;
        for entity in descendants.iter_descendants(scene) {
            let Ok((global, bounds)) = meshes.get(entity) else {
                continue;
            };
            // Bounds are added the frame after the mesh appears.
            let Some(bounds) = bounds else {
                waiting = true;
                break;
            };
            let (min, max) = (Vec3::from(bounds.min()), Vec3::from(bounds.max()));
            for corner in 0..8 {
                let pick = |bit: u32, low: f32, high: f32| if corner & bit == 0 { low } else { high };
                let point = Vec3::new(pick(1, min.x, max.x), pick(2, min.y, max.y), pick(4, min.z, max.z));
                let point = to_scene.transform_point3(global.transform_point(point));
                low = low.min(point);
                high = high.max(point);
            }
            parts.push(entity);
        }
        // Nothing yet: the scene has not been spawned.
        if waiting || parts.is_empty() {
            continue;
        }

        let stats = weapon(model.weapon);
        let size = high - low;
        let Some(spec) = &stats.model else {
            continue;
        };
        if size.z <= 0.0 {
            continue;
        }
        let scale = stats.size[2] / size.z;
        let turn = if spec.barrel_plus_z {
            Quat::from_rotation_y(PI)
        } else {
            Quat::IDENTITY
        };
        transform.rotation = turn;
        transform.scale = Vec3::splat(scale);
        transform.translation = -(turn * ((low + high) / 2.0 * scale));
        model.fitted = true;
        fitted.0[model.weapon as usize] = Some(Fitted {
            size: size * scale,
            aim_point: None,
        });
        if !model.shadows {
            for part in parts {
                commands.entity(part).insert(NotShadowCaster);
            }
        }

        let Some((_, Some(sight))) = &scenes.0[model.weapon as usize] else {
            continue;
        };
        let named = |prefix: &'static str| {
            descendants
                .iter_descendants(scene)
                .filter(move |&e| names.get(e).is_ok_and(|name| name.as_str().starts_with(prefix)))
        };
        let Some(mount) = named(SIGHT_MOUNT).next() else {
            warn!("{} has a sight but no {SIGHT_MOUNT} node", spec.path);
            continue;
        };
        let sight = commands.spawn((WorldAssetRoot(sight.clone()), ChildOf(mount))).id();
        model.sight = SightState::Loading(sight);
        for iron in named(IRON_SIGHT_PREFIX) {
            commands.entity(iron).insert(Visibility::Hidden);
        }
    }
}

/// Finds where a newly loaded sight's dot ended up, for aiming.
fn measure_sights(
    mut commands: Commands,
    mut fitted: ResMut<FittedModels>,
    mut models: Query<(&mut GunModel, &Transform, &Children)>,
    descendants: Query<&Children>,
    globals: Query<&GlobalTransform>,
    meshes: Query<Has<Aabb>, With<Mesh3d>>,
    names: Query<&Name>,
) {
    for (mut model, transform, children) in &mut models {
        let SightState::Loading(sight) = model.sight else {
            continue;
        };
        let mut parts = Vec::new();
        let mut aim = None;
        for entity in descendants.iter_descendants(sight) {
            if let Ok(has_bounds) = meshes.get(entity) {
                parts.push((entity, has_bounds));
            }
            if names.get(entity).is_ok_and(|name| name.as_str() == AIM_POINT) {
                aim = Some(entity);
            }
        }
        // Bounds appear once transforms have caught up with the new parts.
        if parts.is_empty() || parts.iter().any(|&(_, has_bounds)| !has_bounds) {
            continue;
        }
        model.sight = SightState::Measured;
        if !model.shadows {
            for &(part, _) in &parts {
                commands.entity(part).insert(NotShadowCaster);
            }
        }
        let (Some(aim), Some(&scene)) = (aim, children.first()) else {
            warn!("the sight on weapon {} has no {AIM_POINT} node", model.weapon);
            continue;
        };
        let (Ok(aim), Ok(scene)) = (globals.get(aim), globals.get(scene)) else {
            continue;
        };
        // From the gun's file space to the holder, through the fit.
        let in_scene = scene.affine().inverse().transform_point3(aim.translation());
        if let Some(entry) = &mut fitted.0[model.weapon as usize] {
            entry.aim_point = Some(transform.transform_point(in_scene));
        }
    }
}

fn show_models(
    holders: Query<&GunHolder>,
    mut models: Query<(&GunModel, &ChildOf, &mut Visibility)>,
) {
    for (model, parent, mut visibility) in &mut models {
        let shown = model.fitted && holders.get(parent.parent()).is_ok_and(|h| h.weapon == model.weapon);
        let wanted = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        visibility.set_if_neq(wanted);
    }
}
