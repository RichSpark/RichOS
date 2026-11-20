// The RichFiles rail item opens this information page or the standalone Mac app.
(() => {
  "use strict";

  const bridge = window.RichBridge;
  const rail = window.RichNavRail;
  const $ = (id) => document.getElementById(id);
  const page = $("richfiles-page");
  if (!bridge || !rail || !page) return;

  const preference = "richos.richfiles.openStandalone";
  const changes = [...page.querySelectorAll("[data-rf-setting]")];
  let available = false;
  let changing = false;
  let current = null;

  function opensStandalone() {
    return available && localStorage.getItem(preference) === "true";
  }

  function paintChoice() {
    const on = opensStandalone();
    $("rf-always").setAttribute("aria-checked", String(on));
    $("set-richfiles-always").checked = on;
    $("rf-always-hint").textContent = on
      ? "On. From now on, the RichFiles icon opens RichFiles in its own window. You will not see this page again. To get it back, turn this off in Settings."
      : "Turn this on and the RichFiles icon on the left stops showing this page. From then on, every click on it opens RichFiles right away, as its own app.";
    $("set-richfiles-hint").textContent = on
      ? "On: the RichFiles icon opens RichFiles as its own app."
      : "Off: the RichFiles icon opens the RichFiles page.";
  }

  function setChoice(on) {
    localStorage.setItem(preference, String(on));
    paintChoice();
  }

  function open() {
    return bridge.invoke("richfiles_open", {}).catch((error) => window.RichSettings?.toast(String(error)));
  }

  function paintSettings() {
    for (const button of changes) {
      button.disabled = changing || !current;
      button.setAttribute("aria-checked", String(!!current?.[button.dataset.rfSetting]));
    }
  }

  async function settings(option, enabled) {
    if (changing) return;
    changing = true;
    paintSettings();
    const status = $("rf-settings-status");
    status.hidden = true;
    try {
      current = await bridge.invoke("richfiles_settings", { option, enabled });
      if (current.message) {
        status.textContent = current.message;
        status.hidden = false;
      }
    } catch (error) {
      status.textContent = String(error);
      status.hidden = false;
    } finally {
      changing = false;
      paintSettings();
    }
  }

  function show() {
    if (!window.RichDismiss?.question?.()) window.RichHome?.hide?.("richfiles");
    page.hidden = false;
    document.body.classList.add("page-open");
    const crumb = $("page-crumb");
    crumb.textContent = "RichFiles";
    crumb.hidden = false;
    $("richfiles-title").focus({ preventScroll: true });
    settings(null, null);
  }

  function hide() {
    page.hidden = true;
    document.body.classList.remove("page-open");
    $("page-crumb").hidden = true;
  }

  $("rf-open").addEventListener("click", open);
  $("rf-always").addEventListener("click", () => setChoice(!opensStandalone()));
  $("set-richfiles-always").addEventListener("change", (event) => setChoice(event.target.checked));
  for (const button of changes) {
    button.addEventListener("click", () => settings(button.dataset.rfSetting, !current?.[button.dataset.rfSetting]));
  }
  window.addEventListener("focus", () => { if (!page.hidden) settings(null, null); });
  window.RichFilesPage = { open, opensStandalone };
  bridge.invoke("richfiles_available", {}).then((installed) => {
    if (!installed) return;
    available = true;
    paintChoice();
    rail.registerPage("richfiles", { show, hide });
    $("nr-richfiles").hidden = false;
    $("set-richfiles").hidden = false;
  }).catch(() => {});
})();
