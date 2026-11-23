//! The loadout card, shown in the lobby, the pause menu and on the death
//! screen, and the pictures of guns and grenades it and the HUD show.
//!
//! Each picture is drawn by a camera of its own that renders into an image
//! rather than the window, looking at a copy of the gun far below the map, on
//! a render layer of its own so nothing else shows in it:
//!
//! - `Thumbnails`: a side view of every gun with every sight it takes, and of
//!   every grenade. Each camera runs until its picture is taken, then stops
//!   and the copy of the gun goes; the image keeps the picture.
//! - The stage: a live 3D view at the top of the card, of what the pointer is
//!   on, or else of the slot last changed. Pointing at an arrow shows what it
//!   would give.

use std::collections::HashMap;

use bevy::{
    app::{HierarchyPropagatePlugin, Propagate, PropagateSet},
    camera::{
        RenderTarget,
        primitives::Aabb,
        visibility::{RenderLayers, VisibilitySystems},
    },
    math::Affine3A,
    picking::hover::Hovered,
    prelude::*,
    render::render_resource::TextureFormat,
    render::view::screenshot::{Screenshot, ScreenshotCaptured},
    ui_widgets::{Activate, Button, observe},
};

use crate::{
    gun_model::{FittedModels, GunHolder},
    protocol::{GRENADE_KINDS, GrenadeKind, grenade},
    settings::{Action, Settings},
    ui::{self, BUTTON, DIM, KeyHint, TEXT, label, section, set_shown, set_text},
    weapons::{GUN_COUNT, IRON_SIGHTS, KNIFE, SightId, WeaponId, sight, sight_on, sights_for, takes_sights, weapon},
    world::GRENADE_MODELS,
};

/// Thumbnails are this many pixels, larger than they are shown so they stay sharp.
const THUMB_SIZE: UVec2 = UVec2::new(320, 120);
const STAGE_SIZE: UVec2 = UVec2::new(640, 240);
/// Width over height of both: the stage on the card keeps the same shape.
const ASPECT: f32 = 8.0 / 3.0;
/// The render layer of the stage. Thumbnails take the ones after it. (The
/// paint screen has one below it.)
const STAGE_LAYER: usize = 8;
/// Where the copies of the guns are, far below the map.
const PREVIEW_ORIGIN: Vec3 = Vec3::new(0.0, -500.0, 0.0);
/// Vertical field of view of the stage camera, in radians.
const STAGE_FOV: f32 = 0.45;
/// The thumbnails' cameras look from far off, so the guns look nearly flat.
const THUMB_FOV: f32 = 0.12;

pub struct LoadoutPlugin;

impl Plugin for LoadoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(HierarchyPropagatePlugin::<RenderLayers>::new(PostUpdate))
            // Models spawned this frame are on their layer before anything is drawn.
            .configure_sets(
                PostUpdate,
                PropagateSet::<RenderLayers>::default().before(VisibilitySystems::CheckVisibility),
            )
            .init_resource::<LoadoutFocus>()
            .add_systems(Startup, setup)
            .add_systems(Update, (frame_thumbnails, update_cards, show_stage).chain());
    }
}

/// Something to take a picture of: a gun with a sight, or a grenade.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Preview {
    Gun(WeaponId, SightId),
    Grenade(GrenadeKind),
}

impl Preview {
    /// A gun with the sight it really carries when `wanted` is asked for.
    pub fn gun(weapon_id: WeaponId, wanted: SightId) -> Preview {
        Preview::Gun(weapon_id, sight_on(weapon_id, wanted))
    }

    /// Every gun with each sight it takes, the knife, and every grenade.
    fn all() -> Vec<Preview> {
        let guns = (0..GUN_COUNT).chain([KNIFE]).flat_map(|gun| {
            std::iter::once(IRON_SIGHTS)
                .chain(sights_for(gun).iter().copied())
                .map(move |sight| Preview::Gun(gun, sight))
        });
        let grenades = (0..GRENADE_KINDS as GrenadeKind).map(Preview::Grenade);
        guns.chain(grenades).collect()
    }

    /// What the stage says under it.
    fn caption(self) -> String {
        match self {
            Preview::Gun(gun, sight_id) if takes_sights(gun) => {
                format!("{}  -  {}", weapon(gun).name, sight(sight_id).name)
            }
            Preview::Gun(KNIFE, _) => weapon(KNIFE).name.to_string(),
            Preview::Gun(gun, _) => format!("{}  -  no sight mount", weapon(gun).name),
            Preview::Grenade(kind) => format!("{} grenade", grenade(kind).name),
        }
    }
}

/// A side view of every `Preview::all`, the barrel pointing right, on a
/// see-through background.
#[derive(Resource)]
pub struct Thumbnails(HashMap<Preview, Handle<Image>>);

impl Thumbnails {
    /// The picture of a gun or grenade (empty until its model has loaded).
    pub fn get(&self, preview: Preview) -> Option<Handle<Image>> {
        self.0.get(&preview).cloned()
    }
}

/// The image the stage camera draws into.
#[derive(Resource)]
struct Stage(Handle<Image>);

/// The chevron on the arrow buttons, pointing right.
#[derive(Resource)]
struct Chevron(Handle<Image>);

/// What the stage shows when the pointer is on none of the card: the slot last
/// changed. A click leaves the pointer on the arrow, but until it moves off it
/// the stage shows what was picked, not what a second click would give.
#[derive(Resource)]
struct LoadoutFocus {
    slot: Slot,
    clicked: Option<Entity>,
}

impl Default for LoadoutFocus {
    fn default() -> Self {
        LoadoutFocus {
            slot: Slot::Gun(0),
            clicked: None,
        }
    }
}

/// The camera of one thumbnail, until it is taken.
#[derive(Component)]
struct ThumbnailCamera {
    preview: Preview,
    subject: Entity,
    /// The visible bounds of the subject last frame, and how many frames in a
    /// row they have stayed the same.
    bounds: Option<(Vec3, Vec3)>,
    still_for: u32,
    /// Seconds until the picture is next read back to be checked.
    check_in: f32,
    /// A read back is on its way.
    checking: bool,
    /// What the last picture read back held, and how many in a row have held
    /// the same (with something in them).
    last_picture: Option<u64>,
    same_pictures: u32,
}

#[derive(Component)]
struct StageCamera;
/// What the stage camera looks at: turns slowly to show it is 3D.
#[derive(Component)]
struct StageSubject;
#[derive(Component)]
struct StageGun;
#[derive(Component)]
struct StageGrenade(GrenadeKind);

fn target_image(size: UVec2) -> Image {
    Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8Unorm, Some(TextureFormat::Rgba8UnormSrgb))
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut images: ResMut<Assets<Image>>) {
    let grenade_scene = |kind: GrenadeKind| asset_server.load(GltfAssetLabel::Scene(0).from_asset(GRENADE_MODELS[kind as usize]));
    let previews = Preview::all();
    let layers = RenderLayers::from_layers(&(STAGE_LAYER..=STAGE_LAYER + previews.len()).collect::<Vec<_>>());
    // A key light from above in front, and a softer fill from behind, for
    // every picture and nothing else.
    commands.spawn((
        DirectionalLight {
            illuminance: 7000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(1.0, 2.0, -0.8).looking_at(Vec3::ZERO, Vec3::Y),
        layers.clone(),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 2500.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-1.0, 0.3, 1.5).looking_at(Vec3::ZERO, Vec3::Y),
        layers,
    ));

    let mut thumbnails = HashMap::new();
    for (index, preview) in previews.into_iter().enumerate() {
        let layer = RenderLayers::layer(STAGE_LAYER + 1 + index);
        let at = PREVIEW_ORIGIN + Vec3::new(0.0, 0.0, 4.0 * (index + 1) as f32);
        let subject = commands
            .spawn((Transform::from_translation(at), Visibility::default(), Propagate(layer.clone())))
            .id();
        match preview {
            Preview::Gun(gun, sight) => {
                commands.entity(subject).insert(GunHolder {
                    weapon: gun,
                    sight,
                    shadows: false,
                });
            }
            Preview::Grenade(kind) => {
                commands.spawn((WorldAssetRoot(grenade_scene(kind)), ChildOf(subject)));
            }
        }
        let image = images.add(target_image(THUMB_SIZE));
        commands.spawn((
            ThumbnailCamera {
                preview,
                subject,
                bounds: None,
                still_for: 0,
                check_in: 0.0,
                checking: false,
                last_picture: None,
                same_pictures: 0,
            },
            Camera3d::default(),
            Camera {
                order: -2 - index as isize,
                // Until there is something to take a picture of.
                is_active: false,
                clear_color: ClearColorConfig::Custom(Color::NONE),
                ..default()
            },
            RenderTarget::Image(image.clone().into()),
            Projection::from(PerspectiveProjection {
                fov: THUMB_FOV,
                near: 0.05,
                aspect_ratio: ASPECT,
                ..default()
            }),
            Transform::from_translation(at + Vec3::X).looking_at(at, Vec3::Y),
            layer,
        ));
        thumbnails.insert(preview, image);
    }
    commands.insert_resource(Thumbnails(thumbnails));

    // The stage: one gun holder, and each grenade, shown one at a time.
    let layer = RenderLayers::layer(STAGE_LAYER);
    let stage = images.add(target_image(STAGE_SIZE));
    let subject = commands
        .spawn((
            StageSubject,
            Transform::from_translation(PREVIEW_ORIGIN),
            Visibility::default(),
            Propagate(layer.clone()),
            children![(
                StageGun,
                GunHolder {
                    weapon: 0,
                    sight: IRON_SIGHTS,
                    shadows: false,
                },
                Transform::default(),
                Visibility::default(),
            )],
        ))
        .id();
    for kind in 0..GRENADE_KINDS as GrenadeKind {
        commands.spawn((
            StageGrenade(kind),
            WorldAssetRoot(grenade_scene(kind)),
            Transform::default(),
            Visibility::Hidden,
            ChildOf(subject),
        ));
    }
    commands.spawn((
        StageCamera,
        Camera3d::default(),
        Camera {
            order: -1,
            is_active: false,
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
        RenderTarget::Image(stage.clone().into()),
        Projection::from(PerspectiveProjection {
            fov: STAGE_FOV,
            near: 0.01,
            aspect_ratio: ASPECT,
            ..default()
        }),
        Transform::from_translation(PREVIEW_ORIGIN + Vec3::X).looking_at(PREVIEW_ORIGIN, Vec3::Y),
        layer,
    ));
    commands.insert_resource(Stage(stage));
    commands.insert_resource(Chevron(images.add(chevron_image())));
}

/// A "›", white on see-through.
fn chevron_image() -> Image {
    // Distance from (u, v) to the segment from a to b.
    let to_segment = |u: f32, v: f32, a: Vec2, b: Vec2| {
        let p = Vec2::new(u, v);
        let t = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
        p.distance(a + (b - a) * t)
    };
    let (top, tip, bottom) = (Vec2::new(-0.3, -0.62), Vec2::new(0.3, 0.0), Vec2::new(-0.3, 0.62));
    ui::mask_image(64, [255, 255, 255], |u, v, pixel| {
        let distance = to_segment(u, v, top, tip).min(to_segment(u, v, tip, bottom));
        (0.13 - distance) / pixel + 0.5
    })
}

/// The bounds of the parts of `root` that are shown, in the space `to_space`
/// takes world points to. `None` while any part is still missing its bounds,
/// or nothing is shown yet.
fn visible_bounds(
    root: Entity,
    to_space: Affine3A,
    descendants: &Query<&Children>,
    meshes: &Query<(&GlobalTransform, Option<&Aabb>, &InheritedVisibility), With<Mesh3d>>,
) -> Option<(Vec3, Vec3)> {
    let (mut low, mut high) = (Vec3::MAX, Vec3::MIN);
    for entity in descendants.iter_descendants(root) {
        let Ok((global, bounds, visible)) = meshes.get(entity) else {
            continue;
        };
        let bounds = bounds?;
        if !visible.get() {
            continue;
        }
        let (min, max) = (Vec3::from(bounds.min()), Vec3::from(bounds.max()));
        for corner in 0..8 {
            let pick = |bit: u32, low: f32, high: f32| if corner & bit == 0 { low } else { high };
            let corner = Vec3::new(pick(1, min.x, max.x), pick(2, min.y, max.y), pick(4, min.z, max.z));
            let point = to_space.transform_point3(global.transform_point(corner));
            low = low.min(point);
            high = high.max(point);
        }
    }
    (low.x <= high.x).then_some((low, high))
}

/// Frames each thumbnail's subject once it has loaded, and keeps its camera
/// running until the picture is surely taken: a model is not drawn at all
/// until the GPU has its shaders ready, which can take a while after it has
/// loaded. So the picture is read back now and then, and once a few in a row
/// have something in them and are the same, the camera and its subject go.
/// The image keeps the picture.
fn frame_thumbnails(
    mut commands: Commands,
    time: Res<Time>,
    thumbnails: Res<Thumbnails>,
    fitted: Res<FittedModels>,
    mut cameras: Query<(Entity, &mut ThumbnailCamera, &mut Camera, &mut Transform)>,
    descendants: Query<&Children>,
    meshes: Query<(&GlobalTransform, Option<&Aabb>, &InheritedVisibility), With<Mesh3d>>,
    names: Query<(&Name, &InheritedVisibility)>,
) {
    for (entity, mut thumb, mut camera, mut transform) in &mut cameras {
        let ready = match thumb.preview {
            Preview::Gun(gun, sight_id) => {
                fitted.get(gun).is_some()
                    // A sight is in once its dot is there to be seen.
                    && (sight_id == IRON_SIGHTS
                        || descendants.iter_descendants(thumb.subject).any(|part| {
                            names.get(part).is_ok_and(|(name, shown)| name.as_str() == "AimPoint" && shown.get())
                        }))
            }
            Preview::Grenade(_) => true,
        };
        let bounds = visible_bounds(thumb.subject, Affine3A::IDENTITY, &descendants, &meshes).filter(|_| ready);
        let Some((low, high)) = bounds else {
            thumb.still_for = 0;
            continue;
        };
        if thumb.bounds != bounds {
            thumb.bounds = bounds;
            thumb.still_for = 0;
            thumb.last_picture = None;
            thumb.same_pictures = 0;
            camera.is_active = true;
            // From the side, from far off through a narrow lens so it is
            // nearly flat, filling the picture with a small margin.
            let center = (low + high) / 2.0;
            let size = high - low;
            let half = (THUMB_FOV / 2.0).tan();
            let distance = (size.z * 0.54 / (half * ASPECT)).max(size.y * 0.54 / half) + size.x / 2.0;
            *transform = Transform::from_translation(center + Vec3::X * distance).looking_at(center, Vec3::Y);
            continue;
        }
        thumb.still_for += 1;
        thumb.check_in -= time.delta_secs();
        if thumb.still_for < 3 || thumb.checking || thumb.check_in > 0.0 {
            continue;
        }
        let Some(image) = thumbnails.get(thumb.preview) else {
            continue;
        };
        thumb.checking = true;
        thumb.check_in = 0.3;
        commands.spawn(Screenshot::image(image)).observe(
            move |captured: On<ScreenshotCaptured>,
                  mut commands: Commands,
                  mut cameras: Query<(&mut ThumbnailCamera, &mut Camera)>| {
                let Ok((mut thumb, mut camera)) = cameras.get_mut(entity) else {
                    return;
                };
                thumb.checking = false;
                let pixels = captured.image.data.as_deref().unwrap_or_default();
                let drawn = pixels.chunks_exact(4).any(|pixel| pixel[3] > 0);
                let picture = drawn.then(|| {
                    let mut hasher = std::hash::DefaultHasher::new();
                    std::hash::Hash::hash(pixels, &mut hasher);
                    std::hash::Hasher::finish(&hasher)
                });
                thumb.same_pictures = if picture.is_some() && picture == thumb.last_picture {
                    thumb.same_pictures + 1
                } else {
                    0
                };
                thumb.last_picture = picture;
                if thumb.same_pictures >= 2 {
                    debug!("thumbnail of {:?} taken", thumb.preview);
                    camera.is_active = false;
                    commands.entity(thumb.subject).despawn();
                    commands.entity(entity).remove::<ThumbnailCamera>();
                }
            },
        );
    }
}

// ---------------------------------------------------------------- the card

/// One of the three things a loadout holds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    /// 0 = primary, 1 = secondary.
    Gun(usize),
    Grenade,
}

impl Slot {
    fn preview(self, settings: &Settings) -> Preview {
        match self {
            Slot::Gun(slot) => {
                let gun = settings.gun(slot);
                Preview::gun(gun, settings.sight_for(gun))
            }
            Slot::Grenade => Preview::Grenade(settings.grenade),
        }
    }

    fn action(self) -> Action {
        match self {
            Slot::Gun(0) => Action::Primary,
            Slot::Gun(_) => Action::Secondary,
            Slot::Grenade => Action::Grenade,
        }
    }

    fn caption(self) -> &'static str {
        match self {
            Slot::Gun(0) => "PRIMARY",
            Slot::Gun(_) => "SECONDARY",
            Slot::Grenade => "GRENADE",
        }
    }
}

/// The loadout card. Everything in it reads and changes `Settings`.
#[derive(Component)]
pub struct LoadoutCard;

/// A button that steps the gun, the sight on it, or the grenade, back or on.
#[derive(Component, Clone, Copy)]
pub struct LoadoutArrow {
    pub slot: Slot,
    pub sight: bool,
    /// -1 or 1.
    pub step: i32,
}

impl LoadoutArrow {
    fn apply(self, settings: &mut Settings) {
        match (self.slot, self.sight) {
            (Slot::Gun(slot), false) => settings.step_gun(slot, self.step),
            (Slot::Gun(slot), true) => settings.step_sight(slot, self.step),
            (Slot::Grenade, _) => settings.step_grenade(self.step),
        }
    }

    /// What its slot would hold after a click.
    fn preview(self, settings: &Settings) -> Preview {
        let mut after = settings.clone();
        self.apply(&mut after);
        self.slot.preview(&after)
    }
}

/// The box with a slot's name and picture.
#[derive(Component)]
struct SlotBox(Slot);
#[derive(Component)]
struct SlotName(Slot);
#[derive(Component)]
struct SlotImage(Slot);
/// The row with the sight on a slot's gun and its arrows; only for a gun
/// that takes sights.
#[derive(Component)]
struct SightRow(usize);
#[derive(Component)]
struct SightName(usize);
/// "No sight mount", in place of the sight row.
#[derive(Component)]
struct NoMount(usize);
/// The 3D view at the top of a card.
#[derive(Component)]
struct StageView;
#[derive(Component)]
struct StageCaption;
/// The image in an arrow button.
#[derive(Component)]
struct ChevronIcon;

/// The card: a 3D view, the two guns with their sights, and the grenade,
/// each with arrows that step through the choices. `footer` goes under it.
pub fn loadout_card(width: f32, background: Color, footer: impl Bundle) -> impl Bundle {
    (
        LoadoutCard,
        Node {
            width: px(width),
            ..ui::card_node()
        },
        BackgroundColor(background),
        BorderColor::all(ui::HUD_EDGE),
        children![
            section("Loadout"),
            stage_view(),
            gun_slot(0),
            gun_slot(1),
            (arrows_around(Slot::Grenade, false, slot_box(Slot::Grenade)), Node { column_gap: px(6), ..default() }),
            footer,
        ],
    )
}

fn stage_view() -> impl Bundle {
    (
        StageView,
        Node {
            width: percent(100),
            aspect_ratio: Some(ASPECT),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(6)),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.07)),
        BorderColor::all(ui::HUD_EDGE),
        Pickable::IGNORE,
        children![
            (
                ImageNode::default().with_color(Color::NONE),
                Node {
                    width: percent(100),
                    height: percent(100),
                    ..default()
                },
                Pickable::IGNORE,
            ),
            (
                StageCaption,
                label("", 13.0, TEXT),
                TextShadow {
                    offset: Vec2::splat(1.0),
                    color: Color::srgba(0.0, 0.0, 0.0, 0.8),
                },
                Node {
                    position_type: PositionType::Absolute,
                    left: px(9),
                    bottom: px(6),
                    ..default()
                },
                Pickable::IGNORE,
            ),
        ],
    )
}

fn gun_slot(slot: usize) -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(5),
            ..default()
        },
        children![
            (arrows_around(Slot::Gun(slot), false, slot_box(Slot::Gun(slot))), Node { column_gap: px(6), ..default() }),
            (
                SightRow(slot),
                arrows_around(Slot::Gun(slot), true, (
                    SightName(slot),
                    label("", 14.0, TEXT),
                    TextLayout::justify(Justify::Center),
                    Node {
                        flex_grow: 1.0,
                        flex_basis: px(0),
                        ..default()
                    },
                    Pickable::IGNORE,
                )),
                Node {
                    column_gap: px(6),
                    align_items: AlignItems::Center,
                    ..default()
                },
            ),
            (
                NoMount(slot),
                label("No sight mount", 13.0, DIM),
                TextLayout::justify(Justify::Center),
                Node {
                    height: px(22),
                    ..default()
                },
                Pickable::IGNORE,
            ),
        ],
    )
}

/// `inner` with a back arrow before it and an on arrow after it.
fn arrows_around(slot: Slot, sight: bool, inner: impl Bundle) -> impl Bundle {
    children![arrow(slot, sight, -1), inner, arrow(slot, sight, 1)]
}

fn arrow(slot: Slot, sight: bool, step: i32) -> impl Bundle {
    let arrow = LoadoutArrow { slot, sight, step };
    let (width, height, icon) = if sight { (24.0, 22.0, 11.0) } else { (28.0, 48.0, 15.0) };
    (
        arrow,
        Button,
        Hovered::default(),
        Node {
            width: px(width),
            height: px(height),
            flex_shrink: 0.0,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(6)),
            ..default()
        },
        BackgroundColor(BUTTON),
        BorderColor::all(ui::BUTTON_EDGE),
        children![(
            ChevronIcon,
            ImageNode::default().with_color(Color::NONE),
            Node {
                width: px(icon),
                height: px(icon),
                ..default()
            },
            // The back arrow is the on arrow mirrored.
            UiTransform::from_scale(Vec2::new(step as f32, 1.0)),
            Pickable::IGNORE,
        )],
        observe(
            move |activate: On<Activate>, mut settings: ResMut<Settings>, mut focus: ResMut<LoadoutFocus>| {
                arrow.apply(&mut settings);
                settings.save();
                focus.slot = slot;
                focus.clicked = Some(activate.entity);
            },
        ),
    )
}

/// A slot's key and caption, the name of what is in it, and its picture.
fn slot_box(slot: Slot) -> impl Bundle {
    (
        SlotBox(slot),
        Hovered::default(),
        Node {
            // All the same width, whatever they hold.
            flex_grow: 1.0,
            flex_basis: px(0),
            min_width: px(0),
            height: px(48),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            column_gap: px(6),
            padding: UiRect::new(px(9), px(4), px(0), px(0)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(6)),
            ..default()
        },
        BackgroundColor(ui::CARD),
        BorderColor::all(ui::HUD_EDGE),
        children![
            (
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(2),
                    ..default()
                },
                Pickable::IGNORE,
                children![
                    (
                        Node {
                            align_items: AlignItems::Center,
                            column_gap: px(6),
                            ..default()
                        },
                        Pickable::IGNORE,
                        children![key_chip(slot.action()), (label(slot.caption(), 11.0, DIM), Pickable::IGNORE)],
                    ),
                    (SlotName(slot), label("", 17.0, TEXT), Pickable::IGNORE),
                ],
            ),
            (
                SlotImage(slot),
                ImageNode::default().with_color(Color::NONE),
                Node {
                    width: px(104),
                    height: px(39),
                    flex_shrink: 0.0,
                    ..default()
                },
                Pickable::IGNORE,
            ),
        ],
    )
}

/// A small box with the key an action is bound to.
fn key_chip(action: Action) -> impl Bundle {
    (
        Node {
            min_width: px(18),
            height: px(16),
            padding: UiRect::axes(px(4), px(0)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BackgroundColor(ui::FIELD),
        BorderColor::all(ui::BUTTON_EDGE),
        Pickable::IGNORE,
        children![(label("", 11.0, TEXT), KeyHint(action), Pickable::IGNORE)],
    )
}

/// Keeps every card's names, pictures and sight rows in step with the loadout.
fn update_cards(
    settings: Res<Settings>,
    thumbnails: Res<Thumbnails>,
    chevron: Res<Chevron>,
    mut texts: ParamSet<(Query<(&SlotName, &mut Text)>, Query<(&SightName, &mut Text)>)>,
    mut images: ParamSet<(Query<(&SlotImage, &mut ImageNode)>, Query<&mut ImageNode, Added<ChevronIcon>>)>,
    mut rows: ParamSet<(Query<(&SightRow, &mut Node)>, Query<(&NoMount, &mut Node)>)>,
    added: Query<(), Added<LoadoutCard>>,
) {
    for mut icon in &mut images.p1() {
        icon.image = chevron.0.clone();
        icon.color = Color::WHITE;
    }
    // The thumbnails fill in over the first frames, so those are checked always.
    for (slot, mut image) in &mut images.p0() {
        if let Some(handle) = thumbnails.get(slot.0.preview(&settings))
            && image.image != handle
        {
            image.image = handle;
            image.color = Color::WHITE;
        }
    }
    if !settings.is_changed() && added.is_empty() {
        return;
    }
    for (slot, mut text) in &mut texts.p0() {
        let name = match slot.0 {
            Slot::Gun(slot) => weapon(settings.gun(slot)).name,
            Slot::Grenade => grenade(settings.grenade).name,
        };
        set_text(&mut text, name.to_string());
    }
    for (slot, mut text) in &mut texts.p1() {
        set_text(&mut text, sight(settings.sight_for(settings.gun(slot.0))).name.to_string());
    }
    for (row, mut node) in &mut rows.p0() {
        set_shown(&mut node, takes_sights(settings.gun(row.0)));
    }
    for (row, mut node) in &mut rows.p1() {
        set_shown(&mut node, !takes_sights(settings.gun(row.0)));
    }
}

/// Shows on the stage what the pointer is on (for an arrow: what it would
/// give), or else the slot last changed; and runs its camera only while a
/// card is on screen.
fn show_stage(
    time: Res<Time>,
    settings: Res<Settings>,
    stage: Res<Stage>,
    mut focus: ResMut<LoadoutFocus>,
    arrows: Query<(Entity, &LoadoutArrow, &Hovered)>,
    boxes: Query<(&SlotBox, &Hovered)>,
    views: Query<(&ComputedNode, &InheritedVisibility, &Children), With<StageView>>,
    mut view_images: Query<&mut ImageNode>,
    mut captions: Query<&mut Text, With<StageCaption>>,
    camera: Single<(&mut Camera, &mut Transform), With<StageCamera>>,
    subject: Single<(Entity, &mut Transform), (With<StageSubject>, Without<StageCamera>)>,
    mut gun: Single<&mut GunHolder, With<StageGun>>,
    mut grenades: Query<(&StageGrenade, &mut Visibility)>,
    descendants: Query<&Children>,
    meshes: Query<(&GlobalTransform, Option<&Aabb>, &InheritedVisibility), With<Mesh3d>>,
    globals: Query<&GlobalTransform>,
) {
    let (mut camera, mut camera_at) = camera.into_inner();
    let on_screen = views.iter().any(|(node, visible, _)| visible.get() && node.size().x > 0.0);
    if camera.is_active != on_screen {
        camera.is_active = on_screen;
    }
    if !on_screen {
        return;
    }
    for (.., children) in &views {
        for child in children.iter() {
            if let Ok(mut image) = view_images.get_mut(child)
                && image.image != stage.0
            {
                image.image = stage.0.clone();
                image.color = Color::WHITE;
            }
        }
    }

    // Once the pointer has left the arrow just clicked, it previews again.
    if let Some(clicked) = focus.clicked
        && !arrows.get(clicked).is_ok_and(|(_, _, hovered)| hovered.get())
    {
        focus.clicked = None;
    }
    let pointed = arrows
        .iter()
        .find(|(entity, _, hovered)| hovered.get() && focus.clicked != Some(*entity))
        .map(|(_, arrow, _)| arrow.preview(&settings));
    let boxed = boxes.iter().find(|(_, hovered)| hovered.get()).map(|(slot, _)| slot.0.preview(&settings));
    let preview = pointed.or(boxed).unwrap_or_else(|| focus.slot.preview(&settings));

    for mut caption in &mut captions {
        set_text(&mut caption, preview.caption());
    }
    let (gun_id, sight_id, kind) = match preview {
        Preview::Gun(gun_id, sight_id) => (Some(gun_id), sight_id, None),
        Preview::Grenade(kind) => (None, IRON_SIGHTS, Some(kind)),
    };
    // A weapon id no holder shows hides the gun.
    let shown_gun = gun_id.unwrap_or(WeaponId::MAX);
    if gun.weapon != shown_gun || gun.sight != sight_id {
        gun.weapon = shown_gun;
        gun.sight = sight_id;
    }
    for (grenade, mut visibility) in &mut grenades {
        visibility.set_if_neq(if Some(grenade.0) == kind {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }

    // It sways from side to side, and the camera keeps all of it in view:
    // measured unturned, so the view does not breathe in and out as it turns.
    let (subject, mut turn) = subject.into_inner();
    turn.rotation = Quat::from_rotation_y((time.elapsed_secs() * 0.7).sin() * 0.35);
    let Ok(subject_global) = globals.get(subject) else {
        return;
    };
    let Some((low, high)) = visible_bounds(subject, subject_global.affine().inverse(), &descendants, &meshes) else {
        return;
    };
    let center = turn.translation + (low + high) / 2.0;
    let size = (high - low).max(Vec3::splat(0.02));
    let half = (STAGE_FOV / 2.0).tan();
    let across = size.x.max(size.z);
    let distance = (across * 0.54 / (half * ASPECT)).max(size.y * 0.6 / half) + size.x / 2.0;
    let from = Vec3::new(1.0, 0.16, -0.22).normalize();
    *camera_at = Transform::from_translation(center + from * distance).looking_at(center, Vec3::Y);
}
