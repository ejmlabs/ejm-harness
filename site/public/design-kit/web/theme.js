// ejmlabs theme: one light/dark choice for every ejmlabs site. See web/THEME.md.
// Load it synchronously in <head>, after the theme-color meta tag, so the page never flashes the wrong theme:
//   <script src="/design-kit/web/theme.js"></script>
// No dependencies. No eval, no inline handlers, so it passes a strict Content-Security-Policy.
(function () {
  'use strict';

  var NAME = 'ejm-theme';
  var DARK_META = '#131614'; // the dark page ground, used for the browser toolbar color
  var root = document.documentElement;
  var host = location.hostname;
  var shared = host === 'ejmlabs.com' || /\.ejmlabs\.com$/.test(host);
  var system = window.matchMedia ? matchMedia('(prefers-color-scheme: dark)') : null;
  var still = window.matchMedia ? matchMedia('(prefers-reduced-motion: reduce)') : null;

  function valid(value) { return value === 'light' || value === 'dark' ? value : null; }

  // The saved choice. A cookie on ejmlabs.com and its subdomains, so every site shares it; localStorage elsewhere.
  function read() {
    try {
      if (shared) {
        var found = document.cookie.match(/(?:^|;\s*)ejm-theme=(light|dark)(?:;|$)/);
        return found ? found[1] : null;
      }
      return valid(localStorage.getItem(NAME));
    } catch (e) { return null; }
  }

  function save(theme) {
    try {
      if (shared) {
        document.cookie = NAME + '=' + theme + '; Domain=.ejmlabs.com; Path=/; Max-Age=31536000; SameSite=Lax' +
          (location.protocol === 'https:' ? '; Secure' : '');
      } else {
        localStorage.setItem(NAME, theme);
      }
    } catch (e) { /* storage blocked: the choice lasts for this page only */ }
  }

  // ?theme=dark or ?theme=light wins over everything and is never saved. For tests and screenshots.
  var forced = null;
  try { forced = valid(new URLSearchParams(location.search).get('theme')); } catch (e) { /* old browser */ }

  var current = forced || read() || (system && system.matches ? 'dark' : 'light');

  // Put the theme on the page: the attribute, the toolbar color and every toggle. Announce it only when it changed.
  function paint(theme, announce) {
    root.dataset.theme = theme;
    var meta = document.querySelector('meta[name="theme-color"]');
    if (meta) {
      // The authored color is the light color. Add data-dark to the meta tag when the top of your page keeps the same color in both themes.
      if (!meta.hasAttribute('data-light')) meta.setAttribute('data-light', meta.content);
      meta.content = theme === 'dark' ? (meta.getAttribute('data-dark') || DARK_META) : meta.getAttribute('data-light');
    }
    syncButtons(document);
    if (announce) document.dispatchEvent(new CustomEvent('ejm-themechange', { detail: { theme: theme } }));
  }

  function syncButtons(scope) {
    var buttons = scope.querySelectorAll ? scope.querySelectorAll('[data-theme-toggle]') : [];
    for (var i = 0; i < buttons.length; i++) buttons[i].setAttribute('aria-pressed', String(current === 'dark'));
    if (scope.matches && scope.matches('[data-theme-toggle]')) scope.setAttribute('aria-pressed', String(current === 'dark'));
  }

  // Change the theme and remember it. The page crossfades when the browser can and the visitor allows motion.
  function set(theme) {
    theme = valid(theme);
    if (!theme) return current;
    save(theme);
    if (theme === current && root.dataset.theme === theme) return current;
    current = theme;
    var apply = function () { paint(theme, true); };
    if (document.startViewTransition && document.body && !(still && still.matches)) document.startViewTransition(apply);
    else apply();
    return current;
  }

  function toggle() { return set(current === 'dark' ? 'light' : 'dark'); }

  // First paint: set the attribute now, before the body exists. The meta tag and buttons are synced again when the page is parsed.
  root.dataset.theme = current;
  document.addEventListener('DOMContentLoaded', function () { paint(current, false); });

  // Follow the system while the visitor has not chosen.
  if (system) {
    var follow = function () {
      if (forced || read()) return;
      current = system.matches ? 'dark' : 'light';
      paint(current, true);
    };
    if (system.addEventListener) system.addEventListener('change', follow); else if (system.addListener) system.addListener(follow);
  }

  // One listener serves every toggle, including buttons added later. [data-theme-set="dark"] sets a theme directly.
  document.addEventListener('click', function (event) {
    var target = event.target;
    if (!target || !target.closest) return;
    if (target.closest('[data-theme-toggle]')) toggle();
    var direct = target.closest('[data-theme-set]');
    if (direct) set(direct.getAttribute('data-theme-set'));
  });
  if (window.MutationObserver) {
    new MutationObserver(function (records) {
      for (var i = 0; i < records.length; i++)
        for (var j = 0; j < records[i].addedNodes.length; j++)
          if (records[i].addedNodes[j].nodeType === 1) syncButtons(records[i].addedNodes[j]);
    }).observe(root, { childList: true, subtree: true });
  }

  window.ejmTheme = {
    get: function () { return current; },
    set: set,
    toggle: toggle
  };
})();
