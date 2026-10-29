export declare class HarnessError extends Error {
    readonly code: string;
    constructor(code: string, message: string);
}
export declare function asError(error: unknown): Error;
