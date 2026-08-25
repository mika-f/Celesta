// A minimal react-reconciler host config. The "DOM" here is a plain tree of
// {type, props, children} instances — there is no real host platform, just
// data collected for render.ts to walk into a Scene. Mutation mode is used
// because it is the simplest mode to implement correctly; this renderer is
// not performance-sensitive (one composition, re-rendered once per exported
// frame, not sixty times a second against a live display).

import Reconciler from 'react-reconciler';
import { DefaultEventPriority, LegacyRoot } from 'react-reconciler/constants';

export interface HostNode {
  type: string;
  props: Record<string, unknown>;
  children: HostNode[];
}

export interface RootContainer {
  children: HostNode[];
}

type NoTimeout = -1;
const NO_TIMEOUT: NoTimeout = -1;
const NO_CONTEXT = {};

const hostConfig: Reconciler.HostConfig<
  string,
  Record<string, unknown>,
  RootContainer,
  HostNode,
  never,
  never,
  never,
  HostNode,
  typeof NO_CONTEXT,
  Record<string, unknown>,
  never,
  ReturnType<typeof setTimeout>,
  NoTimeout
> = {
  supportsMutation: true,
  supportsPersistence: false,
  supportsHydration: false,
  isPrimaryRenderer: true,

  scheduleTimeout: setTimeout,
  cancelTimeout: clearTimeout,
  noTimeout: NO_TIMEOUT,

  getRootHostContext: () => NO_CONTEXT,
  getChildHostContext: (parentHostContext) => parentHostContext,
  prepareForCommit: () => null,
  resetAfterCommit: () => {},

  // <text>'s string/number children stay on its `children` prop (like DOM
  // `textContent`) instead of becoming separate child instances, so
  // render.ts can read them back with `extractText`. Every other host type
  // only has element children, never bare text.
  shouldSetTextContent: (type) => type === 'text',
  createTextInstance: () => {
    throw new Error(
      'bare text is only supported inside <Text>; other elements only accept element children',
    );
  },

  createInstance(type, props) {
    return { type, props, children: [] };
  },
  appendInitialChild(parent, child) {
    parent.children.push(child);
  },
  finalizeInitialChildren: () => false,

  appendChildToContainer(container, child) {
    container.children.push(child);
  },
  appendChild(parent, child) {
    parent.children.push(child);
  },
  insertBefore(parent, child, beforeChild) {
    const index = parent.children.indexOf(beforeChild);
    parent.children.splice(index === -1 ? parent.children.length : index, 0, child);
  },
  insertInContainerBefore(container, child, beforeChild) {
    const index = container.children.indexOf(beforeChild);
    container.children.splice(index === -1 ? container.children.length : index, 0, child);
  },
  removeChild(parent, child) {
    const index = parent.children.indexOf(child);
    if (index !== -1) {
      parent.children.splice(index, 1);
    }
  },
  removeChildFromContainer(container, child) {
    const index = container.children.indexOf(child);
    if (index !== -1) {
      container.children.splice(index, 1);
    }
  },
  clearContainer(container) {
    container.children = [];
  },

  prepareUpdate: () => ({}),
  commitUpdate(instance, _updatePayload, _type, _oldProps, newProps) {
    instance.props = newProps;
  },

  getPublicInstance: (instance) => instance,
  preparePortalMount: () => {},
  detachDeletedInstance: () => {},

  // Event/DevTools integration this renderer has no use for (there is no
  // real host platform to dispatch DOM-style events against), but the
  // @types/react-reconciler 0.28 HostConfig type marks them required even
  // though the runtime treats them as optional.
  getCurrentEventPriority: () => DefaultEventPriority,
  getInstanceFromNode: () => null,
  beforeActiveInstanceBlur: () => {},
  afterActiveInstanceBlur: () => {},
  prepareScopeUpdate: () => {},
  getInstanceFromScope: () => null,
};

export const HostReconciler = Reconciler(hostConfig);

export function createRoot(): {
  container: RootContainer;
  root: ReturnType<typeof HostReconciler.createContainer>;
} {
  const container: RootContainer = { children: [] };
  const root = HostReconciler.createContainer(
    container,
    LegacyRoot,
    null,
    false,
    null,
    '',
    (error) => {
      throw error instanceof Error ? error : new Error(String(error));
    },
    null,
  );
  return { container, root };
}
