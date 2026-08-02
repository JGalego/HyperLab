/**
 * Choosing a model, and saying where its key is — the browser edition.
 *
 * A sibling of the desktop's panel with the key arrangements a page really
 * has. There is no OS keychain and no environment here: a key typed in goes
 * into this browser's storage, under this site's origin, and the settings
 * record the word `keychain` and nothing else — the same shape the desktop
 * writes, so a settings file means the same thing on both.
 *
 * A key travels one way. Once saved, this panel can tell you that there is
 * one and cannot tell you what it is: there is no call that reads one back.
 * When a question is asked, the key goes from this browser straight to the
 * provider named below — this site is static files, with no server of its
 * own for a key to visit.
 */

import { useEffect, useState } from 'react';

import type {
  AiSettings as Settings,
  AiView,
  KeychainView,
  ProviderConfig,
} from '../../../desktop/src/types';

import * as api from '../api';

interface Props {
  onDone: (view: AiView) => void;
  onCancel: () => void;
  onError: (reason: string) => void;
}

/** The kinds a provider can be, and what each one needs. */
const KINDS = [
  { kind: 'anthropic', label: 'Anthropic', url: false },
  { kind: 'openAi', label: 'OpenAI', url: false },
  { kind: 'openRouter', label: 'OpenRouter', url: true },
  { kind: 'ollama', label: 'Ollama (on this machine)', url: true },
  { kind: 'openAiCompatible', label: 'Anything OpenAI-compatible', url: true },
  { kind: 'mock', label: 'Mock (no network, for trying it out)', url: false },
] as const;

const BLANK: ProviderConfig = { kind: 'anthropic', model: '' };

/** Which of the two key arrangements a configuration is using. */
type Where = 'keychain' | 'none';

function whereIsTheKey(config: ProviderConfig): Where {
  return config.key === undefined ? 'none' : 'keychain';
}

export function AiSettings({ onDone, onCancel, onError }: Props) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [keychain, setKeychain] = useState<KeychainView | null>(null);
  const [name, setName] = useState('');
  const [config, setConfig] = useState<ProviderConfig>(BLANK);
  // Held only while it is being typed, and cleared the moment it is saved.
  const [typed, setTyped] = useState('');

  useEffect(() => {
    Promise.all([api.aiSettings(), api.aiKeychain()]).then(
      ([loaded, store]) => {
        setSettings(loaded);
        setKeychain(store);
        const first = loaded.defaultProvider ?? Object.keys(loaded.providers)[0];
        const chosen = first === undefined ? undefined : loaded.providers[first];
        if (first !== undefined && chosen !== undefined) {
          setName(first);
          setConfig(chosen);
        }
      },
      (reason: unknown) => onError(String(reason)),
    );
  }, [onError]);

  if (settings === null || keychain === null) return null;

  const kind = KINDS.find((entry) => entry.kind === config.kind) ?? KINDS[0];
  const where = whereIsTheKey(config);
  const saved = keychain.holding.includes(name.trim());

  /** Moves the key into this browser, or to nowhere. */
  const putTheKey = (next: Where) => {
    setTyped('');
    if (next === 'keychain') setConfig({ ...config, key: { in: 'keychain' } });
    else {
      // Removed rather than set to undefined: the field is absent from the
      // stored settings, which is what "needs no key" looks like.
      const without = { ...config };
      delete without.key;
      setConfig(without);
    }
  };

  const forget = () => {
    api
      .aiForgetKey(name.trim())
      .then(setKeychain, (reason: unknown) => onError(String(reason)));
  };

  const save = () => {
    const label = name.trim();
    if (label === '' || config.model.trim() === '') {
      onError('a provider needs a name and a model');
      return;
    }
    // Saying the key is saved and putting none there would build a provider
    // that fails on its first question, with the reason two menus away from
    // where it was caused.
    if (where === 'keychain' && typed.trim() === '' && !saved) {
      onError(`there is no key saved for “${label}” yet — type one in`);
      return;
    }

    const next: Settings = {
      defaultProvider: label,
      providers: { ...settings.providers, [label]: config },
    };
    // The key first: a provider rebuilt before its key is in place reports
    // itself broken, and the panel would close over the top of that.
    const stored =
      where === 'keychain' && typed.trim() !== ''
        ? api.aiSetKey(label, typed.trim())
        : Promise.resolve(keychain);

    stored
      .then((store) => {
        setKeychain(store);
        setTyped('');
        return api.aiSaveSettings(next);
      })
      .then(onDone, (reason: unknown) => onError(String(reason)));
  };

  return (
    <div className="dialog__scrim" role="presentation">
      <div
        className="dialog dialog--wide"
        role="dialog"
        aria-modal="true"
        aria-label="AI settings"
      >
        <p className="dialog__message">Which model should the assistant use?</p>

        <label className="properties__row">
          <span className="properties__name">Called</span>
          <input
            className="properties__value"
            value={name}
            spellCheck={false}
            placeholder="work"
            onChange={(event) => setName(event.target.value)}
          />
        </label>

        <label className="properties__row">
          <span className="properties__name">Provider</span>
          <select
            className="properties__value"
            value={config.kind}
            onChange={(event) => setConfig({ ...config, kind: event.target.value })}
          >
            {KINDS.map((entry) => (
              <option key={entry.kind} value={entry.kind}>
                {entry.label}
              </option>
            ))}
          </select>
        </label>

        <label className="properties__row">
          <span className="properties__name">Model</span>
          <input
            className="properties__value"
            value={config.model}
            spellCheck={false}
            placeholder="the model's name, as the provider spells it"
            onChange={(event) => setConfig({ ...config, model: event.target.value })}
          />
        </label>

        {kind.url && (
          <label className="properties__row">
            <span className="properties__name">Address</span>
            <input
              className="properties__value"
              value={config.baseUrl ?? ''}
              spellCheck={false}
              placeholder="http://localhost:11434/v1"
              onChange={(event) => setConfig({ ...config, baseUrl: event.target.value })}
            />
          </label>
        )}

        <label className="properties__row">
          <span className="properties__name">Key</span>
          <select
            className="properties__value"
            value={where}
            onChange={(event) => putTheKey(event.target.value as Where)}
          >
            <option value="keychain">Saved in this browser</option>
            <option value="none">Not needed</option>
          </select>
        </label>

        {where === 'keychain' && (
          <>
            <div className="properties__row">
              <span className="properties__name">
                {saved ? 'Replace with' : 'Type it in'}
              </span>
              <input
                className="properties__value"
                type="password"
                value={typed}
                autoComplete="off"
                spellCheck={false}
                aria-label="API key"
                placeholder={saved ? 'a key is saved — leave blank to keep it' : 'sk-…'}
                onChange={(event) => setTyped(event.target.value)}
              />
              {saved && (
                <button type="button" className="tool" onClick={forget}>
                  Forget
                </button>
              )}
            </div>
            <p className="assistant__note">
              The key stays in this browser&rsquo;s storage and is sent only to the
              provider above. Anyone who can open this browser profile could read it, so
              on a shared computer use Forget when you are done.
            </p>
          </>
        )}

        <div className="dialog__buttons">
          <button type="button" className="tool" onClick={onCancel}>
            Cancel
          </button>
          <button type="button" className="tool" onClick={save}>
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
