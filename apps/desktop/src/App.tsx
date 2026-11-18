import { useCallback, useEffect, useState } from "react";

import { Toasts } from "./components/Toasts";
import { useToasts } from "./hooks/useToasts";
import { api, toIpcError } from "./lib/ipc";
import type { BiometricsStatus, BuildInfo } from "./lib/ipc";
import { UnlockPage } from "./pages/UnlockPage";
import { VaultPage } from "./pages/VaultPage";

type Phase = "loading" | "locked" | "unlocked";

/**
 * Application root.
 *
 * Owns only what must survive a lock/unlock cycle: the theme is inside the
 * vault shell, the session state and the toasts live here. Everything secret
 * dies with {@link VaultPage}, so locking unmounts the whole unlocked tree.
 */
export function App() {
  const [phase, setPhase] = useState<Phase>("loading");
  const [hasVault, setHasVault] = useState(false);
  const [autoLockMinutes, setAutoLockMinutes] = useState(15);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [biometrics, setBiometrics] = useState<BiometricsStatus | null>(null);
  const [buildInfo, setBuildInfo] = useState<BuildInfo | null>(null);
  const { toasts, dismiss, success, failure } = useToasts();

  const refreshStatus = useCallback(async () => {
    try {
      const status = await api.vaultStatus();
      setHasVault(status.hasVault);
      setAutoLockMinutes(status.autoLockMinutes);
      setPhase(status.status === "unlocked" ? "unlocked" : "locked");
    } catch (caught) {
      setError(toIpcError(caught).message);
      setPhase("locked");
    }
  }, []);

  useEffect(() => {
    void refreshStatus();
    api
      .biometricsStatus()
      .then(setBiometrics)
      .catch(() => setBiometrics(null));
    api
      .buildInfo()
      .then(setBuildInfo)
      .catch(() => setBuildInfo(null));
  }, [refreshStatus]);

  const handleUnlock = useCallback(
    async (password: string) => {
      setBusy(true);
      setError(null);
      try {
        await api.unlockVault(password);
        setPhase("unlocked");
        setHasVault(true);
        success("Coffre déverrouillé.");
        return true;
      } catch (caught) {
        const ipc = toIpcError(caught);
        setError(ipc.message);
        return false;
      } finally {
        setBusy(false);
      }
    },
    [success],
  );

  const handleCreate = useCallback(
    async (password: string) => {
      setBusy(true);
      setError(null);
      try {
        await api.createVault(password);
        setPhase("unlocked");
        setHasVault(true);
        success("Coffre créé. Il est déverrouillé pour 15 minutes d'inactivité.");
        return true;
      } catch (caught) {
        const ipc = toIpcError(caught);
        setError(ipc.message);
        if (ipc.code === "vault_exists") {
          setHasVault(true);
        }
        return false;
      } finally {
        setBusy(false);
      }
    },
    [success],
  );

  const handleLock = useCallback(() => {
    void api.lockVault().catch(() => undefined);
    setPhase("locked");
    setError(null);
    success("Coffre verrouillé.");
    void refreshStatus();
  }, [refreshStatus, success]);

  const notices = { success, failure };

  if (phase === "loading") {
    return (
      <main className="unlock" aria-busy="true">
        <p className="empty-state">Chargement du coffre…</p>
      </main>
    );
  }

  if (phase === "locked") {
    return (
      <>
        <UnlockPage
          hasVault={hasVault}
          busy={busy}
          error={error}
          biometrics={biometrics}
          onUnlock={handleUnlock}
          onCreate={handleCreate}
          onRestore={async () => {
            try {
              const restored = await api.restoreSnapshot();
              await refreshStatus();
              return restored;
            } catch (caught) {
              setError(toIpcError(caught).message);
              return null;
            }
          }}
        />
        <Toasts toasts={toasts} onDismiss={dismiss} />
      </>
    );
  }

  return (
    <>
      <VaultPage
        api={api}
        notices={notices}
        toasts={toasts}
        dismissToast={dismiss}
        buildInfo={buildInfo}
        autoLockMinutes={autoLockMinutes}
        onLock={handleLock}
      />
    </>
  );
}
