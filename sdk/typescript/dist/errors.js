export class HarnessError extends Error {
    code;
    constructor(code, message) {
        super(message);
        this.name = "HarnessError";
        this.code = code;
    }
}
export function asError(error) {
    return error instanceof Error ? error : new Error(String(error));
}
