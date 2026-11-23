const store = createStore();
const ui = { screen: "pause", type: "arrows", size: "1280x720" };
const mounted = {};
const PAUSE_NOTE = "Handed out the next time you spawn.";

function freshRoot(id) {
  const fresh = document.createElement("div");
  fresh.id = id;
  document.getElementById(id).replaceWith(fresh);
  return fresh;
}

function mount(name, note) {
  if (mounted[name]) mounted[name].destroy();
  mounted[name] = mountType(LOADOUT_TYPES[ui.type], freshRoot(`${name}-loadout`), store, { note });
}

function mountAll() {
  mount("pause", PAUSE_NOTE);
  mount("lobby", "");
}

function markToolbar() {
  document.querySelectorAll(".bar-group").forEach((group) => {
    group.querySelectorAll("button").forEach((button) => {
      button.classList.toggle("is-on", ui[group.dataset.group] === button.dataset.value);
    });
  });
}

function applyScreens() {
  document.querySelectorAll(".screen").forEach((screen) => {
    screen.hidden = screen.id !== `screen-${ui.screen}`;
  });
  markToolbar();
}

function reportFit(width, height) {
  const screen = document.getElementById(`screen-${ui.screen}`);
  const panel = screen.querySelector(".panel");
  const card = screen.querySelector(".card:has([id$='-loadout'])");
  const fits = panel.offsetWidth <= width - 16 && panel.offsetHeight <= height - 16;
  const label = document.getElementById("fit");
  const cardSize = `loadout card ${card.offsetWidth}×${card.offsetHeight}`;
  label.textContent = `${fits ? "Fits" : "Too big for"} ${width}×${height} · panel ${panel.offsetWidth}×${panel.offsetHeight} · ${cardSize}`;
  label.classList.toggle("is-bad", !fits);
}

function fit() {
  const [width, height] = ui.size.split("x").map(Number);
  const box = document.getElementById("viewport").getBoundingClientRect();
  const scale = Math.min(box.width / width, box.height / height, 1);
  Object.assign(document.getElementById("stage").style, {
    width: `${width}px`,
    height: `${height}px`,
    transform: `scale(${scale})`,
    left: `${(box.width - width * scale) / 2}px`,
    top: `${(box.height - height * scale) / 2}px`,
  });
  reportFit(width, height);
}

function onToolbar(event) {
  const button = event.target.closest(".bar-group button");
  if (!button) return;
  const key = button.closest(".bar-group").dataset.group;
  ui[key] = button.dataset.value;
  if (key === "type") mountAll();
  applyScreens();
  fit();
}

function onKey(event) {
  if (event.key === "Escape" && mounted[ui.screen].back()) event.preventDefault();
}

document.querySelector(".bar").addEventListener("click", onToolbar);
document.getElementById("reset").addEventListener("click", () => store.set(defaultLoadout()));
document.addEventListener("keydown", onKey);
window.addEventListener("resize", fit);

mountAll();
applyScreens();
fit();

const watcher = new ResizeObserver(fit);
document.querySelectorAll(".panel").forEach((panel) => watcher.observe(panel));
