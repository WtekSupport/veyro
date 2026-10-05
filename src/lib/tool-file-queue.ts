export { escapeHtml } from "./html";

export function fileNameFromPath(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  const segment = normalized.split("/").pop();
  return segment ?? path;
}

export function pathsMatch(left: string, right: string): boolean {
  return left.replace(/\\/g, "/").toLowerCase() === right.replace(/\\/g, "/").toLowerCase();
}

export async function runSequentialJobs<T>(
  jobs: T[],
  runOne: (job: T) => Promise<void>,
): Promise<void> {
  for (const job of jobs) {
    await runOne(job);
  }
}
