// A13 (audit 2026-08-13) — anti-flash de thème, en fichier externe et non plus
// en script inline.
//
// Ce code vivait dans `app.html`. Il devait y rester inline pour s'exécuter
// avant le premier paint, mais la CSP en mode `hash` de SvelteKit ne hashe que
// SES propres scripts : celui-ci ne figurait dans aucune liste et se faisait
// donc refuser par l'intersection des deux politiques (en-tête Tauri × meta
// SvelteKit). Symptôme : violation CSP dans la console, thème sombre qui
// repart en clair, flash blanc au démarrage.
//
// Un hash écrit à la main dans `svelte.config.js` aurait marché puis pourri au
// premier espace changé. Un fichier servi par l'origine passe par `'self'`, qui
// est déjà dans les deux politiques, et ne se périme jamais. La balise reste
// synchrone dans le `<head>` : le navigateur la joue avant le premier paint,
// exactement comme l'inline.
(function () {
  try {
    var raw =
      localStorage.getItem("quanta.prefs.v1") || localStorage.getItem("titan.prefs.v1");
    var theme = raw ? JSON.parse(raw).theme || "light" : "light";
    if (theme === "auto") {
      theme =
        window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches
          ? "dark"
          : "light";
    }
    if (theme !== "dark" && theme !== "light") theme = "light";
    var root = document.documentElement;
    root.setAttribute("data-theme", theme);
    root.style.backgroundColor = theme === "dark" ? "#0f1115" : "#ffffff";
  } catch (e) {}
})();
