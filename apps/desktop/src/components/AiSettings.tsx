/**
 * Choosing a model, and saying where its key is.
 *
 * There are two places a key can be, and the panel offers both. Typed in
 * here, it goes to the operating system's keychain — Keychain Services, the
 * Credential Manager, the Secret Service — and the settings file records the
 * word `keychain` and nothing else. Named as an environment variable, it
 * stays wherever the shell put it.
 *
 * A key travels one way. Once saved, this panel can tell you that there is
 * one and cannot tell you what it is: there is no call that reads one back.
 */

import { useEffect, useState } from 'react';

import * as api from '../api';
import type {
  AiSettings as Settings,
  AiView,
  KeychainView,
  ProviderConfig,
} from '../types';

interface Props {
  onDone: (view: AiView) => void;
  onCancel: () => void;
  onError: (reason: string) => void;
}

/** The kinds a provider can be, and what each one needs.  */
const KINDS = [
  { kind: 'anthropic', label: 'Anthropic', variable: 'ANTHROPIC_API_KEY', url: false },
  { kind: 'openAi', label: 'OpenAI', variable: 'OPENAI_API_KEY', url: false },
  { kind: 'openRouter', label: 'OpenRouter', variable: 'OPENROUTER_API_KEY', url: true },
  { kind: 'ollama', label: 'Ollama (on this machine)', variable: '', url: true },
  {
    kind: 'openAiCompatible',
    label: 'Anything OpenAI-compatible',
    variable: '',
    url: true,
  },
] as const;

const BLANK: ProviderConfig = { kind: 'anthropic', model: '' };

/** Which of the three key arrangements a configuration is using. */
type Where = 'keychain' | 'environment' | 'none';

function whereIsTheKey(config: ProviderConfig): Where {
  if (config.key === undefined) return 'none';
  return config.key.in === 'keychain' ? 'keychain' : 'environment';
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

  /** Moves the key to a different place, or to nowhere. */
  const putTheKey = (next: Where) => {
    setTyped('');
    if (next === 'keychain') setConfig({ ...config, key: { in: 'keychain' } });
    else if (next === 'environment') {
      setConfig({ ...config, key: { in: 'environment', name: kind.variable } });
    } else {
      // Removed rather than set to undefined: the field is absent from the
      // settings file, which is what "needs no key" looks like on disk.
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
    // Saying the key is in the keychain and putting none there would build a
    // provider that fails on its first question, with the reason two menus
    // away from where it was caused.
    if (where === 'keychain' && typed.trim() === '' && !saved) {
      onError(`there is no key saved for “${label}” yet — type one in`);
      return;
    }
    if (
      where === 'environment' &&
      (config.key?.in === 'environment' ? config.key.name : '') === ''
    ) {
      onError('name the environment variable the key is in');
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
            <option value="keychain" disabled={!keychain.available}>
              Saved in this computer&rsquo;s keychain
            </option>
            <option value="environment">In an environment variable</option>
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
              The key goes into the keychain this computer already runs. HyperLab&rsquo;s
              settings file records only that it is there, so it can still be copied into
              a bug report.
            </p>
          </>
        )}

        {where === 'environment' && (
          <>
            <label className="properties__row">
              <span className="properties__name">Key is in</span>
              <input
                className="properties__value"
                value={config.key?.in === 'environment' ? config.key.name : ''}
                spellCheck={false}
                placeholder={
                  kind.variable === '' ? 'the variable holding it' : kind.variable
                }
                onChange={(event) =>
                  setConfig({
                    ...config,
                    key: { in: 'environment', name: event.target.value },
                  })
                }
              />
            </label>
            <p className="assistant__note">
              The name of a variable, not a key. HyperLab reads it when the provider is
              built and never writes its value anywhere.
            </p>
          </>
        )}

        {keychain.problem !== undefined && (
          <p className="assistant__note">
            {keychain.problem}. An environment variable works.
          </p>
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
