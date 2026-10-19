/* ejmlabs motion. Load it in <head> as a plain, synchronous script: no defer, no async, no module.
     <script src="/design-kit/web/motion.js"></script>
   The first lines add html.ejm-js before first paint, so the reveal styles in site.css apply only when this
   script runs. Without it (or without IntersectionObserver) every page is fully visible.
   It reveals .ejm-reveal and .ejm-stagger once by adding .is-visible, starts .ejm-typewriter, and counts up .ejm-count.
   It reads no layout and listens to no scroll events. Strict CSP safe: no eval, no inline handlers. */
(function () {
  var w = window, d = document, root = d.documentElement;
  if (!('IntersectionObserver' in w) || !w.matchMedia) return;
  root.classList.add('ejm-js');

  var SEL = '.ejm-reveal,.ejm-stagger,.ejm-typewriter,.ejm-count';
  var still = w.matchMedia('(prefers-reduced-motion: reduce)');
  var io;

  // Count-up: split "1,240+" into prefix, number and suffix, and keep the full text for screen readers.
  function fmt(c, n) {
    return c.pre + n.toLocaleString('en-US', { minimumFractionDigits: c.dec, maximumFractionDigits: c.dec, useGrouping: c.sep }) + c.suf;
  }
  function prep(el) {
    var m = el.textContent.match(/^(\D*)(\d[\d,]*\.?\d*)([\s\S]*)$/);
    if (!m) return;
    var read = d.createElement('span'), seen = d.createElement('span');
    el._c = { pre: m[1], suf: m[3], to: parseFloat(m[2].replace(/,/g, '')), dec: (m[2].split('.')[1] || '').length, sep: m[2].indexOf(',') > -1 };
    read.className = 'ejm-sr';
    read.textContent = el.textContent;
    seen.setAttribute('aria-hidden', 'true');
    seen.textContent = fmt(el._c, 0);
    el.textContent = '';
    el.append(read, seen);
    el._v = seen;
  }
  function settle(el) {
    cancelAnimationFrame(el._r);
    el._v.textContent = fmt(el._c, el._c.to);
  }
  function count(el) {
    var t0, ms = parseFloat(getComputedStyle(root).getPropertyValue('--ejm-duration-slower')) || 720;
    cancelAnimationFrame(el._r);
    el._r = requestAnimationFrame(function tick(t) {
      t0 = t0 || t;
      var p = Math.min((t - t0) / ms, 1);
      el._v.textContent = fmt(el._c, el._c.to * (1 - Math.pow(1 - p, 3)));
      if (p < 1) el._r = requestAnimationFrame(tick);
    });
  }

  function show(el) {
    el.classList.add('is-visible');
    if (el._c) still.matches ? settle(el) : count(el);
  }

  // Call after adding content later: ejmMotion.refresh(container).
  function refresh(scope) {
    if (!io) return;
    (scope || d).querySelectorAll(SEL).forEach(function (el) {
      if (el._seen) return;
      el._seen = 1;
      if (el.classList.contains('ejm-typewriter')) el.style.setProperty('--ejm-chars', el.textContent.trim().length);
      if (el.classList.contains('ejm-count') && !still.matches) prep(el);
      still.matches ? show(el) : io.observe(el);
    });
  }

  // Play the reveals, entrances, typing and counts in a scope again (for specimens and demos).
  function replay(scope) {
    var s = scope || d;
    s.querySelectorAll(SEL).forEach(function (el) {
      el.classList.remove('is-visible');
      if (el._c) el._v.textContent = fmt(el._c, 0);
      if (still.matches) show(el); else io.observe(el);
    });
    // Entrances are plain CSS: take the class off, force one style flush, and put it back.
    var enters = [].slice.call(s.querySelectorAll('.ejm-enter'));
    if (s.matches && s.matches('.ejm-enter')) enters.push(s);
    enters.forEach(function (el) { el.classList.remove('ejm-enter'); });
    void root.offsetWidth;
    enters.forEach(function (el) { el.classList.add('ejm-enter'); });
  }

  w.ejmMotion = { refresh: refresh, replay: replay };

  function start() {
    try {
      io = new IntersectionObserver(function (entries) {
        entries.forEach(function (e) {
          if (e.isIntersecting) { io.unobserve(e.target); show(e.target); }
        });
      }, { threshold: 0.05 });
      refresh();
      // Turning reduced motion on while the page is open shows everything at once.
      still.addEventListener('change', function () {
        if (still.matches) d.querySelectorAll(SEL).forEach(function (el) { io.unobserve(el); show(el); });
      });
      d.addEventListener('click', function (e) {
        var b = e.target.closest && e.target.closest('[data-ejm-replay]');
        if (b) replay(d.querySelector(b.getAttribute('data-ejm-replay')) || d);
      });
    } catch (err) {
      root.classList.remove('ejm-js'); // never leave content hidden
    }
  }
  d.readyState === 'loading' ? d.addEventListener('DOMContentLoaded', start) : start();
})();
