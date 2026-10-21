// Reads the built pages in site/dist without running a script, and checks what a visitor and the rules of the site depend on.
// Run `npm run build` first (`npm run check` does). Node's own test runner and parse5 are all it needs.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { parse } from "parse5";

const site = join(dirname(fileURLToPath(import.meta.url)), "..");
const repo = join(site, "..");
const dist = join(site, "dist");

const ROUTES = ["/", "/docs/", "/docs/user-guide/", "/docs/developer-guide/", "/docs/host-auth/"];
const PAGES = [...ROUTES, "/404.html"];
const HOST = "harness.ejmlabs.com";

// Heading anchors that other pages, the README and readers link to. They must keep working.
const ANCHORS = {
  "/docs/": ["choose", "availability", "first-run", "boundaries", "references"],
  "/docs/host-auth/": [
    "status",
    "setup",
    "vault",
    "migration",
    "claude",
    "provider",
    "completion",
    "verification",
  ],
  "/docs/user-guide/": [
    "requirements",
    "build",
    "first-run",
    "providers",
    "credentials",
    "grants",
    "runs",
    "history",
    "recovery",
    "packages",
    "troubleshooting",
  ],
  "/docs/developer-guide/": [
    "setup",
    "map",
    "rust",
    "ownership",
    "recovery",
    "protocol",
    "extensions",
    "checks",
    "contribute",
    "website",
  ],
};

// ---- helpers ----
const file = (route) => join(dist, route.endsWith(".html") ? route : `${route}index.html`);
const cache = new Map();
const dom = (route) => {
  if (!cache.has(route)) {
    assert.ok(existsSync(file(route)), `${route} is not built: run npm run build`);
    cache.set(route, parse(readFileSync(file(route), "utf8")));
  }
  return cache.get(route);
};
const attr = (n, k) => n.attrs?.find((a) => a.name === k)?.value;
const all = (n, pred = () => true, out = []) => {
  if (n.nodeName && n.nodeName[0] !== "#" && pred(n)) out.push(n);
  for (const c of n.childNodes ?? []) all(c, pred, out);
  if (n.content) all(n.content, pred, out);
  return out;
};
const first = (n, pred) => all(n, pred)[0];
const tag = (name) => (n) => n.nodeName === name;
const raw = (n) => (n.nodeName === "#text" ? n.value : (n.childNodes ?? []).map(raw).join(""));
/** Visible prose: text outside code, scripts and styles. */
const prose = (n) => {
  if (n.nodeName === "#text") return n.value;
  if (["script", "style", "pre", "code", "template"].includes(n.nodeName)) return " ";
  return (n.childNodes ?? []).map(prose).join(" ");
};
const body = (route) => first(dom(route), tag("body"));
const header = (route) => first(dom(route), tag("header"));

// ---- structure ----
for (const route of PAGES) {
  test(`${route}: landmarks, one h1, skip link first, toggle last`, () => {
    const d = dom(route);
    assert.equal(all(d, tag("h1")).length, 1, "exactly one h1");
    const main = first(d, (n) => n.nodeName === "main" && attr(n, "id") === "main");
    assert.ok(main, "main#main");
    const focusable = all(
      body(route),
      (n) =>
        (n.nodeName === "a" && attr(n, "href") !== undefined) ||
        n.nodeName === "button" ||
        n.nodeName === "summary",
    );
    const skip = focusable[0];
    assert.equal(skip?.nodeName, "a");
    assert.equal(attr(skip, "href"), "#main");
    assert.match(attr(skip, "class") ?? "", /skip-link/);
    const inHeader = all(
      header(route),
      (n) =>
        (n.nodeName === "a" && attr(n, "href") !== undefined) ||
        n.nodeName === "button" ||
        n.nodeName === "summary",
    );
    const last = inHeader.at(-1);
    assert.equal(last?.nodeName, "button");
    assert.match(
      attr(last, "aria-label") ?? "",
      /theme/i,
      "the theme toggle is the last control in the header",
    );
    const home = inHeader[0];
    assert.equal(attr(home, "href"), "/");
    assert.match(attr(home, "aria-label") ?? "", /ejm-harness home/);
  });

  test(`${route}: heading levels do not skip`, () => {
    const levels = all(body(route), (n) => /^h[1-6]$/.test(n.nodeName)).map((n) =>
      Number(n.nodeName[1]),
    );
    let max = 0;
    for (const l of levels) {
      assert.ok(l <= max + 1, `h${l} follows h${max}`);
      max = Math.max(max, l);
    }
  });

  test(`${route}: no inline script, images have alt text`, () => {
    for (const s of all(dom(route), tag("script")))
      assert.ok(attr(s, "src"), "every script is a file, so script-src stays self");
    for (const i of all(body(route), tag("img"))) assert.notEqual(attr(i, "alt"), undefined);
  });
}

// ---- head ----
for (const route of PAGES) {
  test(`${route}: title, description, canonical and sharing tags`, () => {
    const d = dom(route);
    const title = raw(first(d, tag("title"))).trim();
    assert.ok(title.length > 3);
    assert.ok(
      title.endsWith("| ejm-harness") || route === "/404.html",
      `title names the site: ${title}`,
    );
    const meta = (k, v) =>
      attr(first(d, (n) => n.nodeName === "meta" && attr(n, k) === v) ?? { attrs: [] }, "content");
    if (route === "/404.html") {
      assert.match(meta("name", "robots") ?? "", /noindex/);
      return;
    }
    const desc = meta("name", "description");
    assert.ok(desc && desc.length > 40 && desc.length < 300, "description");
    const canonical = attr(
      first(d, (n) => n.nodeName === "link" && attr(n, "rel") === "canonical"),
      "href",
    );
    assert.equal(canonical, `https://${HOST}${route}`);
    assert.equal(meta("property", "og:url"), canonical);
    assert.equal(meta("property", "og:image"), `https://${HOST}/social-card.png`);
    assert.ok(
      attr(
        first(d, (n) => n.nodeName === "script" && /theme-boot\.js$/.test(attr(n, "src") ?? "")),
        "src",
      ),
      "theme boot script",
    );
  });
}

test("titles and descriptions are different on every page", () => {
  const titles = ROUTES.map((r) => raw(first(dom(r), tag("title"))));
  assert.equal(new Set(titles).size, ROUTES.length);
});

// ---- links ----
const idsOf = (route) =>
  new Set(all(dom(route), (n) => attr(n, "id") !== undefined).map((n) => attr(n, "id")));

test("external links are marked, internal links resolve, fragments exist", () => {
  for (const route of PAGES) {
    const footer = first(dom(route), tag("footer"));
    assert.ok(
      first(
        footer,
        (n) => n.nodeName === "a" && attr(n, "href") === "https://ejmlabs.com/privacy/",
      ),
      `${route}: footer links website privacy`,
    );
    for (const a of all(body(route), (n) => n.nodeName === "a" && attr(n, "href") !== undefined)) {
      const href = attr(a, "href");
      if (/^https?:\/\//.test(href) && new URL(href).host !== HOST) {
        assert.match(attr(a, "rel") ?? "", /\bexternal\b/, `${route}: ${href} needs rel=external`);
        continue;
      }
      if (/^(mailto|tel):/.test(href)) continue;
      const url = new URL(href, `https://${HOST}${route === "/404.html" ? "/" : route}`);
      const target = url.pathname;
      assert.ok(
        PAGES.includes(target) || existsSync(join(dist, target)),
        `${route}: ${href} goes nowhere`,
      );
      if (url.hash && PAGES.includes(target))
        assert.ok(
          idsOf(target).has(decodeURIComponent(url.hash.slice(1))),
          `${route}: ${href} has no target`,
        );
    }
  }
});

test("old heading anchors still exist", () => {
  for (const [route, ids] of Object.entries(ANCHORS))
    for (const id of ids) assert.ok(idsOf(route).has(id), `${route}#${id}`);
});

test("the other ejmlabs sites and the source repo are linked from the home page", () => {
  const hrefs = all(body("/"), tag("a")).map((a) => attr(a, "href"));
  for (const h of [
    "https://cli.ejmlabs.com/",
    "https://ide.ejmlabs.com/",
    "https://github.com/ejmlabs/ejm-harness",
    "https://ejmlabs.com/",
  ])
    assert.ok(hrefs.includes(h), h);
});

// ---- recorded workflow without scripts ----
test("the home page retains useful recorded output without editable terminal input", () => {
  const section = first(body("/"), (n) => attr(n, "id") === "demo");
  assert.ok(section, "home showcase section exists without scripts");
  const description = attr(section, "aria-describedby");
  assert.ok(description, "playback links its explanatory note");
  const copy = prose(first(body("/"), (n) => attr(n, "id") === description)).replace(/\s+/g, " ");
  assert.match(copy, /record(?:ed|ing)/i);
  assert.match(copy, /no (?:worker or )?model (?:calls )?runs?\b/i);
  const terminal = first(section, (n) => attr(n, "data-showcase") === "ejm-harness");
  assert.ok(terminal, "shared terminal output is present without scripts");
  const output = raw(terminal).replace(/\s+/g, " ");
  assert.match(output, /tool_finished/);
  assert.match(output, /governor while two workspaces request heavy work:/);
  assert.match(output, /reopened saved session: Harbor checks/);
  assert.match(output, /events\(after=0, limit=4\)/);
  assert.equal(
    all(
      section,
      (n) => ["input", "textarea"].includes(n.nodeName) || attr(n, "contenteditable") === "true",
    ).length,
    0,
  );
});

// ---- shared files ----
test("headers, manifest, icons and the social card come from the shared packages", () => {
  const headers = readFileSync(join(dist, "_headers"), "utf8");
  assert.match(headers, /script-src 'self' https:\/\/static\.cloudflareinsights\.com/);
  execFileSync(
    join(site, "node_modules/.bin/ejm-site-headers"),
    [join(dist, "_headers"), "--check"],
    { stdio: "pipe" },
  );
  const manifest = JSON.parse(readFileSync(join(dist, "site.webmanifest"), "utf8"));
  assert.match(manifest.name, /harness/);
  for (const f of [
    "favicon.ico",
    "favicon.svg",
    "apple-touch-icon.png",
    "android-chrome-512.png",
    "social-card.png",
    "theme-boot.js",
    "Manrope-OFL.txt",
    "IBMPlexMono-OFL.txt",
  ])
    assert.ok(existsSync(join(dist, f)), f);
  assert.ok(!existsSync(join(dist, "design-kit")), "the old design kit copy is gone");
});

test("the analytics beacon is not in the prerendered HTML", () => {
  for (const route of PAGES)
    assert.ok(!/cloudflareinsights/.test(readFileSync(file(route), "utf8")));
});

test("no README points at the removed design kit copy", () => {
  for (const f of ["README.md", "sdk/typescript/README.md", "sdk/python/README.md"]) {
    assert.ok(
      !/design-kit/.test(readFileSync(join(repo, f), "utf8")),
      `${f} still links the old design kit path`,
    );
  }
});

test("one shared playback is the primary hero preview and keeps its watch link", () => {
  const home = dom("/");
  const main = first(home, tag("main"));
  const hero = first(main, tag("section"));
  assert.ok(first(hero, tag("h1")), "the first home section is the hero");
  const anchor = first(hero, (n) => attr(n, "id") === "demo");
  assert.ok(anchor, "the existing playback anchor belongs to the hero");
  const players = all(home, (n) => attr(n, "data-showcase") !== undefined);
  assert.equal(players.length, 1, "there is one playback on the page");
  assert.equal(
    first(anchor, (n) => attr(n, "data-showcase") === "ejm-harness"),
    players[0],
  );
  assert.deepEqual(
    all(players[0], tag("button")).map((n) => attr(n, "aria-label")),
    ["Pause ejm-harness playback", "Restart ejm-harness playback"],
  );
  assert.ok(
    all(home, (n) => n.nodeName === "a" && attr(n, "href") === "/#demo").some(
      (n) => raw(n).trim() === "Watch",
    ),
  );
  assert.ok(
    first(hero, (n) => n.nodeName === "a" && attr(n, "href") === "/docs/user-guide/#requirements"),
  );
  assert.ok(first(hero, (n) => n.nodeName === "a" && attr(n, "href") === "#contribute"));
  assert.ok(
    first(
      hero,
      (n) => n.nodeName === "a" && attr(n, "href") === "https://github.com/ejmlabs/ejm-harness",
    ),
  );
  assert.equal(
    all(home, (n) => n.nodeName === "figure" && attr(n, "data-shot") !== undefined).length,
    0,
  );
  // Logo preloads remain valid; the removed responsive capture must not be preloaded.
  assert.equal(
    all(
      home,
      (n) =>
        n.nodeName === "link" && attr(n, "as") === "image" && attr(n, "imagesrcset") !== undefined,
    ).length,
    0,
  );
});
