export function syncStatusOverlayPopoverPosition(popover: HTMLElement): void {
  const statusBar = document.querySelector<HTMLElement>(".status-bar");
  const gap = 8;
  const top =
    statusBar && statusBar.getBoundingClientRect().bottom > 0
      ? statusBar.getBoundingClientRect().bottom + gap
      : 112;
  popover.style.setProperty("--status-quick-settings-top", `${Math.round(top)}px`);
}
