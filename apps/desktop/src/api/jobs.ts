/**
 * Typed wrappers over the activity-manager commands.
 *
 * `listSyncJobs` reads the current background jobs (running, pending, and
 * recent terminal) with their account/mailbox names already resolved. The
 * two cancel calls request cooperative cancellation. All three degrade
 * gracefully outside the desktop shell (ADR-0005): `listSyncJobs` resolves
 * to `null` and the cancels resolve to `null` without throwing, so callers
 * need no branching.
 */
import { safeInvoke } from "../lib/tauri";
import type { SyncJobDto } from "../types/mail";

/**
 * Lists sync jobs for the activity manager. Resolves to `null` in the
 * browser preview where the IPC bridge is absent.
 */
export function listSyncJobs(): Promise<SyncJobDto[] | null> {
  return safeInvoke<SyncJobDto[]>("list_sync_jobs");
}

/**
 * Requests cooperative cancellation of a single job. A no-op for a job that
 * already finished. Resolves to `null` in the browser preview.
 */
export function cancelSyncJob(jobId: number): Promise<null> {
  return safeInvoke<null>("cancel_sync_job", { jobId });
}

/**
 * Requests cooperative cancellation of every currently tracked job.
 * Resolves to `null` in the browser preview.
 */
export function cancelAllSyncJobs(): Promise<null> {
  return safeInvoke<null>("cancel_all_sync_jobs");
}
