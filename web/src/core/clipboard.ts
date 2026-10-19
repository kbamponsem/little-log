/**
 * Put text on the clipboard, and say whether it actually got there.
 *
 * `navigator.clipboard` needs a secure context and permission, and a Tauri
 * WebView on Android gives neither reliably — it rejects. The old code answered
 * a rejection by calling `window.prompt`, which never appears in a WebView, and
 * then claimed success anyway: the button said "Copied ✓" while the clipboard
 * stayed empty. `execCommand` is deprecated, but it is the one that works inside
 * a WebView, so it is the fallback.
 */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    /* fall through — WebViews usually refuse the async clipboard */
  }

  try {
    const area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.setAttribute("aria-hidden", "true");
    area.style.position = "fixed";
    area.style.top = "-1000px";
    area.style.opacity = "0";
    document.body.append(area);
    area.select();
    area.setSelectionRange(0, text.length);
    const copied = document.execCommand("copy");
    area.remove();
    return copied;
  } catch {
    return false;
  }
}
