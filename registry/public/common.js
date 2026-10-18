export async function api(path, options = {}) {
    const response = await fetch(path, {
        credentials: "same-origin",
        redirect: "error",
        ...options,
    });
    const text = await response.text();
    if (text.length > 64 * 1024)
        throw new Error("The server returned too much data. Try again later.");
    let data;
    try {
        data = JSON.parse(text);
    } catch {
        throw new Error("The registry could not complete this request. Try again later.");
    }
    if (!response.ok) {
        const error = new Error(
            response.status === 401
                ? "Sign in to continue."
                : "The request failed. Check your details and try again.",
        );
        error.status = response.status;
        throw error;
    }
    return data;
}

export function post(path, data) {
    return api(path, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(data),
    });
}

export function status(message, error = false) {
    const element = document.getElementById("status");
    element.textContent = message;
    element.toggleAttribute("data-error", error);
}

export async function signIn() {
    const callbackURL = `${location.origin}${location.pathname}${location.search}`;
    const result = await post("/api/auth/sign-in/social", {
        provider: "github",
        callbackURL,
        disableRedirect: true,
    });
    const url = new URL(result.url);
    if (
        url.protocol !== "https:" ||
        url.origin !== "https://github.com" ||
        url.pathname !== "/login/oauth/authorize"
    ) {
        throw new Error("The sign-in address could not be verified.");
    }
    location.assign(url.href);
}

document.getElementById("sign-in")?.addEventListener("click", async () => {
    try {
        await signIn();
    } catch (error) {
        status(error.message, true);
    }
});
