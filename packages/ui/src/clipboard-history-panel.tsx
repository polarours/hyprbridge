import { Button, Panel } from "./primitives";

export interface ClipboardHistoryEntry {
  id: string;
  preview: string;
  content_type: string;
  timestamp: string;
  pinned: boolean;
}

export interface ClipboardHistoryPanelProps {
  entries: ClipboardHistoryEntry[];
  onPin: (id: string) => void;
  onUnpin: (id: string) => void;
  onDelete: (id: string) => void;
  onClear: () => void;
}

export function ClipboardHistoryPanel(props: ClipboardHistoryPanelProps) {
  return (
    <Panel title="Clipboard History" eyebrow="Clipboard">
      <div className="ib-clipboard-history">
        {props.entries.length > 0 ? (
          <>
            <div className="ib-clipboard-history__actions">
              <Button tone="neutral" onClick={props.onClear}>Clear All</Button>
            </div>
            <div className="ib-clipboard-history__list">
              {props.entries.map((entry) => (
                <div key={entry.id} className={`ib-clipboard-history__entry ${entry.pinned ? "ib-clipboard-history__entry--pinned" : ""}`}>
                  <div className="ib-clipboard-history__entry-content">
                    <span className="ib-clipboard-history__preview">{entry.preview}</span>
                    <span className="ib-clipboard-history__meta">
                      {entry.content_type} · {new Date(entry.timestamp).toLocaleTimeString()}
                    </span>
                  </div>
                  <div className="ib-clipboard-history__entry-actions">
                    <Button tone="neutral" onClick={() => entry.pinned ? props.onUnpin(entry.id) : props.onPin(entry.id)}>
                      {entry.pinned ? "Unpin" : "Pin"}
                    </Button>
                    <Button tone="neutral" onClick={() => props.onDelete(entry.id)}>×</Button>
                  </div>
                </div>
              ))}
            </div>
          </>
        ) : (
          <p className="ib-copy">No clipboard history yet.</p>
        )}
      </div>
    </Panel>
  );
}