//! Weapon models from `assets/models`, drawn in place of the box guns for the
//! weapons that have one (see `Weapon::model`).
//!
//! Every gun holder (the first-person gun, and each remote player's gun) gets a
//! child per model and shows the one for the weapon in hand. Once a model has
//! loaded it is measured, turned barrel-forward, centred on the holder and
//! scaled to its weapon's `size` length, so a new model drops in as it is.

use std::f32::consts::PI;

use bevy::{camera::primitives::Aabb, light::NotShadowCaster, prelude::*};

use crate::weapons::{WEAPONS, WeaponId, weapon};

pub struct GunModelPlugin;

impl Plugin for GunModelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ModelSizes>()
            .add_systems(Startup, load_models)
            .add_systems(
                Update,
                (
                    attach_models,
                    fit_models,
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

/// Each modelled weapon's size once its model is fitted (width, height and
/// length in metres, centred on the holder). `None` while it is loading, and
/// for weapons that are drawn as boxes.
#[derive(Resource, Default)]
pub struct ModelSizes([Option<Vec3>; WEAPONS.len()]);

impl ModelSizes {
    pub fn get(&self, id: WeaponId) -> Option<Vec3> {
        self.0.get(id as usize).copied().flatten()
    }
}

/// The loaded scene of each weapon's model, by weapon id.
#[derive(Resource)]
struct ModelScenes(Vec<Option<Handle<WorldAsset>>>);

/// One weapon's model under a holder. Hidden until it is fitted.
#[derive(Component)]
struct GunModel {
    weapon: WeaponId,
    fitted: bool,
    shadows: bool,
}

fn load_models(mut commands: Commands, asset_server: Res<AssetServer>) {
    let scenes = WEAPONS
        .iter()
        .map(|w| {
            w.model
                .as_ref()
                .map(|model| asset_server.load(GltfAssetLabel::Scene(0).from_asset(model.path)))
        })
        .collect();
    commands.insert_resource(ModelScenes(scenes));
}

fn attach_models(
    mut commands: Commands,
    scenes: Res<ModelScenes>,
    holders: Query<(Entity, &GunHolder), Added<GunHolder>>,
) {
    for (holder, settings) in &holders {
        for (id, scene) in scenes.0.iter().enumerate() {
            let Some(scene) = scene else {
                continue;
            };
            let model = commands
                .spawn((
                    GunModel {
                        weapon: id as WeaponId,
                        fitted: false,
                        shadows: settings.shadows,
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

/// Measures each newly loaded model and fits it in place of the box gun.
fn fit_models(
    mut commands: Commands,
    mut sizes: ResMut<ModelSizes>,
    mut models: Query<(&mut GunModel, &mut Transform, &Children)>,
    descendants: Query<&Children>,
    globals: Query<&GlobalTransform>,
    meshes: Query<(&GlobalTransform, Option<&Aabb>), With<Mesh3d>>,
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
        sizes.0[model.weapon as usize] = Some(size * scale);
        if !model.shadows {
            for part in parts {
                commands.entity(part).insert(NotShadowCaster);
            }
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
