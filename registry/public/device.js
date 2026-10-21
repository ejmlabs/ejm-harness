import { api, post, status } from "/common.js";

let verified = null;
let verificationGeneration = 0;
const form = document.getElementById("verify");
const request = document.getElementById("request");
const approve = document.getElementById("approve");
const deny = document.getElementById("deny");
const confirm = document.getElementById("confirm");
const input = document.getElementById("user-code");

function reset() {
    verificationGeneration += 1;
    verified = null;
    approve.disabled = true;
    deny.disabled = true;
    confirm.checked = false;
    request.hidden = true;
}

async function verify() {
    reset();
    const generation = verificationGeneration;
    const code = input.value.trim();
    if (!/^[A-Za-z0-9-]{6,32}$/.test(code)) {
        status("Enter the code shown in your terminal.", true);
        return;
    }
    try {
        const result = await api(`/api/auth/device?${new URLSearchParams({ user_code: code })}`);
        if (generation !== verificationGeneration) return;
        if (
            result.client_id !== "ejm-cli" ||
            typeof result.user_code !== "string" ||
            result.status !== "pending" ||
            (result.scope !== undefined && result.scope !== null && result.scope !== "registry")
        ) {
            throw new Error(
                "This request could not be verified. Start a new sign-in request in your client.",
            );
        }
        verified = Object.freeze({ user_code: result.user_code });
        document.getElementById("client").textContent = "ejm-cli";
        document.getElementById("code").textContent = result.user_code;
        document.getElementById("scope").textContent = result.scope || "Registry session";
        request.hidden = false;
        deny.disabled = false;
        status("Check the code and application before you approve.");
    } catch (error) {
        if (generation === verificationGeneration) status(error.message, true);
    }
}

form.addEventListener("submit", (event) => {
    event.preventDefault();
    void verify();
});
input.addEventListener("input", reset);
confirm.addEventListener("change", () => {
    approve.disabled = !verified || !confirm.checked;
});

async function decide(action) {
    if (!verified || (action === "approve" && !confirm.checked)) return;
    const code = verified.user_code;
    approve.disabled = true;
    deny.disabled = true;
    try {
        await post(`/api/auth/device/${action}`, { userCode: code });
        reset();
        form.hidden = true;
        status(
            action === "approve" ? "Device approved. Return to your terminal." : "Request denied.",
        );
    } catch (error) {
        reset();
        status(error.message, true);
    }
}
approve.addEventListener("click", () => void decide("approve"));
deny.addEventListener("click", () => void decide("deny"));

async function load() {
    try {
        await api("/v1/me");
        form.hidden = false;
        status("Enter the code shown in your terminal.");
        input.value = new URLSearchParams(location.search).get("user_code") || "";
        if (input.value) await verify();
    } catch (error) {
        status(error.message, error.status !== 401);
        document.getElementById("sign-in").hidden = error.status !== 401;
    }
}
void load();
