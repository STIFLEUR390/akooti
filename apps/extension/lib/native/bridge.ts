/**
 * Typed client of the Native Messaging bridge.
 *
 * The native host is a frame pump to the daemon socket, so one port behaves
 * like one daemon connection: requests are answered in order, one at a time.
 * The bridge therefore serialises them, keeps one port open while the service
 * worker lives, and reconnects on the next call when the port died (browser
 * restart, daemon restart, extension reload) — no timer to leak in an MV3
 * worker that the browser stops at will.
 *
 * Nothing here holds a secret: values travel from the daemon to the caller and
 * are passed on without being logged or stored.
 */
import {
  type DaemonRequest,
  type DaemonResponse,
  HOST_NAME,
  PROTOCOL_VERSION,
  handshakeRequest,
} from "../protocol";

/** Error codes the bridge itself can produce. */
export type BridgeErrorCode =
  | "host_missing"
  | "daemon_unavailable"
  | "protocol"
  | "timeout"
  | "not_paired"
  | "locked"
  | "denied";

/** Failure raised before or during a bridge exchange. */
export class BridgeError extends Error {
  constructor(
    readonly code: BridgeErrorCode,
    message: string,
  ) {
    super(message);
    this.name = "BridgeError";
  }
}

/** The subset of `browser.runtime.Port` the bridge uses. */
export interface NativePortLike {
  postMessage(message: unknown): void;
  disconnect(): void;
  onMessage: { addListener(listener: (message: unknown) => void): void };
  onDisconnect: { addListener(listener: () => void): void };
}

/** Injectable environment: the browser in production, a double in tests. */
export interface BridgeDeps {
  connectNative(hostName: string): NativePortLike;
  /** Milliseconds before a silent daemon is reported as a timeout. */
  timeoutMs?: number;
  setTimeout?: (handler: () => void, milliseconds: number) => unknown;
  clearTimeout?: (handle: unknown) => void;
}

const DEFAULT_TIMEOUT_MS = 15_000;

interface Exchange {
  resolve(response: DaemonResponse): void;
  reject(error: BridgeError): void;
  timer: unknown;
}

/**
 * Serialises requests over one Native Messaging port.
 *
 * A rejected handshake (`version_mismatch`) or a missing host stays sticky
 * until {@link reset}, so the popup shows one clear message per session
 * instead of retrying a host that is not installed.
 */
export class NativeBridge {
  private port: NativePortLike | null = null;
  private exchange_: Exchange | null = null;
  private queue: Promise<unknown> = Promise.resolve();
  private handshake: DaemonResponse | null = null;
  private sticky: BridgeError | null = null;

  private readonly hostName: string;
  private readonly timeoutMs: number;
  private readonly setTimer: (handler: () => void, milliseconds: number) => unknown;
  private readonly clearTimer: (handle: unknown) => void;

  constructor(
    private readonly deps: BridgeDeps,
    options: { hostName?: string } = {},
  ) {
    this.hostName = options.hostName ?? HOST_NAME;
    this.timeoutMs = deps.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    this.setTimer = deps.setTimeout ?? ((handler, ms) => setTimeout(handler, ms));
    this.clearTimer = deps.clearTimeout ?? ((handle) => clearTimeout(handle as number));
  }

  /** Whether a port is currently open. */
  get connected(): boolean {
    return this.port !== null;
  }

  /** Forgets the port, the handshake and any sticky failure. */
  reset(): void {
    this.failPending(new BridgeError("daemon_unavailable", "the bridge was reset"));
    this.dropPort();
    this.sticky = null;
    this.handshake = null;
  }

  /**
   * Sends one request, handshaking first when needed.
   *
   * @throws {BridgeError} when the host is missing, the daemon is gone, the
   * protocol versions disagree, or the daemon stays silent past the timeout.
   */
  async request(request: DaemonRequest): Promise<DaemonResponse> {
    if (this.sticky) {
      throw this.sticky;
    }
    if (request.method !== "handshake") {
      await this.ensureHandshake();
    }
    return this.post(request);
  }

  /** Handshake answer of the open port, when it already happened. */
  get capabilities(): string[] {
    if (this.handshake?.kind === "handshake") {
      return this.handshake.capabilities;
    }
    return [];
  }

  private async ensureHandshake(): Promise<void> {
    if (this.handshake) {
      return;
    }
    const answer = await this.post(handshakeRequest());
    this.acceptHandshake(answer);
  }

  private acceptHandshake(answer: DaemonResponse): void {
    if (answer.kind === "error") {
      this.sticky = new BridgeError(
        answer.code === "version_mismatch" ? "protocol" : "daemon_unavailable",
        answer.message,
      );
      throw this.sticky;
    }
    if (answer.kind !== "handshake") {
      throw new BridgeError("protocol", "the daemon did not answer the handshake");
    }
    if (answer.protocolVersion !== PROTOCOL_VERSION) {
      this.sticky = new BridgeError(
        "protocol",
        `the daemon speaks protocol ${answer.protocolVersion}, this extension needs ${PROTOCOL_VERSION}`,
      );
      throw this.sticky;
    }
    this.handshake = answer;
  }

  /**
   * Queues one exchange so a single port never carries two requests at once.
   *
   * The daemon answers in order on one connection, so order is the only
   * correlation the protocol needs.
   */
  private post(request: DaemonRequest): Promise<DaemonResponse> {
    const next = this.queue.then(() => this.exchange(request));
    // A failed exchange must not poison the queue for the following calls.
    this.queue = next.catch(() => undefined);
    return next;
  }

  private exchange(request: DaemonRequest): Promise<DaemonResponse> {
    return new Promise<DaemonResponse>((resolve, reject) => {
      let port: NativePortLike;
      try {
        port = this.ensurePort();
      } catch (error) {
        reject(
          error instanceof BridgeError
            ? error
            : new BridgeError("host_missing", "the Akooti native host is not registered"),
        );
        return;
      }
      const timer = this.setTimer(() => {
        this.failPending(new BridgeError("timeout", "the Akooti daemon did not answer in time"));
      }, this.timeoutMs);
      this.exchange_ = { resolve, reject, timer };
      try {
        port.postMessage(request);
      } catch {
        this.failPending(
          new BridgeError("daemon_unavailable", "the bridge closed before the request was sent"),
        );
      }
    });
  }

  private ensurePort(): NativePortLike {
    if (this.port) {
      return this.port;
    }
    const port = this.deps.connectNative(this.hostName);
    port.onMessage.addListener((message) => this.onMessage(message));
    port.onDisconnect.addListener(() => {
      // The host exits when the browser closes the port or the daemon stops.
      this.port = null;
      this.handshake = null;
      this.failPending(
        new BridgeError("daemon_unavailable", "the Akooti desktop app is not running"),
      );
    });
    this.port = port;
    return port;
  }

  private onMessage(message: unknown): void {
    const current = this.exchange_;
    if (!current) {
      // Nothing awaits this frame: it can only come from a stale session.
      return;
    }
    this.exchange_ = null;
    this.clearTimer(current.timer);
    const response = message as DaemonResponse;
    if (typeof response !== "object" || response === null || typeof response.kind !== "string") {
      current.reject(new BridgeError("protocol", "the daemon answered an invalid frame"));
      return;
    }
    current.resolve(response);
  }

  private failPending(error: BridgeError): void {
    const current = this.exchange_;
    if (!current) {
      return;
    }
    this.exchange_ = null;
    this.clearTimer(current.timer);
    current.reject(error);
  }

  private dropPort(): void {
    const port = this.port;
    this.port = null;
    if (!port) {
      return;
    }
    try {
      port.disconnect();
    } catch {
      // A port already closed by the browser throws on disconnect.
    }
  }
}

/**
 * Turns a daemon refusal into a typed bridge error.
 *
 * `locked` and `invalid_credentials` are worth their own code: the popup has a
 * distinct call to action for each (unlock the app, re-pair the token).
 */
export function bridgeErrorFromResponse(response: DaemonResponse): BridgeError | null {
  if (response.kind !== "error") {
    return null;
  }
  switch (response.code) {
    case "locked":
      return new BridgeError("locked", "the vault is locked; unlock Akooti to continue");
    case "invalid_credentials":
      return new BridgeError("not_paired", "the token was rejected; pair the extension again");
    case "forbidden":
      return new BridgeError("denied", response.message);
    case "daemon_unavailable":
      return new BridgeError("daemon_unavailable", response.message);
    default:
      return new BridgeError("protocol", response.message);
  }
}
