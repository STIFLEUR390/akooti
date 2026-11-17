import { describe, expect, test } from "bun:test";

import { IpcError, resolveApi, toIpcError } from "./ipc";

describe("error envelope", () => {
  test("keeps the backend code and message", () => {
    const error = toIpcError({ code: "invalid_credentials", message: "Mot de passe incorrect." });
    expect(error).toBeInstanceOf(IpcError);
    expect(error.code).toBe("invalid_credentials");
    expect(error.message).toBe("Mot de passe incorrect.");
    expect(error.isLocked).toBe(false);
  });

  test("flags a locked session", () => {
    expect(toIpcError({ code: "locked", message: "Le coffre est verrouillé." }).isLocked).toBe(
      true,
    );
  });

  test("never leaks an unknown shape", () => {
    const error = toIpcError(new Error("boom"));
    expect(error.code).toBe("internal");
    expect(error.message).toBe("Une erreur est survenue.");
  });

  test("passes an IpcError through unchanged", () => {
    const original = new IpcError("not_found", "absent");
    expect(toIpcError(original)).toBe(original);
  });

  test("accepts a bare string", () => {
    expect(toIpcError("échec").message).toBe("échec");
  });
});

describe("runtime selection", () => {
  test("without a Tauri bridge the UI gets the mock", async () => {
    // bun has no `window`, so the real command surface is unreachable.
    const api = resolveApi();
    const status = await api.vaultStatus();
    expect(status.status).toBe("locked");
    expect(status.autoLockMinutes).toBe(15);
  });
});
