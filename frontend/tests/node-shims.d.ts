/** Minimal Node test module declarations for the local TypeScript test harness. */
declare module "node:test" {
  const test: (name: string, fn: () => void | Promise<void>) => void;
  export default test;
}

/** Vite's optional build-time environment shape used by the API client in Node tests. */
interface ImportMeta {
  env?: {
    VITE_API_BASE?: string;
  };
}

/** Minimal strict-assert declarations used by the local TypeScript test harness. */
declare module "node:assert/strict" {
  /** Asserts that two values are structurally equal. */
  export function deepEqual(actual: unknown, expected: unknown, message?: string): void;
  /** Asserts that two values are strictly equal. */
  export function equal(actual: unknown, expected: unknown, message?: string): void;
  /** Asserts that value is truthy. */
  export function ok(value: unknown, message?: string): void;

  const assert: {
    deepEqual: typeof deepEqual;
    equal: typeof equal;
    ok: typeof ok;
  };

  export default assert;
}

/** Minimal filesystem declaration used by source-surface contract tests. */
declare module "node:fs" {
  /** Reads a UTF-8 text file synchronously. */
  export function readFileSync(path: URL, encoding: "utf8"): string;
}
