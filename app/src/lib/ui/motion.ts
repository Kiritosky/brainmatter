// Shared transitions. Everything collapses to instant when the user asks the
// system for reduced motion.
import { cubicOut, quintOut } from 'svelte/easing';
import type { TransitionConfig } from 'svelte/transition';

const reduced = () =>
  typeof matchMedia !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches;

/** Dialogs and popovers: fade in while growing from 97 % and rising 6 px. */
export function pop(_node: Element, { duration = 180, y = 6 } = {}): TransitionConfig {
  if (reduced()) return { duration: 0 };
  return {
    duration,
    easing: quintOut,
    css: (t) => `opacity: ${t}; transform: translateY(${(1 - t) * y}px) scale(${0.97 + 0.03 * t})`,
  };
}

/** Side panels: slide in from the right. */
export function slideIn(_node: Element, { duration = 220, x = 24 } = {}): TransitionConfig {
  if (reduced()) return { duration: 0 };
  return { duration, easing: cubicOut, css: (t) => `opacity: ${t}; transform: translateX(${(1 - t) * x}px)` };
}

/** Backdrops and simple fades. */
export function fadeQuick(_node: Element, { duration = 140 } = {}): TransitionConfig {
  if (reduced()) return { duration: 0 };
  return { duration, css: (t) => `opacity: ${t}` };
}
