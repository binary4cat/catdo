import { Plugin, PluginKey } from '@milkdown/kit/prose/state';
import { Decoration, DecorationSet } from '@milkdown/kit/prose/view';
import type { Node as ProsemirrorNode } from '@milkdown/kit/prose/model';

export function normalizeWikiLinks(markdown: string): string {
  return markdown.replace(/\\?\[\\?\[([^\]\n]+?)\\?\]\\?\]/g, '[[$1]]');
}

export function normalizeMarkdown(markdown: string): string {
  return normalizeWikiLinks(markdown)
    .replace(/^\s*<br\s*\/?>\s*\n?/i, '')
    .replace(
      /!\[[^\]\n]*\]\((_assets\/Pasted_image_[^)\n]+)\)/g,
      '![]($1)',
    );
}

// ProseMirror plugin for WikiLink decorations and click handling
export function createWikiLinkPlugin(
  onNavigate: (target: string) => void,
): Plugin {
  const key = new PluginKey('wikilink-decoration');

  return new Plugin({
    key,
    props: {
      handleClick(view, pos, event) {
        const target = event.target as HTMLElement;
        if (target.classList?.contains('wiki-link')) {
          const linkTarget = target.getAttribute('data-target');
          if (linkTarget) {
            onNavigate(linkTarget);
            return true;
          }
        }
        return false;
      },
      decorations(state) {
        const { doc } = state;
        const decorations: Decoration[] = [];
        const wikiLinkRegex = /\[\[([^\]]+)\]\]/g;

        doc.descendants((node: ProsemirrorNode, pos: number) => {
          if (!node.isText || !node.text) return;
          const text = node.text;
          let match: RegExpExecArray | null;
          
          while ((match = wikiLinkRegex.exec(text)) !== null) {
            const start = pos + match.index;
            const end = start + match[0].length;
            const content = match[1]!;
            const parts = content.split('|');
            const target = parts[0]!.trim();
            const display = parts.length > 1 ? parts[1]!.trim() : target;
            
            decorations.push(
              Decoration.inline(start, end, {
                class: 'wiki-link',
                'data-target': target,
                nodeName: 'span',
              }),
            );
          }
        });

        return DecorationSet.create(doc, decorations);
      },
    },
  });
}

// WikiLink autocomplete popup manager
export class WikiLinkAutocomplete {
  private popup: HTMLDivElement | null = null;
  private items: string[] = [];
  private selectedIndex = 0;
  private onSelect: ((path: string) => void) | null = null;
  private getAllPaths: () => string[] = () => [];

  constructor(getAllPaths: () => string[]) {
    this.getAllPaths = getAllPaths;
  }

  show(anchor: { left: number; top: number; bottom: number }, query: string, onSelect: (path: string) => void) {
    this.onSelect = onSelect;
    const allPaths = this.getAllPaths();
    const q = query.toLowerCase();
    this.items = q
      ? allPaths.filter((p) => {
          const name = p.split('/').pop() || p;
          return name.toLowerCase().includes(q) || p.toLowerCase().includes(q);
        })
      : allPaths;
    this.items = this.items.slice(0, 10); // max 10 results
    this.selectedIndex = 0;

    if (this.items.length === 0) {
      this.hide();
      return;
    }

    if (!this.popup) {
      this.popup = document.createElement('div');
      this.popup.className = 'wikilink-autocomplete';
      this.popup.style.cssText = `
        position: fixed;
        z-index: 9999;
        background: var(--bg);
        border: 1px solid var(--border);
        border-radius: 6px;
        box-shadow: 0 4px 12px rgba(0,0,0,0.15);
        max-height: 240px;
        overflow-y: auto;
        min-width: 240px;
        padding: 4px 0;
      `;
      document.body.appendChild(this.popup);
    }

    this.popup.style.left = `${anchor.left}px`;
    this.popup.style.top = `${anchor.bottom + 4}px`;
    this.renderItems();
  }

  private renderItems() {
    if (!this.popup) return;
    this.popup.innerHTML = '';
    this.items.forEach((item, idx) => {
      const div = document.createElement('div');
      const name = item.split('/').pop() || item;
      div.textContent = name;
      div.title = item;
      div.style.cssText = `
        padding: 6px 12px;
        cursor: pointer;
        font-size: 13px;
        color: var(--text);
        ${idx === this.selectedIndex ? 'background: var(--hover-bg);' : ''}
      `;
      div.addEventListener('mouseenter', () => {
        this.selectedIndex = idx;
        this.renderItems();
      });
      div.addEventListener('mousedown', (e) => {
        e.preventDefault();
        this.selectCurrent();
      });
      this.popup!.appendChild(div);
    });
  }

  handleKeyDown(e: KeyboardEvent): boolean {
    if (!this.popup) return false;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      this.selectedIndex = Math.min(this.selectedIndex + 1, this.items.length - 1);
      this.renderItems();
      return true;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      this.selectedIndex = Math.max(this.selectedIndex - 1, 0);
      this.renderItems();
      return true;
    }
    if (e.key === 'Enter' || e.key === 'Tab') {
      e.preventDefault();
      this.selectCurrent();
      return true;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      this.hide();
      return true;
    }
    return false;
  }

  private selectCurrent() {
    if (this.items.length > 0 && this.onSelect) {
      this.onSelect(this.items[this.selectedIndex]!);
    }
    this.hide();
  }

  hide() {
    if (this.popup) {
      this.popup.remove();
      this.popup = null;
    }
  }

  get isVisible() {
    return this.popup !== null;
  }
}

// ProseMirror plugin for [[ trigger detection
export function createWikiLinkInputPlugin(
  autocomplete: WikiLinkAutocomplete,
  onComplete: (from: number, to: number, linkText: string) => void,
): Plugin {
  const key = new PluginKey('wikilink-input');

  return new Plugin({
    key,
    props: {
      handleKeyDown(view, event) {
        if (autocomplete.isVisible) {
          return autocomplete.handleKeyDown(event);
        }
        return false;
      },
      handleTextInput(view, from, to, text) {
        const { state } = view;
        // Check if we just typed the second `[`
        if (text === '[') {
          const before = state.doc.textBetween(Math.max(0, from - 1), from);
          if (before === '[') {
            // We have `[[` — start autocomplete tracking
            // The actual autocomplete tracking is done in the update handler
          }
        }
        return false;
      },
    },
    view(editorView) {
      return {
        update(view, prevState) {
          const { state } = view;
          const { selection } = state;
          if (!selection.empty) {
            autocomplete.hide();
            return;
          }

          const pos = selection.$head;
          const textBefore = pos.parent.textBetween(
            0,
            pos.parentOffset,
            undefined,
            '\ufffc',
          );

          // Look for [[ without closing ]]
          const openIdx = textBefore.lastIndexOf('[[');
          if (openIdx === -1 || textBefore.indexOf(']]', openIdx) !== -1) {
            autocomplete.hide();
            return;
          }

          const query = textBefore.slice(openIdx + 2);
          // Don't show autocomplete if query contains newline
          if (query.includes('\n')) {
            autocomplete.hide();
            return;
          }

          // Get cursor coordinates for positioning
          const coords = view.coordsAtPos(selection.from);
          autocomplete.show(
            { left: coords.left, top: coords.top, bottom: coords.bottom },
            query,
            (selectedPath: string) => {
              // Replace the [[ + query with the full [[path|name]]
              const name = selectedPath.split('/').pop()?.replace(/\.md$/, '') || selectedPath;
              const linkText = `[[${selectedPath}|${name}]]`;
              const startPos = pos.before() + openIdx + 1; // absolute pos of first [
              const endPos = selection.from;
              
              const tr = state.tr.replaceWith(
                startPos,
                endPos,
                state.schema.text(linkText),
              );
              view.dispatch(tr);
              autocomplete.hide();
            },
          );
        },
        destroy() {
          autocomplete.hide();
        },
      };
    },
  });
}
