import { spawn, type SpawnOptionsWithoutStdio } from "child_process";

/**
 * Child-process helpers.
 *
 * This module must stay free of any `vscode` import, directly or transitively:
 * it is reachable from `src/uninstall.ts`, which VS Code runs in plain Node
 * where the `vscode` module does not exist.
 */

export interface SpawnAsyncReturns {
  stdout: string;
  stderr: string;
  status: number | null;
  error?: Error | undefined;
}

export async function spawnAsync(
  path: string,
  args?: ReadonlyArray<string>,
  options?: SpawnOptionsWithoutStdio,
): Promise<SpawnAsyncReturns> {
  const child = spawn(path, args, options);
  const stdout: Array<Buffer> = [];
  const stderr: Array<Buffer> = [];
  try {
    const res = await new Promise<{
      stdout: string;
      stderr: string;
      status: number | null;
    }>((resolve, reject) => {
      child.stdout.on("data", (chunk) => stdout.push(Buffer.from(chunk)));
      child.stderr.on("data", (chunk) => stderr.push(Buffer.from(chunk)));
      child.on("error", (error) =>
        reject({
          stdout: Buffer.concat(stdout).toString("utf8"),
          stderr: Buffer.concat(stderr).toString("utf8"),
          error,
        }),
      );
      child.on("close", (status) =>
        resolve({
          stdout: Buffer.concat(stdout).toString("utf8"),
          stderr: Buffer.concat(stderr).toString("utf8"),
          status,
        }),
      );
    });

    return {
      stdout: res.stdout,
      stderr: res.stderr,
      status: res.status,
    };
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
  } catch (e: any) {
    return {
      stdout: e.stdout,
      stderr: e.stderr,
      status: e.status,
      error: e.error,
    };
  }
}
