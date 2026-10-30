/** Stable tint per AI client, so different AIs are easy to tell apart.
 * No green or red: those read as success and failure here. */
const AI_TINTS = ['#c38bff', '#4cc9f0', '#ff6fb5', '#ffa24c', '#8fa8ff', '#e6c64f', '#b6a4ff'];

export function tint(name: string) {
  let hash = 2166136261;
  for (const ch of name) hash = Math.imul(hash ^ ch.charCodeAt(0), 16777619) >>> 0;
  return AI_TINTS[hash % AI_TINTS.length];
}

/** Inline style for a round avatar in the given colour. */
export const avatarStyle = (color: string) =>
  `color: ${color}; background: color-mix(in srgb, ${color} 16%, transparent); box-shadow: inset 0 0 0 1px color-mix(in srgb, ${color} 30%, transparent)`;
