#!/usr/bin/env node
import { execFile, spawn } from "node:child_process";
import { closeSync, openSync, writeSync } from "node:fs";
import { chmod, mkdtemp, rm } from "node:fs/promises";
import { createConnection } from "node:net";
import { constants, tmpdir } from "node:os";
import { isAbsolute, join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const LOG_LIMIT = 64 * 1024;
const START_TIMEOUT_MS = 15_000;
// CanvasTTY allows 2s before force-closing its PTY. Leave time to reap children.
const STOP_TIMEOUT_MS = 750;
const VERSION_ERROR = "This test frontend requires official codex-cli 0.159.2; backend version check failed.";
const VALUE_FLAGS = new Set([
  "-m", "--model", "--local-provider", "-p", "--profile", "-s", "--sandbox",
  "-a", "--ask-for-approval", "-C", "--cd", "--add-dir"
]);
const BOOLEAN_FLAGS = new Set([
  "--strict-config", "--search", "--no-alt-screen", "--oss", "--worktree",
  "--dangerously-bypass-hook-trust", "-h", "--help", "-V", "--version"
]);

export function parseArguments(argv) {
  const separator = argv.indexOf("--");
  if (separator < 0) throw new Error("Expected --backend ABS --frontend ABS -- [Codex flags].");
  const paths = {};
  for (let i = 0; i < separator; i += 2) {
    const key = argv[i];
    const value = argv[i + 1];
    if (!["--backend", "--frontend"].includes(key) || paths[key] ||
        i + 1 >= separator || !value || !isAbsolute(value) || value.includes("\0")) {
      throw new Error("Backend and frontend must each be specified once as absolute executable paths.");
    }
    paths[key] = value;
  }
  if (!paths["--backend"] || !paths["--frontend"]) {
    throw new Error("Both --backend and --frontend are required.");
  }

  const frontendArgs = [];
  const backendConfig = [];
  let auto = false;
  let yolo = false;
  let sandbox;
  let approval;
  let resume = false;
  let resumeId;
  let resumeLast = false;
  const resumeExtras = [];
  const args = argv.slice(separator + 1);
  for (let i = 0; i < args.length; i++) {
    const token = args[i];
    if (token.includes("\0")) throw new Error("Arguments must not contain NUL bytes.");
    const equals = token.startsWith("--") ? token.indexOf("=") : -1;
    const flag = equals < 0 ? token : token.slice(0, equals);
    const value = () => {
      const result = equals < 0 ? args[++i] : token.slice(equals + 1);
      if (result === undefined || !result || result.startsWith("-") || result.includes("\0")) {
        throw new Error("A Codex flag is missing its value.");
      }
      return result;
    };
    if (flag === "-c" || flag === "--config") {
      const override = value();
      if (override.indexOf("=") < 1 || !override.slice(0, override.indexOf("=")).trim()) {
        throw new Error("Config overrides must have a nonempty KEY=VALUE.");
      }
      backendConfig.push("-c", override);
      frontendArgs.push("-c", override);
    } else if (flag === "--no-daemon" && equals < 0) {
      // This launcher owns the server instead of consulting the shared daemon.
    } else if (["--approve-for-me", "--not-so-yolo"].includes(flag) && equals < 0) {
      if (auto) throw new Error("Duplicate approval mode.");
      auto = true;
      frontendArgs.push("--approve-for-me");
    } else if (["--dangerously-bypass-approvals-and-sandbox", "--yolo", "-yolo"].includes(flag) && equals < 0) {
      if (yolo) throw new Error("Duplicate approval mode.");
      yolo = true;
      frontendArgs.push("--dangerously-bypass-approvals-and-sandbox");
    } else if (token === "resume") {
      if (resume) throw new Error("Duplicate resume command.");
      resume = true;
      if (args[i + 1] && !args[i + 1].startsWith("-")) {
        resumeId = args[++i];
        if (!UUID.test(resumeId)) throw new Error("Resume accepts only a canonical session UUID.");
        resumeId = resumeId.toLowerCase();
      }
    } else if (resume && equals < 0 && ["--last", "--all", "--include-non-interactive"].includes(flag)) {
      if (flag === "--last") resumeLast = true;
      else resumeExtras.push(flag === "--all" ? "--resume-show-all" : "--resume-include-non-interactive");
    } else if (VALUE_FLAGS.has(flag)) {
      const result = value();
      frontendArgs.push(flag, result);
      if (["-s", "--sandbox"].includes(flag)) {
        if (sandbox || !["read-only", "workspace-write", "danger-full-access"].includes(result)) {
          throw new Error("Invalid or duplicate sandbox mode.");
        }
        sandbox = result;
      } else if (["-a", "--ask-for-approval"].includes(flag)) {
        if (approval || !["on-request", "never"].includes(result)) {
          throw new Error("Invalid or duplicate approval policy.");
        }
        approval = result;
      }
    } else if (BOOLEAN_FLAGS.has(flag) && equals < 0) {
      frontendArgs.push(flag);
    } else {
      throw new Error("Unsupported Codex argument; positional prompts and custom remote endpoints are forbidden.");
    }
  }
  if ((auto && (yolo || sandbox || approval)) || (yolo && approval)) {
    throw new Error("Conflicting approval or sandbox flags.");
  }
  if (resumeLast && resumeId) throw new Error("Choose either resume UUID or --last.");
  if (auto) {
    backendConfig.push("-c", 'approvals_reviewer="auto_review"', "-c", 'approval_policy="on-request"', "-c", 'sandbox_mode="workspace-write"');
  } else {
    if (yolo || approval) backendConfig.push("-c", `approval_policy="${yolo ? "never" : approval}"`);
    if (yolo || sandbox) backendConfig.push("-c", `sandbox_mode="${yolo ? "danger-full-access" : sandbox}"`);
  }
  if (resume) {
    frontendArgs.push(...(resumeId ? ["--resume-session-id", resumeId] : [resumeLast ? "--resume-last" : "--resume"]), ...resumeExtras);
  }
  return { backend: paths["--backend"], frontend: paths["--frontend"], backendConfig, frontendArgs };
}

function childState(command, args, options) {
  const child = spawn(command, args, options);
  const state = { child, done: false, exitDone: false, error: false };
  state.exited = new Promise((resolveExit) => {
    child.on("error", () => {
      state.error = true;
      state.exitDone = true;
      resolveExit({ error: true });
    });
    child.once("exit", (code, signal) => {
      state.exitDone = true;
      resolveExit({ code, signal, error: state.error });
    });
  });
  state.closed = new Promise((resolveClosed) => {
    child.once("close", (code, signal) => {
      state.done = true;
      resolveClosed({ code, signal, error: state.error });
    });
  });
  return state;
}

async function verifyBackendVersion(command, signal) {
  let result;
  const child = execFile(command, ["--version"], {
    cwd: process.cwd(), env: process.env, signal,
    timeout: 3_000, maxBuffer: 16 * 1024, killSignal: "SIGKILL"
  }, (error, stdout) => { result = { error, stdout }; });
  await new Promise((resolveClosed) => child.once("close", resolveClosed));
  if (result?.error || result?.stdout.trim() !== "codex-cli 0.159.2") throw new Error(VERSION_ERROR);
}

function probeListener(socket, signal) {
  return new Promise((resolveProbe) => {
    const connection = createConnection(socket);
    const finish = (ready) => {
      signal.removeEventListener("abort", abort);
      connection.destroy();
      resolveProbe(ready);
    };
    const abort = () => finish(false);
    connection.once("connect", () => finish(true));
    connection.once("error", () => finish(false));
    connection.setTimeout(200, () => finish(false));
    signal.addEventListener("abort", abort, { once: true });
    if (signal.aborted) abort();
  });
}

async function waitForListener(socket, backend, signal) {
  const deadline = Date.now() + START_TIMEOUT_MS;
  while (!signal.aborted && !backend.exitDone && Date.now() < deadline) {
    if (await probeListener(socket, signal)) return;
    await delay(50, undefined, { signal });
  }
  throw new Error("Official Codex backend did not become ready.");
}

function groupAlive(pid) {
  try { process.kill(-pid, 0); return true; }
  catch (error) { if (error.code === "ESRCH") return false; throw error; }
}

function signalGroup(pid, signal) {
  try { process.kill(-pid, signal); }
  catch (error) { if (error.code !== "ESRCH") throw error; }
}

async function stopBackend(state) {
  if (!state) return;
  const pid = state.child.pid;
  if (pid) {
    // spawn(detached:true) established this group; never discover other PIDs.
    signalGroup(pid, "SIGTERM");
    const deadline = Date.now() + STOP_TIMEOUT_MS;
    while (groupAlive(pid) && Date.now() < deadline) await delay(25);
    if (groupAlive(pid)) signalGroup(pid, "SIGKILL");
  }
  await state.closed;
}

async function stopFrontend(state) {
  if (!state) return;
  if (!state.done) {
    state.child.kill("SIGTERM");
    const timer = setTimeout(() => { if (!state.done) state.child.kill("SIGKILL"); }, STOP_TIMEOUT_MS);
    try { await state.closed; } finally { clearTimeout(timer); }
  } else await state.closed;
}

export async function runLauncher(config) {
  const controller = new AbortController();
  let interruptedCode;
  const handlers = new Map(["SIGINT", "SIGTERM", "SIGHUP"].map((name) => [name, () => {
    interruptedCode ??= 128 + constants.signals[name];
    controller.abort();
  }]));
  for (const [name, handler] of handlers) process.on(name, handler);
  let directory;
  let logFd;
  let backend;
  let frontend;
  let logBytes = 0;
  try {
    await verifyBackendVersion(config.backend, controller.signal);
    if (controller.signal.aborted) return interruptedCode;
    directory = await mkdtemp(join(tmpdir(), "ctty-tui-"));
    await chmod(directory, 0o700);
    const socket = join(directory, "rpc.sock");
    logFd = openSync(join(directory, "backend.log"), "wx", 0o600);
    if (controller.signal.aborted) return interruptedCode;
    backend = childState(config.backend, ["app-server", "--listen", `unix://${socket}`, ...config.backendConfig], {
      cwd: process.cwd(),
      env: { ...process.env, CODEX_INTERNAL_APP_SERVER_REMOTE_CONTROL_DISABLED: "1" },
      detached: true,
      stdio: ["ignore", "pipe", "pipe"]
    });
    const drain = (chunk) => {
      const data = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
      const remaining = LOG_LIMIT - logBytes;
      if (remaining > 0) {
        try { logBytes += writeSync(logFd, data, 0, Math.min(data.length, remaining)); }
        catch { interruptedCode ??= 1; controller.abort(); }
      }
    };
    backend.child.stdout.on("data", drain);
    backend.child.stderr.on("data", drain);
    await waitForListener(socket, backend, controller.signal);
    if (controller.signal.aborted) return interruptedCode;
    frontend = childState(config.frontend, ["--remote", `unix://${socket}`, "--owned-local-server", ...config.frontendArgs], {
      cwd: process.cwd(), env: process.env, stdio: "inherit"
    });
    const stopped = new Promise((resolveStop) => controller.signal.addEventListener("abort", () => resolveStop({ type: "signal" }), { once: true }));
    const result = await Promise.race([
      backend.exited.then((outcome) => ({ type: "backend", ...outcome })),
      frontend.closed.then((outcome) => ({ type: "frontend", ...outcome })),
      stopped
    ]);
    if (result.type === "signal") return interruptedCode;
    if (result.type === "backend" || result.error) throw new Error("A Codex process exited unexpectedly.");
    return result.code ?? (128 + (constants.signals[result.signal] ?? 1));
  } catch (error) {
    if (controller.signal.aborted) return interruptedCode;
    const knownErrors = [VERSION_ERROR, "Official Codex backend did not become ready."];
    process.stderr.write(`CanvasTTY test TUI: ${error instanceof Error && knownErrors.includes(error.message) ? error.message : "Unable to run the isolated Codex session."}\n`);
    return 1;
  } finally {
    try {
      await Promise.all([stopFrontend(frontend), stopBackend(backend)]);
    } finally {
      if (logFd !== undefined) closeSync(logFd);
      if (directory) await rm(directory, { recursive: true, force: true });
      for (const [name, handler] of handlers) process.removeListener(name, handler);
    }
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { process.exitCode = await runLauncher(parseArguments(process.argv.slice(2))); }
  catch { process.stderr.write("CanvasTTY test TUI: invalid launcher arguments.\n"); process.exitCode = 2; }
}
