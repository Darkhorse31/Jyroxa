export type RemoveChoice = "cancel" | "remove" | "remove-delete";

// Resolver for the prompt that is currently showing. Buttons settle it
// directly: in the installed WebView2 build the dialog's "close" event was not
// delivered after close() from these handlers, so waiting on it left every
// Remove click unanswered.
let settle: ((choice: RemoveChoice) => void) | null = null;
let wired = false;

function finish(dlg: HTMLDialogElement, choice: RemoveChoice): void {
  const done = settle;
  settle = null;
  if (dlg.open) dlg.close();
  done?.(choice);
}

function wireRemoveDialog(dlg: HTMLDialogElement): void {
  if (wired) return;
  wired = true;

  const on = (id: string, choice: RemoveChoice) =>
    document.getElementById(id)?.addEventListener("click", (e) => {
      e.preventDefault();
      finish(dlg, choice);
    });
  on("remove-cancel", "cancel");
  on("remove-close-btn", "cancel");
  on("remove-list-only", "remove");
  on("remove-and-delete", "remove-delete");

  dlg.addEventListener("cancel", (e) => {
    e.preventDefault();
    finish(dlg, "cancel");
  });
  // Closed some other way (Escape handler in main.ts): treat as Cancel.
  dlg.addEventListener("close", () => {
    if (settle) finish(dlg, "cancel");
  });
}

/**
 * Show remove confirmation for one or more downloads.
 * Escape and Cancel dismiss without removing.
 */
export function promptRemoveDownloads(count: number): Promise<RemoveChoice> {
  const dlg = document.getElementById("dlg-remove") as HTMLDialogElement | null;
  if (!dlg) {
    return Promise.resolve("cancel");
  }

  wireRemoveDialog(dlg);
  // A prompt still pending (should not happen) is answered as Cancel.
  if (settle) finish(dlg, "cancel");

  const title = document.getElementById("remove-dialog-title");
  const body = document.getElementById("remove-dialog-body");
  if (title) {
    title.textContent =
      count === 1 ? "Remove download?" : `Remove ${count} downloads?`;
  }
  if (body) {
    body.textContent =
      count === 1
        ? "Remove this item from the list, or also delete its file from disk."
        : "Remove the selected items from the list, or also delete their files from disk.";
  }

  return new Promise((resolve) => {
    settle = resolve;
    if (!dlg.open) dlg.showModal();
    (document.getElementById("remove-list-only") as HTMLButtonElement | null)?.focus();
  });
}