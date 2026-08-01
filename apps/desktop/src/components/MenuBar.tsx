/** The menu bar. */

import { useEffect, useRef, useState } from 'react';

import type { StackView } from '../types';

/** One line of a menu. `null` draws a separator. */
export type MenuEntry = null | {
  label: string;
  shortcut?: string;
  disabled?: boolean;
  run: () => void;
};

interface Props {
  view: StackView;
  menus: { title: string; entries: MenuEntry[] }[];
}

/**
 * A menu bar in the classic arrangement: the application's name, then the
 * menus, then the state of the document on the right.
 */
export function MenuBar({ view, menus }: Props) {
  const [open, setOpen] = useState<string | null>(null);
  const bar = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open === null) return undefined;
    const dismiss = (event: MouseEvent) => {
      if (!bar.current?.contains(event.target as Node)) setOpen(null);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(null);
    };
    window.addEventListener('mousedown', dismiss);
    window.addEventListener('keydown', escape);
    return () => {
      window.removeEventListener('mousedown', dismiss);
      window.removeEventListener('keydown', escape);
    };
  }, [open]);

  return (
    <div className="menubar" ref={bar}>
      <span className="menubar__title">HyperLab</span>
      {menus.map((menu) => (
        <div className="menu" key={menu.title}>
          <button
            type="button"
            className="menu__button"
            aria-expanded={open === menu.title}
            onClick={() => setOpen(open === menu.title ? null : menu.title)}
            onMouseEnter={() => open !== null && setOpen(menu.title)}
          >
            {menu.title}
          </button>
          {open === menu.title && (
            <ul className="menu__items">
              {menu.entries.map((entry, index) =>
                entry === null ? (
                  <li className="menu__separator" key={`separator-${index}`} />
                ) : (
                  <li key={entry.label}>
                    <button
                      type="button"
                      className="menu__item"
                      disabled={entry.disabled ?? false}
                      onClick={() => {
                        setOpen(null);
                        entry.run();
                      }}
                    >
                      <span>{entry.label}</span>
                      {entry.shortcut && (
                        <span className="menu__shortcut">{entry.shortcut}</span>
                      )}
                    </button>
                  </li>
                ),
              )}
            </ul>
          )}
        </div>
      ))}
      <span className="menubar__spacer" />
      <span className="menubar__title">
        {view.stackName}
        {view.dirty ? ' •' : ''}
      </span>
    </div>
  );
}
