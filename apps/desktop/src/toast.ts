// Rich toast notifications with action buttons and web notifications support

export interface ToastAction {
  label: string;
  onClick: () => void | Promise<void>;
  primary?: boolean;
}

export interface ToastOptions {
  title?: string;
  message: string;
  icon?: string;
  actions?: ToastAction[];
  duration?: number;
  type?: "info" | "success" | "error" | "warning";
}

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

export function showToast(
  messageOrOptions: string | ToastOptions,
  duration = 3200
): HTMLElement {
  const container = ensureContainer();
  const options: ToastOptions =
    typeof messageOrOptions === "string"
      ? { message: messageOrOptions, duration }
      : { duration: 4000, ...messageOrOptions };

  const toast = document.createElement("div");
  toast.className = `toast ${options.type || "info"}`;
  if (options.actions && options.actions.length > 0) {
    toast.classList.add("toast-with-actions");
  }

  let html = "";
  if (options.icon) {
    html += `<span class="toast-icon">${options.icon}</span>`;
  }
  html += `<div class="toast-content">`;
  if (options.title) {
    html += `<div class="toast-title">${options.title}</div>`;
  }
  html += `<div class="toast-message">${options.message}</div>`;
  html += `</div>`;

  if (options.actions && options.actions.length > 0) {
    html += `<div class="toast-actions"></div>`;
  }

  html += `<button type="button" class="toast-dismiss" aria-label="Dismiss">&times;</button>`;
  toast.innerHTML = html;

  if (options.actions && options.actions.length > 0) {
    const actionsContainer = toast.querySelector(".toast-actions")!;
    options.actions.forEach((action) => {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = `toast-action-btn ${action.primary ? "primary" : ""}`;
      btn.textContent = action.label;
      btn.addEventListener("click", async (e) => {
        e.stopPropagation();
        try {
          await action.onClick();
        } finally {
          dismissToast(toast);
        }
      });
      actionsContainer.appendChild(btn);
    });
  }

  const dismissBtn = toast.querySelector(".toast-dismiss");
  dismissBtn?.addEventListener("click", () => dismissToast(toast));

  container.appendChild(toast);

  requestAnimationFrame(() => {
    toast.classList.add("show");
  });

  const timeoutMs = options.duration ?? 3200;
  if (timeoutMs > 0) {
    const timer = setTimeout(() => {
      dismissToast(toast);
    }, timeoutMs);

    // Pause dismissal on hover if it has actions
    if (options.actions && options.actions.length > 0) {
      toast.addEventListener("mouseenter", () => clearTimeout(timer));
      toast.addEventListener("mouseleave", () => {
        setTimeout(() => dismissToast(toast), 2000);
      });
    }
  }

  return toast;
}

function dismissToast(toast: HTMLElement) {
  toast.classList.remove("show");
  setTimeout(() => {
    if (toast.parentNode) {
      toast.parentNode.removeChild(toast);
    }
  }, 220);
}

// Request and dispatch standard Web Notifications when enabled
export async function requestNotificationPermission(): Promise<boolean> {
  if (!("Notification" in window)) return false;
  if (Notification.permission === "granted") return true;
  if (Notification.permission !== "denied") {
    const permission = await Notification.requestPermission();
    return permission === "granted";
  }
  return false;
}

export function dispatchSystemNotification(
  title: string,
  options?: { body?: string; icon?: string; onClick?: () => void }
) {
  if (!("Notification" in window) || Notification.permission !== "granted") {
    return;
  }
  try {
    const notif = new Notification(title, {
      body: options?.body,
      icon: options?.icon || "/src/assets/logo.svg",
    });
    if (options?.onClick) {
      notif.onclick = () => {
        window.focus();
        options.onClick!();
      };
    }
  } catch (err) {
    console.warn("Could not dispatch web notification:", err);
  }
}
