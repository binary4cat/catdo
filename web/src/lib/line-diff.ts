// Markdown line-diff engine.
// It deliberately emits one contiguous, snapshot-anchored operation. When
// line-ending semantics cannot be represented losslessly, it returns [] so
// the caller uses a guarded full PUT instead of risking a malformed patch.

export interface PatchOperation {
  op: 'replace_lines' | 'insert_before' | 'insert_after' | 'delete_lines' | 'append';
  start_line?: number;
  end_line?: number;
  line?: number;
  text?: string;
  expected_hash?: string;
  anchor_hash?: string;
  expected_tail_hash?: string;
}

interface ParsedText {
  lines: string[];
  eol: '\n' | '\r\n';
  trailingNewline: boolean;
  valid: boolean;
}

function parseText(text: string): ParsedText {
  let crlf = 0;
  let lf = 0;
  let loneCr = false;
  for (let index = 0; index < text.length; index += 1) {
    if (text[index] === '\r') {
      if (text[index + 1] === '\n') crlf += 1;
      else loneCr = true;
    } else if (text[index] === '\n') {
      lf += 1;
    }
  }

  if (loneCr || (crlf > 0 && lf > 0)) {
    return { lines: [], eol: '\n', trailingNewline: false, valid: false };
  }

  const eol: '\n' | '\r\n' = crlf > 0 ? '\r\n' : '\n';
  const normalized = eol === '\r\n' ? text.replaceAll('\r\n', '\n') : text;
  const trailingNewline = normalized.endsWith('\n');
  const body = trailingNewline ? normalized.slice(0, -1) : normalized;
  const lines = body === '' ? (trailingNewline ? [''] : []) : body.split('\n');
  return { lines, eol, trailingNewline, valid: true };
}

function patchText(lines: string[], trailingNewline: boolean, eol: '\n' | '\r\n'): string {
  const body = lines.join(eol);
  return trailingNewline ? `${body}${eol}` : body;
}

/** SHA-256-based short hash matching backend's 16-hex-character anchor hash. */
async function sha256Short(text: string): Promise<string> {
  const data = new TextEncoder().encode(text);
  const hashBuf = await crypto.subtle.digest('SHA-256', data);
  const bytes = new Uint8Array(hashBuf);
  return Array.from(bytes.slice(0, 8))
    .map((byte) => byte.toString(16).toUpperCase().padStart(2, '0'))
    .join('');
}

async function linesHash(lines: string[], start: number, end: number): Promise<string> {
  return sha256Short(lines.slice(start - 1, end).join('\n'));
}

export async function contentVersion(content: string): Promise<string> {
  const data = new TextEncoder().encode(content);
  const hashBuf = await crypto.subtle.digest('SHA-256', data);
  const bytes = new Uint8Array(hashBuf);
  const hex = Array.from(bytes)
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('');
  return `sha256:${hex}`;
}

/**
 * Compute a lossless contiguous line patch.
 *
 * A single contiguous operation is intentional: it avoids ambiguous ordering
 * when several edits target the same snapshot. If the edit changes EOL style
 * or only the final-newline bit, the caller uses full PUT with version guard.
 */
export async function computeLineDiff(
  oldText: string,
  newText: string,
): Promise<PatchOperation[]> {
  if (oldText === newText) return [];

  const oldDoc = parseText(oldText);
  const newDoc = parseText(newText);
  if (!oldDoc.valid || !newDoc.valid || oldDoc.eol !== newDoc.eol) return [];
  if (oldDoc.trailingNewline !== newDoc.trailingNewline) return [];

  const totalLines = oldDoc.lines.length + newDoc.lines.length;
  if (totalLines > 20000) return [];

  let prefixLen = 0;
  const minLen = Math.min(oldDoc.lines.length, newDoc.lines.length);
  while (prefixLen < minLen && oldDoc.lines[prefixLen] === newDoc.lines[prefixLen]) {
    prefixLen += 1;
  }

  let suffixLen = 0;
  while (
    suffixLen < minLen - prefixLen &&
    oldDoc.lines[oldDoc.lines.length - 1 - suffixLen] ===
      newDoc.lines[newDoc.lines.length - 1 - suffixLen]
  ) {
    suffixLen += 1;
  }

  const oldStart = prefixLen;
  const oldEnd = oldDoc.lines.length - suffixLen;
  const newStart = prefixLen;
  const newEnd = newDoc.lines.length - suffixLen;
  const oldChanged = oldEnd - oldStart;
  const newChanged = newEnd - newStart;

  if (oldChanged === 0 && newChanged === 0) return [];

  if (oldChanged === 0) {
    const text = patchText(
      newDoc.lines.slice(newStart, newEnd),
      newEnd === newDoc.lines.length ? newDoc.trailingNewline : false,
      newDoc.eol,
    );
    if (oldDoc.lines.length === 0) return [{ op: 'append', text }];
    if (oldStart === 0) {
      return [{
        op: 'insert_before',
        line: 1,
        anchor_hash: await sha256Short(oldDoc.lines[0]),
        text,
      }];
    }
    if (oldStart >= oldDoc.lines.length) {
      return [{
        op: 'append',
        expected_tail_hash: await sha256Short(oldDoc.lines[oldDoc.lines.length - 1]),
        text,
      }];
    }
    return [{
      op: 'insert_after',
      line: oldStart,
      anchor_hash: await sha256Short(oldDoc.lines[oldStart - 1]),
      text,
    }];
  }

  if (newChanged === 0) {
    const startLine = oldStart + 1;
    const endLine = oldEnd;
    return [{
      op: 'delete_lines',
      start_line: startLine,
      end_line: endLine,
      expected_hash: await linesHash(oldDoc.lines, startLine, endLine),
    }];
  }

  const startLine = oldStart + 1;
  const endLine = oldEnd;
  return [{
    op: 'replace_lines',
    start_line: startLine,
    end_line: endLine,
    expected_hash: await linesHash(oldDoc.lines, startLine, endLine),
    text: patchText(
      newDoc.lines.slice(newStart, newEnd),
      newEnd === newDoc.lines.length ? newDoc.trailingNewline : false,
      newDoc.eol,
    ),
  }];
}

export function generateRequestId(): string {
  return crypto.randomUUID();
}
