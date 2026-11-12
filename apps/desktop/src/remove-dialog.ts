export type RemoveChoice = "cancel" | "remove" | "remove-delete";

let wired = false;

function wireRemoveDialog(dlg: HTMLDialogElement): void {
  if (wired) return;
  wired = true;

  const btnCancel = document.getElementById("remove-cancel");
  const btnRemove = document.getElementById("remove-list-only");
  const btnRemoveDelete = document.getElementById("remove-and-delete");
  const closeBtn = document.getElementById("remove-close-btn");

  const close = () => {
    if (dlg.open) dlg.close();
  };

  btnCancel?.addEventListener("click", () => close());
  closeBtn?.addEventListener("click", () => close());

  dlg.addEventListener("cancel", (e) => {
    e.preventDefault();
    close();
  });

  dlg.addEventListener("close", () => {
    const primary = btnRemove as HTMLButtonElement | null;
    primary?.blur();
  });

  // Store choice on buttons via data attribute before close
  btnRemove?.addEventListener("click", () => {
    dlg.dataset.choice = "remove";
    close();
  });
  btnRemoveDelete?.addEventListener("click", () => {
    dlg.dataset.choice = "remove-delete";
    close();
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

  delete dlg.dataset.choice;

  return new Promise((resolve) => {
    const onClose = () => {
      dlg.removeEventListener("close", onClose);
      const choice = dlg.dataset.choice as RemoveChoice | undefined;
      if (choice === "remove" || choice === "remove-delete") {
        resolve(choice);
      } else {
        resolve("cancel");
      }
    };
    dlg.addEventListener("close", onClose);
    dlg.showModal();
    const focusTarget = document.getElementById(
      "remove-list-only"
    ) as HTMLButtonElement | null;
    focusTarget?.focus();
  });
}
