export async function needsCloudTotpVerification(): Promise<boolean> {
  try {
    const { totpGetStatus, cloudTotpSessionActive } = await import("@/lib/tauri");
    const [totpOn, sessionOk] = await Promise.all([
      totpGetStatus(),
      cloudTotpSessionActive(),
    ]);
    return totpOn && !sessionOk;
  } catch {
    return false;
  }
}
