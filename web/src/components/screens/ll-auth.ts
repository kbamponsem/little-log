import { defineComponent, LLComponent } from "../../core/component";
import { errorMessage, fieldValue, setError } from "../../core/form";
import { LL } from "../../core/ui-store";
import type { AuthRequest } from "../../core/ui-store";
import type { AuthMode, Session } from "../../data/model";
import { store } from "../../data/store";
import { esc } from "../../format";
import type { LlChipGroup } from "../ui/ll-chip-group";
import "./ll-auth.css";

const MODE_OPTIONS = [
  { id: "login", label: "Sign in" },
  { id: "register", label: "Create account" },
];

/**
 * The sign-in / create-account screen — the app's front door while there is no
 * session. It replaces what used to be the auth bottom sheet: a full page, so
 * the form gets the whole viewport and the keyboard never fights the scrim.
 *
 * `ll-app` calls `configure()` from `ui.auth` before unhiding it; on success the
 * screen emits `ll-signed-in` and the app shell takes over.
 */
export class LlAuth extends LLComponent {
  /** Invite token from the URL, if the app was opened through one. */
  invite: string | null = null;

  private mode: AuthMode = "login";
  private submitButton: HTMLButtonElement | null = null;
  private registerRow: HTMLElement | null = null;
  private password: HTMLInputElement | HTMLTextAreaElement | null = null;
  private checkedSetup = false;

  /** Point the screen at a request and re-render. Called by `ll-app`. */
  configure(request: AuthRequest): void {
    this.invite = request.invite;
    this.mode = request.mode;
    this.rerender();
  }

  protected template(): string {
    const extras = store.repo
      .authFields()
      .map(
        (field) =>
          `<ll-field name="extra_${esc(field.name)}" label="${esc(field.label)}" type="${esc(field.type)}" value="${esc(field.value)}"></ll-field>`,
      )
      .join("");

    return `
      <div class="auth-card">
        <h1>Little Log</h1>
        <p class="sub" data-role="blurb"></p>
        <ll-chip-group data-role="mode"></ll-chip-group>
        <div data-role="register">
          <ll-field name="name" label="Your name" autocomplete="name"></ll-field>
        </div>
        <ll-field name="email" label="Email" type="email" autocomplete="email"></ll-field>
        <ll-field name="password" label="Password" type="password"></ll-field>
        ${extras}
        <p class="err" hidden></p>
        <button class="go" type="button" data-action="submit"></button>
        <p class="note">The log lives on your device and syncs only to your own server.</p>
      </div>
    `;
  }

  protected mount(): void {
    this.submitButton = this.querySelector<HTMLButtonElement>('[data-action="submit"]');
    this.registerRow = this.querySelector<HTMLElement>('[data-role="register"]');
    this.password = this.querySelector<HTMLInputElement>('ll-field[name="password"] input');

    this.querySelector<LlChipGroup>('[data-role="mode"]')?.configure({
      options: MODE_OPTIONS,
      value: this.mode,
      toggle: false,
    });
    this.applyMode();
    this.checkSetup();
  }

  protected subscribe(): void {
    this.addEventListener("click", (event) => {
      const action = (event.target as HTMLElement).closest<HTMLElement>("[data-action]")?.dataset
        .action;
      if (action === "submit") void this.submit();
    });

    this.addEventListener("ll-change", (event) => {
      if ((event.target as HTMLElement).dataset.role === "mode") {
        this.setMode((event as CustomEvent<{ value: AuthMode }>).detail.value);
      }
    });

    // Enter submits from any text field, the way a real sign-in form should.
    this.addEventListener("keydown", (event) => {
      if (event.key === "Enter" && event.target instanceof HTMLInputElement) void this.submit();
    });
  }

  private setMode(mode: AuthMode): void {
    this.mode = mode;
    this.querySelector<LlChipGroup>('[data-role="mode"]')?.configure({
      options: MODE_OPTIONS,
      value: mode,
      toggle: false,
    });
    this.applyMode();
  }

  /** Targeted updates so switching mode never clears what was typed. */
  private applyMode(): void {
    const register = this.mode === "register";
    if (this.registerRow) this.registerRow.hidden = !register;
    if (this.submitButton) this.submitButton.textContent = register ? "Create account" : "Sign in";
    if (this.password) {
      this.password.setAttribute("autocomplete", register ? "new-password" : "current-password");
    }
    const blurb = this.querySelector<HTMLElement>('[data-role="blurb"]');
    if (blurb) {
      blurb.textContent = this.invite
        ? "You’re invited — this link creates your account and adds you to the family."
        : register
          ? "Create an account to keep this log in sync."
          : "Sign in to sync this log with your family.";
    }
  }

  /** First run ever: there are no accounts, so default to "Create account". */
  private checkSetup(): void {
    if (this.checkedSetup) return;
    this.checkedSetup = true;
    void store.repo
      .status()
      .then((status) => {
        if (status?.needs_setup && this.mode !== "register") this.setMode("register");
      })
      .catch(() => undefined);
  }

  private async submit(): Promise<void> {
    setError(this, null);
    const register = this.mode === "register";
    const name = fieldValue(this, "name");
    const email = fieldValue(this, "email");
    const password = fieldValue(this, "password");

    if (register && !name) return setError(this, "Please enter your name.");
    if (!email || !password) return setError(this, "Email and password are required.");

    for (const field of store.repo.authFields()) {
      const value = fieldValue(this, `extra_${field.name}`);
      if (field.name === "server") store.repo.server = value;
    }

    const button = this.submitButton;
    if (button) button.disabled = true;
    try {
      const session: Session = register
        ? await store.repo.register({ name, email, password, invite: this.invite })
        : await store.repo.login({ email, password });

      // The app shell adopts the session, clears the invite URL, navigates to
      // Today and hides this screen — see `ll-app`'s ll-signed-in.
      this.emit(LL.signedIn, { session });
    } catch (error) {
      setError(this, errorMessage(error));
    } finally {
      if (button) button.disabled = false;
    }
  }
}

defineComponent("ll-auth", LlAuth);

/** `?invite=<token>` from the URL, if any. */
export function readInvite(): string | null {
  try {
    return new URLSearchParams(location.search).get("invite");
  } catch {
    return null;
  }
}
