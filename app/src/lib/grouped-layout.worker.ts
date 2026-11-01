import { groupedLayout, type LayoutInput } from './grouped-layout.ts';

self.onmessage = (e: MessageEvent<{ run: number; input: LayoutInput }>) => {
  const { run, input } = e.data;
  self.postMessage({ run, result: groupedLayout(input) });
};
