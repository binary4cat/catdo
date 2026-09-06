import { $prose } from '@milkdown/kit/utils';
import { createWikiLinkPlugin, createWikiLinkInputPlugin, WikiLinkAutocomplete } from './wikilink';
import type { Plugin } from '@milkdown/kit/prose/state';

export function buildWikiLinkPlugins(
  onNavigate: (target: string) => void,
  getAllPaths: () => string[],
) {
  const autocomplete = new WikiLinkAutocomplete(getAllPaths);

  const decoPlugin = $prose(() => createWikiLinkPlugin(onNavigate));
  const inputPlugin = $prose(() => createWikiLinkInputPlugin(autocomplete, () => {}));

  return { decoPlugin, inputPlugin, autocomplete };
}
