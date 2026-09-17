export function renderDeviceHint(deviceCount: number): string {
  if (deviceCount > 0) {
    return `<p class="hint">${deviceCount} microphone(s) available.</p>`;
  }

  return `<p class="hint">No microphones detected. Check system permissions.</p>`;
}
