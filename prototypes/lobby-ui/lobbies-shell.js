function markLobbyToolbar() {
  document.querySelectorAll(".bar-group").forEach((group) => {
    group.querySelectorAll("button").forEach((button) => {
      button.classList.toggle("is-on", lobbyUi[group.dataset.group] === button.dataset.value);
    });
  });
}

function fitLobbies() {
  const [width, height] = lobbyUi.size.split("x").map(Number);
  const box = document.getElementById("viewport").getBoundingClientRect();
  const scale = Math.min(box.width / width, box.height / height, 1);
  Object.assign(document.getElementById("stage").style, {
    width: `${width}px`,
    height: `${height}px`,
    transform: `scale(${scale})`,
    left: `${(box.width - width * scale) / 2}px`,
    top: `${(box.height - height * scale) / 2}px`,
  });
  const panel = document.getElementById("menu-panel");
  const fits = panel.offsetWidth <= width - 16 && panel.offsetHeight <= height - 16;
  const label = document.getElementById("fit");
  label.textContent = `${fits ? "Fits" : "Too big for"} ${width}×${height} · menu panel ${panel.offsetWidth}×${panel.offsetHeight}`;
  label.classList.toggle("is-bad", !fits);
}

function onLobbyToolbar(event) {
  const button = event.target.closest(".bar-group button");
  if (!button) return;
  const key = button.closest(".bar-group").dataset.group;
  lobbyUi[key] = button.dataset.value;
  lobbyUi.status = "";
  lobbyUi.browse = false;
  lobbyUi.detail = null;
  renderLobbies();
  markLobbyToolbar();
  fitLobbies();
}

function tickLive() {
  if (!SIDE_TYPES[lobbyUi.type] || lobbyUi.browse || lobbyUi.detail !== null) return;
  if (performance.now() - lobbyUi.updated < 5000) return updateAgo();
  refreshLobbies();
  return renderSide();
}

document.querySelector(".bar").addEventListener("click", onLobbyToolbar);
document.getElementById("screen-menu").addEventListener("click", onLobbyClick);
document.addEventListener("keydown", (event) => {
  if (event.key !== "Escape" || (!lobbyUi.browse && lobbyUi.detail === null)) return;
  LOBBY_ACTIONS.close();
  renderLobbies();
});
window.addEventListener("resize", fitLobbies);
setInterval(tickLive, 1000);

renderLobbies();
markLobbyToolbar();
fitLobbies();
new ResizeObserver(fitLobbies).observe(document.getElementById("menu-panel"));
