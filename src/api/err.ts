/**
 * Best-effort error message extraction for `invoke` rejections and JS errors.
 * Rust-side serializes AppError as a plain string, but JS errors and
 * pre-flight validation failures still come through as Error objects.
 */
export function errMsg(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  if (e && typeof e === "object") {
    const obj = e as Record<string, unknown>;
    if (typeof obj.message === "string") return obj.message;
    try {
      return JSON.stringify(obj);
    } catch {
      return String(obj);
    }
  }
  return String(e);
}
