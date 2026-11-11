// Simple, elegant toast notifications for user actions

let toastContainer: HTMLElement | null = null;

function ensureContainer(): HTMLElement {
  if (!toastContainer || !document.body.contains(toastContainer)) {
    toastContainer = document.createElement("div");
    toastContainer.className = "toast-container";
    toastContainer.setAttribute("aria-live", "polite");
    document.body.appendChild(toastContainer);
  }
  return toastContainer;
}

export function showToast(message: string, duration = 2400) {
  const container = ensureContainer();
  const el = document.createElement("div");
  el.className = "toast";
  el.textContent = message;

  container.appendChild(el);

  // Trigger enter transition
  requestAnimationFrame(() => {
    el.classList.add("show");
  });

  setTimeout(() => {
    el.classList.remove("show");
    setTimeout(() => {
      if (el.parentNode) el.parentNode.removeChild(el);
    }, 200);
  }, duration);
}
