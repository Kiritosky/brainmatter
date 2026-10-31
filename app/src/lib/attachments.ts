// Helpers for showing attachments. Their type comes from whoever added the file
// (possibly an AI), so only an explicit list of types is ever rendered, and a
// blob only gets the type it is rendered as: HTML can never run.
import { api, type Attachment } from './api';

export const MAX_ATTACHMENT_BYTES = 200 * 1024 * 1024;

export type PreviewKind = 'image' | 'pdf' | 'text' | 'none';

const RASTER_IMAGE = /^image\/(png|jpeg|gif|webp|svg\+xml)$/;
const TEXT = /^(text\/(plain|markdown|csv)|application\/json)$/;

export function previewKind(mime: string): PreviewKind {
  if (RASTER_IMAGE.test(mime)) return 'image';
  if (mime === 'application/pdf') return 'pdf';
  if (TEXT.test(mime)) return 'text';
  return 'none';
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toLocaleString('de-DE', { maximumFractionDigits: 1 })} MB`;
}

/** Decrypts into a blob URL for images and PDFs. The caller revokes it. */
export async function attachmentUrl(a: Attachment): Promise<string> {
  const data = await api.attachmentData(a.id);
  return URL.createObjectURL(new Blob([data], { type: previewKind(a.mime) === 'none' ? 'application/octet-stream' : a.mime }));
}

export async function attachmentText(a: Attachment): Promise<string> {
  return new TextDecoder().decode(await api.attachmentData(a.id));
}
