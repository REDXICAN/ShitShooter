const IRON_SIGHTS = 0;

const SIGHTS = [
  { name: "Iron sights", path: null },
  { name: "Red dot", path: "../../assets/models/sights/red_dot.glb" },
  { name: "Scope", path: "../../assets/models/sights/scope.glb" },
  { name: "Reflex", path: "../../assets/models/sights/reflex.glb" },
];

const GUNS = [
  { name: "Rifle", damage: 20, magazine: 30, reload: 1.6, interval: 0.11, auto: true, model: "../../assets/models/weapons/M4A1.gltf", barrelPlusZ: true, sights: [1, 3] },
  { name: "SMG", damage: 13, magazine: 36, reload: 1.3, interval: 0.065, auto: true, model: "../../assets/models/weapons/MP7.gltf", barrelPlusZ: true, sights: [1, 3] },
  { name: "Shotgun", damage: 12, pellets: 9, magazine: 6, reload: 2.2, interval: 0.8, auto: false, model: "../../assets/models/weapons/Shotgun.glb", barrelPlusZ: true, sights: [] },
  { name: "Sniper", damage: 90, magazine: 5, reload: 2.4, interval: 1.2, auto: false, model: "../../assets/models/weapons/SCAR.gltf", barrelPlusZ: true, sights: [1, 3, 2] },
  { name: "Pistol", damage: 26, magazine: 12, reload: 1.1, interval: 0.17, auto: false, model: "../../assets/models/weapons/M1911Tactical.gltf", barrelPlusZ: false, sights: [1, 3] },
];

const GRENADES = [
  { name: "Frag", max: 2, recharge: 12, model: "../../assets/models/grenades/frag.glb" },
  { name: "Flash", max: 2, recharge: 15, model: "../../assets/models/grenades/flash.glb" },
  { name: "Smoke", max: 1, recharge: 20, model: "../../assets/models/grenades/smoke.glb" },
];

const KNIFE = { name: "Knife", model: "../../assets/models/weapons/Knife.glb", barrelPlusZ: true };

const BINDS ={ primary: "1", secondary: "2", grenade: "G" };

const DEFAULT_LOADOUT = { primary: 0, secondary: 4, sights: [0, 0, 0, 0, 0], grenade: 0 };
