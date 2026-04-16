import {
  Show,
  children as resolveChildren,
  createContext,
  createSignal,
  useContext,
  type Accessor,
  type JSX,
} from 'solid-js';

interface TabsContextValue {
  active: Accessor<string>;
  setActive: (id: string) => void;
  register: (id: string) => void;
  ids: Accessor<string[]>;
}

const TabsContext = createContext<TabsContextValue>();

function useTabs(): TabsContextValue {
  const ctx = useContext(TabsContext);
  if (!ctx) throw new Error('Tabs sub-components must be used inside <Tabs>');
  return ctx;
}

export interface TabsProps {
  defaultTab: string;
  onChange?: (id: string) => void;
  children: JSX.Element;
}

/** Headless tab controller.  The context tracks the active id plus the ordered
 *  list of registered tab ids — the latter drives keyboard navigation so
 *  consumers don't have to pass the same list twice. */
export const Tabs = (props: TabsProps): JSX.Element => {
  const [active, setActiveSig] = createSignal(props.defaultTab);
  const [ids, setIds] = createSignal<string[]>([]);

  const setActive = (id: string) => {
    if (id === active()) return;
    setActiveSig(id);
    props.onChange?.(id);
  };

  const register = (id: string) => {
    setIds((list) => (list.includes(id) ? list : [...list, id]));
  };

  return (
    <TabsContext.Provider value={{ active, setActive, register, ids }}>
      <div class="tabs-root">{props.children}</div>
    </TabsContext.Provider>
  );
};

export const TabList = (props: { children: JSX.Element }): JSX.Element => {
  const ctx = useTabs();

  const onKeyDown = (e: KeyboardEvent) => {
    const list = ctx.ids();
    if (list.length === 0) return;
    const idx = list.indexOf(ctx.active());
    let next: number | null = null;
    if (e.key === 'ArrowRight') next = (idx + 1) % list.length;
    else if (e.key === 'ArrowLeft') next = (idx - 1 + list.length) % list.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = list.length - 1;
    if (next === null) return;
    e.preventDefault();
    const target = list[next]!;
    ctx.setActive(target);
    // Move focus to the newly active tab so keyboard users keep flow.
    queueMicrotask(() => {
      const btn = document.getElementById(`tab-${target}`);
      btn?.focus();
    });
  };

  return (
    <div class="tab-list" role="tablist" onKeyDown={onKeyDown}>
      {props.children}
    </div>
  );
};

export interface TabProps {
  id: string;
  children: JSX.Element;
}

export const Tab = (props: TabProps): JSX.Element => {
  const ctx = useTabs();
  ctx.register(props.id);
  const isActive = () => ctx.active() === props.id;
  return (
    <button
      id={`tab-${props.id}`}
      type="button"
      role="tab"
      class="tab"
      classList={{ active: isActive() }}
      aria-selected={isActive()}
      aria-controls={`tabpanel-${props.id}`}
      tabIndex={isActive() ? 0 : -1}
      onClick={() => ctx.setActive(props.id)}
    >
      {props.children}
    </button>
  );
};

export interface TabPanelProps {
  id: string;
  children: JSX.Element;
}

export const TabPanel = (props: TabPanelProps): JSX.Element => {
  const ctx = useTabs();
  const resolved = resolveChildren(() => props.children);
  return (
    <Show when={ctx.active() === props.id}>
      <div
        id={`tabpanel-${props.id}`}
        role="tabpanel"
        class="tab-panel"
        aria-labelledby={`tab-${props.id}`}
      >
        {resolved()}
      </div>
    </Show>
  );
};
