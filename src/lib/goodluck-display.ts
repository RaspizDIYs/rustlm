import type { GoodLuckUser } from "@/lib/tauri";

export function goodluckAvatarSrc(user: GoodLuckUser): string {
  const dataUrl = user.local_avatar_path?.trim();
  if (dataUrl && dataUrl.startsWith("data:")) {
    return dataUrl;
  }
  if (user.avatar_url?.trim()) {
    return user.avatar_url;
  }
  return "";
}
