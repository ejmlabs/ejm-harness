// Private normalized identity boundary for API fixtures. Never used in deployment.
export default {
    fetch(request) {
        if (request.method !== "GET" || new URL(request.url).pathname !== "/internal/session") {
            return Response.json({ error: { code: "request_not_allowed" } }, { status: 403 });
        }
        const subjects = new Map([
            ["Bearer alice-token", { id: "alice", display_name: "Alice", is_operator: false }],
            ["Bearer bob-token", { id: "bob", display_name: "Bob", is_operator: false }],
            [
                "Bearer operator-token",
                { id: "operator", display_name: "Operator", is_operator: true },
            ],
        ]);
        const identity = subjects.get(request.headers.get("authorization"));
        return identity
            ? Response.json(identity)
            : Response.json({ error: { code: "sign_in_required" } }, { status: 401 });
    },
};
