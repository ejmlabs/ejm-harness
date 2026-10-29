/** Build the wire result returned by a host tool or package tool export. */
export function toolOutput(content, options = {}) {
    return {
        content,
        data: options.data ?? null,
        is_error: options.isError ?? false,
        artifact: options.artifact ?? null,
    };
}
/** Registering a tool grants the host's declared effect. Repository files grant nothing. */
export function defineTool(options) {
    return {
        definition: {
            name: options.name,
            description: options.description,
            input_schema: options.inputSchema,
            effect: options.effect ?? "read",
            version: options.version ?? "1",
        },
        execute: options.execute,
    };
}
