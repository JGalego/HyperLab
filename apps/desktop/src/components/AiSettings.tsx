/**
 * Choosing a model.
 *
 * There is no box to paste a key into, and that is deliberate rather than
 * unfinished: a provider is configured with the *name* of an environment
 * variable, so the settings file can be copied into a bug report and the key
 * stays wherever the operating system keeps it.
 */

import { useEffect, useState } from 'react';

import * as api from '../api';
import type { AiSettings as Settings, AiView, ProviderConfig } from '../types';

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

export function AiSettings({ onDone, onCancel, onError }: Props) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [name, setName] = useState('');
  const [config, setConfig] = useState<ProviderConfig>(BLANK);

  useEffect(() => {
    api.aiSettings().then(
      (loaded) => {
        setSettings(loaded);
        const first = loaded.defaultProvider ?? Object.keys(loaded.providers)[0];
        if (first !== undefined && loaded.providers[first] !== undefined) {
          setName(first);
          setConfig(loaded.providers[first]);
        }
      },
      (reason: unknown) => onError(String(reason)),
    );
  }, [onError]);

  if (settings === null) return null;

  const chosen = KINDS.find((entry) => entry.kind === config.kind) ?? KINDS[0];

  const save = () => {
    const label = name.trim();
    if (label === '' || config.model.trim() === '') {
      onError('a provider needs a name and a model');
      return;
    }
    const next: Settings = {
      defaultProvider: label,
      providers: { ...settings.providers, [label]: config },
    };
    api.aiSaveSettings(next).then(onDone, (reason: unknown) => onError(String(reason)));
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

        {chosen.url && (
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
          <span className="properties__name">Key is in</span>
          <input
            className="properties__value"
            value={config.apiKeyEnv ?? ''}
            spellCheck={false}
            placeholder={chosen.variable === '' ? 'no key needed' : chosen.variable}
            onChange={(event) => setConfig({ ...config, apiKeyEnv: event.target.value })}
          />
        </label>

        <p className="assistant__note">
          The name of an environment variable, not a key. HyperLab reads the variable when
          it starts and never writes its value anywhere.
        </p>

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
