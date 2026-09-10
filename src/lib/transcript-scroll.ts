export interface TranscriptPort {
  top(): number;
  setTop(value: number): void;
  rows(): { id: string; top: number; bottom: number }[];
}
export interface TranscriptAnchor { id: string; offset: number }

export function messageWindowStart(total: number, limit: number, pinned: number | null): number {
  return pinned !== null && pinned < total ? Math.max(0, pinned) : Math.max(0, total - limit);
}

export function captureTranscriptAnchor(port: TranscriptPort): TranscriptAnchor | null {
  const first = port.rows().find((row) => row.bottom > 0);
  return first ? { id: first.id, offset: first.top } : null;
}

export function restoreTranscriptAnchor(port: TranscriptPort, anchor: TranscriptAnchor | null) {
  if (!anchor) return;
  const row = port.rows().find((candidate) => candidate.id === anchor.id);
  if (row) port.setTop(port.top() + row.top - anchor.offset);
}

export function transcriptPort(container: HTMLElement): TranscriptPort {
  return {
    top: () => container.scrollTop,
    setTop: (value) => { container.scrollTop = value; },
    rows: () => {
      const top = container.getBoundingClientRect().top;
      return [...container.querySelectorAll<HTMLElement>("[data-message-index]")].map((element) => {
        const bounds = element.getBoundingClientRect();
        return { id: element.dataset.messageIndex!, top: bounds.top - top, bottom: bounds.bottom - top };
      });
    },
  };
}
