import { api, post, status } from "/common.js";

async function load() {
    try {
        const result = await api("/v1/me");
        document.getElementById("account").hidden = false;
        document.getElementById("identity").textContent =
            `Signed in as ${result.user.display_name}.`;
        if (result.suspended || result.publisher?.suspended) {
            status("Publishing is suspended for this account.", true);
            return;
        }
        status("");
        if (result.publisher) {
            document.getElementById("publisher").textContent =
                `Publisher: @${result.publisher.handle}`;
            document.getElementById("publish-help").hidden = false;
        } else {
            document.getElementById("claim").hidden = false;
        }
    } catch (error) {
        status(error.message, error.status !== 401);
        document.getElementById("sign-in").hidden = error.status !== 401;
    }
}

document.getElementById("claim").addEventListener("submit", async (event) => {
    event.preventDefault();
    const button = event.currentTarget.querySelector("button");
    button.disabled = true;
    try {
        await post("/v1/publishers", {
            handle: document.getElementById("handle").value,
            display_name: document.getElementById("display-name").value.trim(),
        });
        document.getElementById("claim").hidden = true;
        await load();
    } catch (error) {
        status(error.message, true);
    } finally {
        button.disabled = false;
    }
});
document.getElementById("sign-out").addEventListener("click", async () => {
    try {
        await post("/api/auth/sign-out", {});
        location.reload();
    } catch (error) {
        status(error.message, true);
    }
});
document.getElementById("revoke-devices").addEventListener("click", async () => {
    try {
        await post("/api/auth/revoke-other-sessions", {});
        status("Other devices have been signed out.");
    } catch (error) {
        status(error.message, true);
    }
});
void load();
