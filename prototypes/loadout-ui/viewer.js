import * as THREE from "three";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";

const FOV = 22;
const THUMB_W = 240;
const THUMB_H = 100;
const loader = new GLTFLoader();
const scenes = new Map();
const thumbs = new Map();
const waiting = new Set();
let queue = Promise.resolve();
let thumbRenderer = null;
let notifyQueued = false;

function loadScene(path) {
  if (!scenes.has(path)) scenes.set(path, loader.loadAsync(path).then((gltf) => gltf.scene));
  return scenes.get(path);
}

function modelSpec(kind, id) {
  if (kind === "grenade") return { path: GRENADES[id].model, turn: 0 };
  if (kind === "knife") return { path: KNIFE.model, turn: KNIFE.barrelPlusZ ? Math.PI / 2 : -Math.PI / 2 };
  return { path: GUNS[id].model, turn: GUNS[id].barrelPlusZ ? Math.PI / 2 : -Math.PI / 2 };
}

async function mountSight(model, sightId) {
  const path = SIGHTS[sightId] && SIGHTS[sightId].path;
  const mount = model.getObjectByName("SightMount");
  if (!path || !mount) return;
  mount.add((await loadScene(path)).clone(true));
  model.traverse((node) => {
    if (node.name.startsWith("IronSight")) node.visible = false;
  });
}

async function buildModel(kind, id, sightId) {
  const spec = modelSpec(kind, id);
  const model = (await loadScene(spec.path)).clone(true);
  const turn = new THREE.Group();
  turn.add(model);
  turn.rotation.y = spec.turn;
  turn.updateMatrixWorld(true);
  const box = new THREE.Box3().setFromObject(turn);
  const size = box.getSize(new THREE.Vector3());
  const scale = 2 / Math.max(size.x, size.y, size.z);
  turn.scale.setScalar(scale);
  turn.position.copy(box.getCenter(new THREE.Vector3()).multiplyScalar(-scale));
  await mountSight(model, sightId);
  const pivot = new THREE.Group();
  pivot.add(turn);
  pivot.updateMatrixWorld(true);
  return { pivot, box: new THREE.Box3().setFromObject(pivot, true) };
}

function makeScene() {
  const scene = new THREE.Scene();
  const key = new THREE.DirectionalLight(0xffffff, 2.6);
  key.position.set(2, 4, 5);
  const rim = new THREE.DirectionalLight(0xffb070, 1.6);
  rim.position.set(-4, 2, -3);
  scene.add(new THREE.HemisphereLight(0xe8eeff, 0x3a3f52, 3.2), key, rim);
  return scene;
}

function frame(camera, box, aspect) {
  const size = box.getSize(new THREE.Vector3());
  const center = box.getCenter(new THREE.Vector3());
  const tan = Math.tan(THREE.MathUtils.degToRad(FOV / 2));
  const across = Math.hypot(size.x, size.z) / 2;
  const tall = (size.y * 1.25) / 2;
  const dist = Math.max(tall / tan, (across * 1.12) / (tan * aspect)) + across * 0.2;
  camera.aspect = aspect;
  camera.position.set(center.x, center.y + dist * 0.08, dist);
  camera.lookAt(center.x, center.y, 0);
  camera.updateProjectionMatrix();
}

function tune(renderer) {
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.4;
}

function getThumbRenderer() {
  if (thumbRenderer) return thumbRenderer;
  thumbRenderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, preserveDrawingBuffer: true });
  thumbRenderer.setPixelRatio(1);
  thumbRenderer.setSize(THUMB_W, THUMB_H, false);
  tune(thumbRenderer);
  return thumbRenderer;
}

async function renderThumb(kind, id, sightId) {
  const { pivot, box } = await buildModel(kind, id, sightId);
  pivot.rotation.set(0.14, kind === "grenade" ? 0.6 : -0.22, 0);
  const scene = makeScene();
  scene.add(pivot);
  const camera = new THREE.PerspectiveCamera(FOV, 1, 0.01, 100);
  frame(camera, box, THUMB_W / THUMB_H);
  const renderer = getThumbRenderer();
  renderer.render(scene, camera);
  return renderer.domElement.toDataURL("image/png");
}

function notify() {
  if (notifyQueued) return;
  notifyQueued = true;
  setTimeout(() => {
    notifyQueued = false;
    document.dispatchEvent(new CustomEvent("gun-thumb"));
  }, 0);
}

function thumb(kind, id, sightId) {
  const key = `${kind}:${id}:${sightId}`;
  if (thumbs.has(key)) return thumbs.get(key);
  if (waiting.has(key)) return null;
  waiting.add(key);
  queue = queue
    .then(() => renderThumb(kind, id, sightId))
    .then((url) => {
      thumbs.set(key, url);
      notify();
    })
    .catch((error) => console.error(`No image for ${key}`, error));
  return null;
}

function createViewer() {
  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  tune(renderer);
  const canvas = renderer.domElement;
  canvas.title = "Drag to turn";
  const scene = makeScene();
  const camera = new THREE.PerspectiveCamera(FOV, 1, 0.01, 100);
  const st = { key: "", token: 0, pivot: null, box: null, base: 0, time: 0, spin: false, drag: null, alive: true, last: 0, w: 0, h: 0 };
  const mode = document.createElement("button");
  mode.type = "button";
  mode.className = "lt-view-mode";
  mode.textContent = "Spin";

  mode.addEventListener("click", () => {
    st.spin = !st.spin;
    st.time = 0;
    st.base = 0;
    mode.textContent = st.spin ? "Side" : "Spin";
  });
  canvas.addEventListener("pointerdown", (event) => {
    st.drag = { x: event.clientX, base: st.base };
    canvas.setPointerCapture(event.pointerId);
  });
  canvas.addEventListener("pointermove", (event) => {
    if (st.drag) st.base = st.drag.base + (event.clientX - st.drag.x) * 0.012;
  });
  canvas.addEventListener("pointerup", () => {
    st.drag = null;
  });

  function resizeIfNeeded() {
    const host = canvas.parentElement;
    const w = host.clientWidth;
    const h = host.clientHeight;
    if (w === st.w && h === st.h) return;
    st.w = w;
    st.h = h;
    renderer.setSize(w, h);
    frame(camera, st.box, w / h);
  }

  function tick(now) {
    if (!st.alive) return;
    requestAnimationFrame(tick);
    const dt = Math.min(0.05, (now - (st.last || now)) / 1000);
    st.last = now;
    if (!st.pivot || !canvas.isConnected || canvas.offsetParent === null) return;
    if (!st.drag) st.time += dt;
    resizeIfNeeded();
    const swing = st.spin ? st.time * 0.9 : 0.42 * Math.sin(st.time * 0.9) - 0.25;
    st.pivot.rotation.set(0.12, st.base + swing, 0);
    renderer.render(scene, camera);
  }

  async function show(kind, id, sightId) {
    const key = `${kind}:${id}:${sightId}`;
    if (key === st.key) return;
    st.key = key;
    const token = ++st.token;
    const built = await buildModel(kind, id, sightId);
    if (token !== st.token || !st.alive) return;
    if (st.pivot) scene.remove(st.pivot);
    st.pivot = built.pivot;
    st.box = built.box;
    st.w = 0;
    scene.add(st.pivot);
  }

  requestAnimationFrame(tick);
  return {
    place(host) {
      host.append(canvas, mode);
      st.w = 0;
    },
    show,
    destroy() {
      st.alive = false;
      renderer.dispose();
      canvas.remove();
      mode.remove();
    },
  };
}

window.GunViews = { thumb, createViewer };
document.dispatchEvent(new CustomEvent("gunviews-ready"));
