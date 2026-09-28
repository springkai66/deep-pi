/**
 * Run one step of a startup/shutdown sequence with a hard time bound.
 *
 * A step that never settles is the failure mode that leaves the app permanently
 * wedged: a lock held forever makes every later action silently return, which
 * reads to the user as "the app ignores my clicks". The bound is the point —
 * the caller always regains control and can release whatever it was holding.
 * Timing out does NOT cancel the underlying host call; it only stops us waiting.
 */
export type BoundedStepOutcome<T> =
  | { ok: true; value: T }
  | { ok: false; error: unknown };

export async function runBoundedStep<T>(
  name: string,
  step: () => Promise<T>,
  timeoutMs: number,
): Promise<BoundedStepOutcome<T>> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    const value = await Promise.race([
      step(),
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => reject(new Error(`${name} did not settle within ${timeoutMs}ms`)), timeoutMs);
      }),
    ]);
    return { ok: true, value };
  } catch (error) {
    return { ok: false, error };
  } finally {
    if (timer) clearTimeout(timer);
  }
}
