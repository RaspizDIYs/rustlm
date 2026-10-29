const tiers = ["Grandmaster", "Challenger", "Platinum", "Emerald", "Diamond", "Bronze", "Silver", "Master", "Iron", "Gold"];
export function rankBadge(display: string) {
 const tier = tiers.find((tier) => new RegExp("\\b" + tier + "\\b", "i").test(display));
 const division = display.match(/\b(IV|III|II|I)\b/i)?.[1]?.toUpperCase();
 const lp = display.match(/\b(\d+)\s*LP\b/i)?.[1];
 return {tier, label: [division, lp].filter(Boolean).join(" ")};
}
