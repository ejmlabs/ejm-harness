import { api, status } from "/common.js";

let cursor = null;
let query = "";
let busy = false;
let generation = 0;
const packages = document.getElementById("packages");
const more = document.getElementById("more");

async function load(reset) {
    if (busy && !reset) return;
    const currentGeneration = reset ? ++generation : generation;
    busy = true;
    more.disabled = true;
    try {
        const parameters = new URLSearchParams({ q: query, limit: "20" });
        if (!reset && cursor) parameters.set("cursor", cursor);
        const result = await api(`/v1/packages?${parameters}`);
        if (currentGeneration !== generation) return;
        if (reset) packages.replaceChildren();
        for (const item of result.packages) {
            const row = document.createElement("article");
            row.className = "package";
            const details = document.createElement("div");
            const title = document.createElement("h2");
            title.textContent = item.name;
            const description = document.createElement("p");
            description.textContent = item.description;
            const version = document.createElement("span");
            version.className = "version";
            version.textContent = `Last published: ${item.latest_version}`;
            const install = document.createElement("code");
            install.textContent = `${item.name}@${item.latest_version}`;
            details.append(title, description, version);
            row.append(details, install);
            packages.append(row);
        }
        cursor = result.next_cursor;
        more.hidden = !cursor;
        status(
            packages.children.length
                ? ""
                : "No extensions found. Try another name or publish the first one.",
        );
    } catch (error) {
        if (currentGeneration === generation) status(error.message, true);
    } finally {
        if (currentGeneration === generation) {
            busy = false;
            more.disabled = false;
        }
    }
}

document.getElementById("search").addEventListener("submit", (event) => {
    event.preventDefault();
    query = document.getElementById("query").value.trim();
    void load(true);
});
more.addEventListener("click", () => void load(false));
void load(true);
