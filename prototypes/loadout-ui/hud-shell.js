const hudStore = createStore();
const hudUi = { hud: "compact", reload: "auto", size: "1280x720" };
const hudApi = createHud(document.getElementById("hud-weapons"), hudStore);

function markHudToolbar() {
  document.querySelectorAll(".bar-group").forEach((group) => {
    group.querySelectorAll("button").forEach((button) => {
      button.classList.toggle("is-on", hudUi[group.dataset.group] === button.dataset.value);
    });
  });
  const auto = document.querySelector('[data-group="reload"] [data-value="auto"]');
  auto.textContent = `Type default (${RELOAD_NAMES[RELOAD_DEFAULT[hudUi.hud]]})`;
}

function fitHud() {
  const [width, height] = hudUi.size.split("x").map(Number);
  const box = document.getElementById("viewport").getBoundingClientRect();
  const scale = Math.min(box.width / width, box.height / height, 1);
  Object.assign(document.getElementById("stage").style, {
    width: `${width}px`,
    height: `${height}px`,
    transform: `scale(${scale})`,
    left: `${(box.width - width * scale) / 2}px`,
    top: `${(box.height - height * scale) / 2}px`,
  });
  const panel = document.getElementById("hud-weapons");
  document.getElementById("fit").textContent = `${width}×${height} · weapon panel ${panel.offsetWidth}×${panel.offsetHeight}`;
}

function onHudToolbar(event) {
  const button = event.target.closest(".bar-group button");
  if (!button) return;
  const key = button.closest(".bar-group").dataset.group;
  hudUi[key] = button.dataset.value;
  if (key === "hud") hudApi.setType(hudUi.hud);
  if (key === "reload") hudApi.setReload(hudUi.reload);
  markHudToolbar();
  fitHud();
}

document.querySelector(".bar").addEventListener("click", onHudToolbar);
document.addEventListener("keydown", (event) => {
  if (!event.repeat && hudApi.key(event.key)) event.preventDefault();
});
document.getElementById("screen-hud").addEventListener("mousedown", (event) => {
  if (event.button === 0 && !event.target.closest(".hud-anchor")) hudApi.trigger(true);
});
window.addEventListener("mouseup", () => hudApi.trigger(false));
window.addEventListener("blur", () => hudApi.trigger(false));
window.addEventListener("resize", fitHud);

hudApi.setType(hudUi.hud);
markHudToolbar();
fitHud();
new ResizeObserver(fitHud).observe(document.getElementById("hud-weapons"));
