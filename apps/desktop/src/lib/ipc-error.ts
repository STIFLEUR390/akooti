/**
 * Typed IPC failure, in its own module so the mock can throw it without
 * importing `ipc.ts` (which builds the API, and so imports the mock).
 */
export class IpcError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "IpcError";
    this.code = code;
  }

  /** True when the vault locked itself mid-action. */
  get isLocked(): boolean {
    return this.code === "locked";
  }
}
