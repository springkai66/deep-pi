export interface PetTaskHover {
  revision: number;
  hovered: boolean;
}

/** Track pet hover across popup startup, including events racing with the initial snapshot. */
export function observePetTaskHover(
  listen: (onHover: (state: PetTaskHover) => void) => Promise<() => void>,
  snapshot: () => Promise<PetTaskHover>,
  changed: (hovered: boolean) => void,
): () => void {
  let revision = 0;
  let disposed = false;
  const listener = listen((state) => {
    if (disposed || state.revision < revision) return;
    revision = state.revision;
    changed(state.hovered);
  });
  // The native event can arrive while registration is in flight. Read the snapshot
  // only after subscription is live so that neither side of that gap is lost.
  void listener.then(() => snapshot()).then((state) => {
    if (disposed || state.revision < revision) return;
    revision = state.revision;
    if (state.hovered) changed(true);
  }).catch(() => {});
  return () => {
    disposed = true;
    void listener.then((unlisten) => unlisten()).catch(() => {});
  };
}
