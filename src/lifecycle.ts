// Work that must be saved before the app quits. The backend emits `app-closing` when the
// window is closed or the app is quit; pending saves run, then the app exits.

const savers = new Set<() => unknown>();

/** Registers a function that saves pending work (may return a promise); returns an unregister. */
export function onBeforeExit(save: () => unknown): () => void {
  savers.add(save);
  return () => {
    savers.delete(save);
  };
}

/** Runs every registered saver and waits for them (failures are ignored). */
export async function saveAll(): Promise<void> {
  await Promise.allSettled([...savers].map(async (save) => save()));
}
