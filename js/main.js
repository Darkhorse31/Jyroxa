(function () {
  var STORAGE = "jyroxa-site-theme";

  function storedTheme() {
    try {
      var v = localStorage.getItem(STORAGE);
      if (v === "light" || v === "dark") return v;
    } catch (_) {}
    return null;
  }

  function currentTheme() {
    var t = document.documentElement.getAttribute("data-theme");
    if (t === "light" || t === "dark") return t;
    return storedTheme() || "dark";
  }

  function syncToggleButton() {
    var btn = document.querySelector("[data-theme-toggle]");
    if (!btn) return;
    var t = currentTheme();
    var next = t === "light" ? "dark" : "light";
    btn.setAttribute("aria-pressed", t === "light" ? "true" : "false");
    btn.textContent = next.charAt(0).toUpperCase() + next.slice(1);
    btn.setAttribute("aria-label", "Switch to " + next + " theme");
  }

  function applyTheme(theme, persist) {
    document.documentElement.setAttribute("data-theme", theme);
    if (persist) {
      try {
        localStorage.setItem(STORAGE, theme);
      } catch (_) {}
    }
    syncToggleButton();
  }

  var toggle = document.querySelector("[data-theme-toggle]");
  if (toggle) {
    toggle.addEventListener("click", function () {
      var next = currentTheme() === "light" ? "dark" : "light";
      applyTheme(next, true);
    });
  }

  syncToggleButton();

  var navToggle = document.querySelector("[data-nav-toggle]");
  var nav = document.querySelector("[data-site-nav]");
  if (navToggle && nav) {
    navToggle.addEventListener("click", function () {
      var open = nav.classList.toggle("is-open");
      navToggle.setAttribute("aria-expanded", open ? "true" : "false");
    });
  }
})();